use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_render::{Color, DrawCommand, FontWeight};
use super::{Widget, LayoutCtx, PaintCtx, ScrollAxes};
use super::container::draw_rounded_rect_pub;
use super::text_input::paint_caret;
use super::text_edit::{
    char_byte_offset, grapheme_boundaries, style_runs, CursorStyle, EditController, EditableDecl,
    LineLayout, SpanFn, TextLayoutSnapshot,
};

/// A multi-line, wrapped, virtualized-paint text field (D116 Step 4).
///
/// Built entirely on the same D116 core `TextInput` uses — `Transaction`/
/// `Selection`/`Command`/`EditController` are unchanged; `TextLayoutSnapshot`
/// already models "a list of visual lines" so `TextArea` just populates one
/// [`LineLayout`] per WRAPPED line instead of `TextInput`'s single entry.
/// Click/drag/double/triple-click dispatch in `engine.rs` is untouched —
/// it only ever talks to `editable.layout`, never knows which widget it's
/// looking at.
///
/// New in this widget: word-wrapping (`wrap_char_ranges` below), Enter
/// inserts a real newline (`engine.rs`, gated on `multiline`), Up/Down
/// cross wrapped lines with goal-column memory (`TextEditState::goal_x`),
/// and vertical scrolling via the same zero-wiring [`rosace_scroll::ScrollController`]
/// `ListView`/`ScrollView` use (D101) — wheel events mutate the
/// controller's atoms directly, never the render tree, so there's no
/// `!Sync`/`!Send` wall to work around here the way click dispatch has.
///
/// Paint is virtualized: every visual line's geometry is computed each
/// frame (needed for correct click-anywhere/goal-column behavior), but
/// only lines intersecting the current viewport actually emit paint
/// commands — a large document's PAINT cost stays bounded even though its
/// LAYOUT cost does not yet (a named follow-up, not this step's exit bar).
pub struct TextArea {
    pub value: String,
    pub placeholder: String,
    pub focused: bool,
    pub width: Option<f32>,
    pub height: f32,
    pub font_size: f32,
    pub radius: f32,
    on_change: Option<Arc<dyn Fn(String) + Send + Sync>>,
    controller: Option<EditController>,
    spans: Option<Arc<SpanFn>>,
    cursor_style: Option<CursorStyle>,
    field: Option<rosace_forms::FormField>,
    filters: Vec<super::text_edit::InputFilter>,
    show_scrollbar: bool,
    scrollbar_color: Color,
}

impl TextArea {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            placeholder: String::from("Type here..."),
            focused: false,
            width: None,
            height: 160.0,
            font_size: 11.0,
            radius: 6.0,
            on_change: None,
            controller: None,
            spans: None,
            cursor_style: None,
            field: None,
            filters: Vec::new(),
            show_scrollbar: true,
            scrollbar_color: Color::rgb(60, 65, 95),
        }
    }
    pub fn value(mut self, v: impl Into<String>) -> Self { self.value = v.into(); self }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self { self.placeholder = p.into(); self }
    pub fn focused(mut self) -> Self { self.focused = true; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn on_change(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }
    pub fn controller(mut self, c: EditController) -> Self {
        self.controller = Some(c);
        self
    }
    /// See `TextInput::spans` — same seam, same contract (D116 Step 5).
    pub fn spans(mut self, f: impl Fn(&str, Option<(usize, usize)>) -> Vec<super::text_edit::Span> + Send + Sync + 'static) -> Self {
        self.spans = Some(Arc::new(f));
        self
    }
    /// See `TextInput::cursor_style` — same seam, same contract.
    pub fn cursor_style(mut self, s: CursorStyle) -> Self {
        self.cursor_style = Some(s);
        self
    }
    /// See `TextInput::field` — same seam, same contract (D116 Step 8).
    pub fn field(mut self, f: rosace_forms::FormField) -> Self {
        self.value = f.get();
        let bound = f.clone();
        self.on_change = Some(Arc::new(move |v| {
            bound.set(v);
            bound.validate();
        }));
        // See `TextInput::field`'s identical eager-validate comment.
        f.validate();
        self.field = Some(f);
        self
    }
    /// See `TextInput::filters` — same seam, same contract.
    pub fn filters(mut self, filters: Vec<super::text_edit::InputFilter>) -> Self {
        self.filters = filters;
        self
    }
    /// Hide the vertical scroll-position thumb (see `ScrollView::no_scrollbar`
    /// — same convention). Content still scrolls; only the indicator is gone.
    pub fn no_scrollbar(mut self) -> Self { self.show_scrollbar = false; self }
    pub fn scrollbar_color(mut self, c: Color) -> Self { self.scrollbar_color = c; self }
}

impl Default for TextArea {
    fn default() -> Self { Self::new() }
}

/// Greedy word-wrap that preserves EXACT char offsets into `chars` (no
/// text is dropped or normalized) — every char index belongs to exactly
/// one returned `(start, end)` line range, so click/caret placement is
/// always well-defined. Hard breaks (`\n`) always start a new line,
/// including a trailing empty line when `chars` ends with `\n`.
///
/// Known limitation: a single word wider than `max_width` overflows its
/// line rather than hard-breaking mid-word — acceptable for prose, a
/// named follow-up for pathological input (Step 4 exit bar doesn't
/// require it).
fn wrap_char_ranges(chars: &[char], max_width: f32, measure: &dyn Fn(&str) -> f32) -> Vec<(usize, usize)> {
    let n = chars.len();
    let mut ranges = Vec::new();
    let mut para_start = 0usize;
    loop {
        let mut para_end = para_start;
        while para_end < n && chars[para_end] != '\n' { para_end += 1; }
        wrap_paragraph(chars, para_start, para_end, max_width, measure, &mut ranges);
        if para_end >= n { break; }
        para_start = para_end + 1; // skip the '\n'
        if para_start == n {
            ranges.push((n, n)); // value ends with '\n' -> trailing empty line
            break;
        }
    }
    if ranges.is_empty() { ranges.push((0, 0)); }
    ranges
}

fn wrap_paragraph(
    chars: &[char], start: usize, end: usize, max_width: f32,
    measure: &dyn Fn(&str) -> f32, ranges: &mut Vec<(usize, usize)>,
) {
    if start == end {
        ranges.push((start, end));
        return;
    }
    let mut line_start = start;
    let mut cursor = start;
    while cursor < end {
        // Extend to the end of the next token: a run of non-space chars
        // plus its trailing spaces (spaces stay attached to the word
        // BEFORE the break, matching every real editor's wrap).
        let mut tok_end = cursor;
        while tok_end < end && chars[tok_end] != ' ' { tok_end += 1; }
        while tok_end < end && chars[tok_end] == ' ' { tok_end += 1; }
        if tok_end == cursor { tok_end = end; }
        if cursor > line_start {
            let candidate: String = chars[line_start..tok_end].iter().collect();
            if measure(&candidate) > max_width {
                ranges.push((line_start, cursor));
                line_start = cursor;
            }
        }
        cursor = tok_end;
    }
    ranges.push((line_start, end));
}

impl Widget for TextArea {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        let show_error = self.field.as_ref().is_some_and(|f| f.is_touched() && !f.is_valid());
        Size {
            width: self.width.unwrap_or(super::avail_w(constraints)),
            height: self.height + if show_error { super::text_input::ERROR_ROW_H } else { 0.0 },
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(rosace_core::Role::TextInput)
            .label(&self.placeholder).value(&self.value));

        let focus = ctx.focus_node_seeded(self.focused);
        ctx.register_focus(focus.clone());
        let is_focused = focus.is_focused();

        // The scroll VIEWPORT only — `ctx.rect` may be taller than
        // `self.height` when an error caption is reserved below it
        // (D116 Step 8), same convention as `TextInput`.
        let full_rect = ctx.rect;
        let r = Rect { origin: full_rect.origin, size: Size { width: full_rect.size.width, height: self.height } };
        let bg = Color::rgb(15, 16, 28);
        let border = if is_focused { Color::rgb(110, 75, 210) } else { Color::rgb(32, 35, 58) };
        draw_rounded_rect_pub(ctx, r, bg, self.radius);
        ctx.stroke_rrect(r, self.radius, border, if is_focused { 1.5 } else { 1.0 });

        const PAD: f32 = 10.0;
        let line_h = ctx.font.line_height(self.font_size);
        let max_w = (r.size.width - PAD * 2.0).max(1.0);
        let has_value = !self.value.is_empty();

        let chars: Vec<char> = self.value.chars().collect();
        let ranges = wrap_char_ranges(&chars, max_w, &|s: &str| ctx.font.measure_text(s, self.font_size));
        let n_lines = ranges.len();
        let content_h = n_lines as f32 * line_h;
        // The scrollable extent is the text PLUS the top and bottom padding
        // it's drawn inside (lines render at `PAD + i*line_h - scroll_y`).
        // Using bare `content_h` here left the last line's bottom exactly
        // PAD px past the clip at max scroll — a real clipped-last-line
        // bug caught live.
        let content_extent = content_h + PAD * 2.0;

        let ctrl = ctx.scroll_controller();
        let vp_s = [r.size.width, r.size.height];
        if ctrl.viewport_size.get() != vp_s { ctrl.viewport_size.set(vp_s); }
        let cs = [r.size.width, content_extent];
        if ctrl.content_size.get() != cs { ctrl.content_size.set(cs); }

        let max_scroll = (content_extent - r.size.height).max(0.0);
        let mut scroll_y = ctrl.offset.get()[1].clamp(0.0, max_scroll);

        let state = ctx.text_edit();
        let cursor = state.cursor();
        let cursor_line = ranges.iter().position(|&(s, e)| cursor >= s && cursor <= e).unwrap_or(0);

        // Scroll-into-view (D116 Step 4) — only meaningful while focused,
        // and only when the caret actually MOVED since the last chase
        // (typing, arrows, click). Chasing on every focused paint fought
        // the user's wheel input: the caret-blink animation repaints
        // constantly while focused, so with the caret on a bottom line
        // every wheel-up was snapped straight back within a frame ("no
        // scrolling at the bottom"), and a mid-document caret clamped
        // scrolling to a viewport-sized window around itself.
        if is_focused && state.scrolled_cursor != Some(cursor) {
            ctx.set_scrolled_cursor(Some(cursor));
            let cursor_y = cursor_line as f32 * line_h;
            if cursor_y < scroll_y {
                scroll_y = cursor_y;
            } else if cursor_y + line_h + PAD * 2.0 > scroll_y + r.size.height {
                // `+ PAD * 2.0`: the caret line must clear the bottom clip
                // edge with its padding, mirroring `content_extent` above.
                scroll_y = cursor_y + line_h + PAD * 2.0 - r.size.height;
            }
            scroll_y = scroll_y.clamp(0.0, max_scroll);
        }
        if ctrl.offset.get()[1] != scroll_y {
            ctrl.offset.set([ctrl.offset.get()[0], scroll_y]);
        }

        // `- PAD`: a line whose bottom pokes into the top padding band is
        // still partially visible and must paint (the clip trims it).
        let first_visible = (((scroll_y - PAD) / line_h).floor().max(0.0)) as usize;
        let last_visible = (((scroll_y + r.size.height) / line_h).ceil() as usize).min(n_lines);

        let boundaries = grapheme_boundaries(&self.value);
        let text_color = if has_value { Color::rgb(220, 222, 240) } else { Color::rgb(80, 85, 118) };

        // Styled spans (D116 Step 5) — computed ONCE for the whole value,
        // sliced per-line below via `style_runs`. Never applied to the
        // placeholder (there's no real text to tokenize).
        let spans = if has_value {
            self.spans.as_ref().map(|f| f(&self.value, state.last_edit_range))
        } else {
            None
        };

        let cursor_style = self.cursor_style.clone()
            .unwrap_or_else(|| ctx.theme.ext::<CursorStyle>().cloned().unwrap_or_default());

        ctx.record(DrawCommand::PushClip { rect: r });

        let mut lines: Vec<LineLayout> = Vec::with_capacity(n_lines);
        for (i, &(ls, le)) in ranges.iter().enumerate() {
            let y = r.origin.y + PAD + i as f32 * line_h - scroll_y;
            let bchars: Vec<usize> = boundaries.iter().copied().filter(|&c| c >= ls && c <= le).collect();
            let lb = char_byte_offset(&self.value, ls);
            let bx: Vec<f32> = bchars.iter().map(|&c| {
                let cb = char_byte_offset(&self.value, c);
                r.origin.x + PAD + ctx.font.measure_text(&self.value[lb..cb], self.font_size)
            }).collect();
            lines.push(LineLayout { char_range: (ls, le), y, height: line_h, boundary_chars: bchars, boundary_x: bx });

            if i < first_visible || i >= last_visible { continue; }
            let ll = &lines[i];

            if has_value {
                if is_focused {
                    // Standard half-open interval overlap: this line spans
                    // [ls, le); a multi-line selection highlights the
                    // portion of EACH overlapping line separately —
                    // `x_at` already clamps an out-of-range endpoint to
                    // this line's own start/end boundary.
                    if let Some((sel_s, sel_e)) = state.selection_range() {
                        if sel_s < le && sel_e > ls {
                            let x0 = ll.x_at(sel_s);
                            let x1 = ll.x_at(sel_e);
                            if x1 > x0 {
                                ctx.fill_rect(Rect {
                                    origin: Point { x: x0, y },
                                    size: Size { width: x1 - x0, height: line_h },
                                }, Color::rgba(110, 75, 210, 90));
                            }
                            // Draggable selection handles (D116 Step 7) —
                            // only on the line that actually OWNS each
                            // endpoint (a multi-line selection's middle
                            // lines get none).
                            let handle_y = y + line_h;
                            if sel_s >= ls && sel_s <= le {
                                ctx.fill_circle(Point { x: x0, y: handle_y }, 4.0, Color::rgb(180, 160, 255));
                            }
                            if sel_e >= ls && sel_e <= le {
                                ctx.fill_circle(Point { x: x1, y: handle_y }, 4.0, Color::rgb(180, 160, 255));
                            }
                        }
                    }

                    // IME preedit underline (D116 Step 6) — same
                    // half-open overlap technique as the selection above.
                    if let Some((ims, ime_)) = state.ime_range {
                        if ims < le && ime_ > ls {
                            let x0 = ll.x_at(ims);
                            let x1 = ll.x_at(ime_);
                            if x1 > x0 {
                                ctx.fill_rect(Rect {
                                    origin: Point { x: x0, y: y + line_h - 1.0 },
                                    size: Size { width: x1 - x0, height: 1.5 },
                                }, text_color);
                            }
                        }
                    }
                }
                if let Some(spans) = &spans {
                    for (rs, re, color, weight) in style_runs(spans, ls, le) {
                        if rs >= re { continue; }
                        let rb = char_byte_offset(&self.value, rs);
                        let reb = char_byte_offset(&self.value, re);
                        ctx.record(DrawCommand::DrawText {
                            text: self.value[rb..reb].to_string(),
                            origin: Point { x: ll.x_at(rs), y },
                            color: color.unwrap_or(text_color),
                            px: self.font_size,
                            weight: weight.unwrap_or(FontWeight::Regular),
                        });
                    }
                } else {
                    let ub = char_byte_offset(&self.value, le);
                    let line_text = &self.value[lb..ub];
                    ctx.draw_text_at(line_text, Point { x: r.origin.x + PAD, y }, text_color, self.font_size);
                }
            }

            if is_focused && i == cursor_line {
                // Report this field's caret rect to the platform (D116
                // Step 6) so the OS's CJK candidate window anchors near
                // it — regardless of whether the caret itself is visibly
                // blinking this frame.
                let cx = ll.x_at(cursor);
                rosace_core::set_ime_cursor_area(Some(Rect {
                    origin: Point { x: cx, y },
                    size: Size { width: 2.0, height: line_h },
                }));

                if state.selection_range().is_none() {
                    let t = super::anim_clock() - state.last_edit_at;
                    let blink_on = t < 0.5 || (((t - 0.5) / cursor_style.blink_rate) as i64 % 2 == 0);
                    if blink_on {
                        paint_caret(ctx, &cursor_style, cx, y, line_h, self.font_size, ll, cursor);
                    }
                }
            }
        }

        if !has_value {
            ctx.text(&self.placeholder, PAD, PAD, text_color, self.font_size);
        }
        if is_focused {
            super::request_animation();
        }

        ctx.record(DrawCommand::PopClip);

        // Scroll-position thumb (same convention as `ScrollView` — drawn
        // AFTER PopClip so it isn't clipped, re-reading the offset fresh
        // rather than the `scroll_y` captured above, which predates this
        // frame's wheel/scroll-into-view update).
        if self.show_scrollbar {
            let fresh_y = ctrl.offset.get()[1].clamp(0.0, max_scroll);
            let ratio = (r.size.height / content_extent.max(1.0)).min(1.0);
            if ratio < 1.0 {
                let bar_h = r.size.height * ratio;
                let max_bar_y = r.origin.y + r.size.height - bar_h;
                let bar_y = (r.origin.y + (fresh_y / content_extent) * r.size.height)
                    .clamp(r.origin.y, max_bar_y.max(r.origin.y));
                ctx.fill_rect(Rect {
                    origin: Point { x: r.origin.x + r.size.width - 4.0, y: bar_y },
                    size: Size { width: 3.0, height: bar_h },
                }, self.scrollbar_color);
            }
        }

        ctx.register_editable(EditableDecl {
            value: self.value.clone(),
            rect: r,
            multiline: true,
            obscure: false,
            on_change: self.on_change.clone().unwrap_or_else(|| Arc::new(|_| {})),
            controller: self.controller.clone(),
            layout: TextLayoutSnapshot { lines },
            filters: self.filters.clone(),
        });

        let wheel = ctrl.clone();
        ctx.register_scroll_target(r, ScrollAxes::Y, Arc::new(move |_dx, dy| {
            wheel.scroll_by(0.0, -dy);
        }));

        // Inline validation error (D116 Step 8) — see `TextInput`'s
        // identical block for the touched/`Role::Alert` reasoning.
        if let Some(field) = &self.field {
            if field.is_touched() {
                if let Some(err) = field.errors().first() {
                    ctx.semantics(super::Semantics::new(rosace_core::Role::Alert).label(&err.message));
                    ctx.record(DrawCommand::DrawText {
                        text: err.message.clone(),
                        origin: Point { x: full_rect.origin.x + 2.0, y: r.origin.y + r.size.height + 2.0 },
                        color: Color::rgb(230, 90, 90),
                        px: 10.0,
                        weight: FontWeight::Regular,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges_of(s: &str, max_width: f32, char_w: f32) -> Vec<(usize, usize)> {
        let chars: Vec<char> = s.chars().collect();
        wrap_char_ranges(&chars, max_width, &|t: &str| t.chars().count() as f32 * char_w)
    }
    fn slice(s: &str, r: (usize, usize)) -> String {
        s.chars().skip(r.0).take(r.1 - r.0).collect()
    }

    #[test]
    fn short_text_that_fits_is_a_single_line() {
        let r = ranges_of("hello", 1000.0, 10.0);
        assert_eq!(r, vec![(0, 5)]);
    }

    #[test]
    fn empty_string_is_one_empty_line() {
        assert_eq!(ranges_of("", 1000.0, 10.0), vec![(0, 0)]);
    }

    #[test]
    fn long_text_wraps_at_a_word_boundary_covering_every_char_with_no_gaps() {
        // "aaaa bbbb cccc" at char_w=10, max_width=45 fits "aaaa " (50 >
        // 45 already for "aaaa b"... use generous width so exactly two
        // words fit per line): width for "aaaa bbbb " = 10*10=100.
        let s = "aaaa bbbb cccc";
        let r = ranges_of(s, 100.0, 10.0);
        assert!(r.len() >= 2, "must wrap into multiple lines, got {r:?}");
        // Every char index in [0, len) belongs to exactly one line —
        // ranges are contiguous with no gaps or overlaps.
        assert_eq!(r.first().unwrap().0, 0);
        assert_eq!(r.last().unwrap().1, s.chars().count());
        for w in r.windows(2) {
            assert_eq!(w[0].1, w[1].0, "line ranges must be contiguous: {r:?}");
        }
        // Reassembling every line's slice must reconstruct the original
        // text exactly (no characters dropped or duplicated).
        let rebuilt: String = r.iter().map(|&rg| slice(s, rg)).collect();
        assert_eq!(rebuilt, s);
    }

    #[test]
    fn explicit_newline_always_starts_a_new_line_regardless_of_width() {
        let r = ranges_of("ab\ncd", 1000.0, 10.0);
        assert_eq!(r, vec![(0, 2), (3, 5)], "the '\\n' itself (index 2) is consumed, not part of either line");
    }

    #[test]
    fn trailing_newline_produces_a_final_empty_line() {
        let r = ranges_of("ab\n", 1000.0, 10.0);
        assert_eq!(r, vec![(0, 2), (3, 3)]);
    }

    #[test]
    fn multiple_consecutive_newlines_produce_empty_lines_between_them() {
        let r = ranges_of("a\n\nb", 1000.0, 10.0);
        assert_eq!(r, vec![(0, 1), (2, 2), (3, 4)]);
    }
}
