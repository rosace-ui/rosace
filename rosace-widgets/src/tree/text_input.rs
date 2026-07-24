use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_render::{Color, DrawCommand, FontWeight};
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;
use super::text_edit::{
    char_byte_offset, char_count, grapheme_boundaries, style_runs, CursorShape, CursorStyle,
    EditController, EditableDecl, LineLayout, SpanFn, TextLayoutSnapshot,
};

/// A single-line text input field.
///
/// Real keyboard editing (D112/Phase 28 Step 1): click to focus, type,
/// arrow-key navigation, Shift+arrow selection, Home/End, Cmd/Ctrl+A
/// select-all, Cmd/Ctrl+C/X/V clipboard — all dispatched by the engine
/// against this widget's persistent render-tree node
/// (`PaintCtx::register_editable`/`text_edit`), not by this `paint(&self)`
/// call itself (which can't mutate anything). This widget stays a
/// CONTROLLED component, the same convention `Slider`/`Switch`/`Checkbox`
/// already use: the app owns the true `String` (typically a `ctx.state`
/// atom), passes it in via `.value()`, and gets edits back via
/// `.on_change()`. What this widget's own render-tree node persists is
/// only the ephemeral editing chrome (caret position, selection).
pub struct TextInput {
    pub value: String,
    pub placeholder: String,
    pub focused: bool,
    pub obscure: bool,
    pub width: Option<f32>,
    pub height: f32,
    pub font_size: f32,
    pub radius: f32,
    background: Option<Color>,
    border_color: Option<Color>,
    focus_color: Option<Color>,
    on_change: Option<Arc<dyn Fn(String) + Send + Sync>>,
    controller: Option<EditController>,
    spans: Option<Arc<SpanFn>>,
    cursor_style: Option<CursorStyle>,
    keyboard_type: rosace_core::KeyboardType,
    field: Option<rosace_forms::FormField>,
    filters: Vec<super::text_edit::InputFilter>,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            placeholder: String::from("Type here..."),
            focused: false,
            obscure: false,
            width: None,
            height: 36.0,
            font_size: 11.0,
            radius: 6.0,
            background: None,
            border_color: None,
            focus_color: None,
            on_change: None,
            controller: None,
            spans: None,
            cursor_style: None,
            keyboard_type: rosace_core::KeyboardType::default(),
            field: None,
            filters: Vec::new(),
        }
    }
    pub fn value(mut self, v: impl Into<String>) -> Self { self.value = v.into(); self }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self { self.placeholder = p.into(); self }
    /// Seed this input as focused on its FIRST paint only (a one-shot
    /// request, not a per-frame re-request — see `PaintCtx::focus_node_seeded`).
    /// Real, persistent focus state now lives on this widget's own
    /// [`rosace_a11y::FocusNode`] (auto-created, zero wiring required),
    /// driven by click/Tab from then on.
    pub fn focused(mut self) -> Self { self.focused = true; self }
    pub fn obscure(mut self) -> Self { self.obscure = true; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    /// Box fill color (a fixed dark tone if unset — kept as the
    /// long-standing default rather than switched to a theme token, since
    /// that would visibly shift every existing app using this widget).
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Unfocused border color.
    pub fn border(mut self, c: Color) -> Self { self.border_color = Some(c); self }
    /// Focused border color (also thickens slightly, unchanged).
    pub fn focus_color(mut self, c: Color) -> Self { self.focus_color = Some(c); self }
    /// Report edits — called by the engine's key/click dispatch whenever
    /// this input's value actually changes (typing, paste, cut). Without
    /// this, the input still accepts keystrokes/selection/caret movement
    /// (all real, all repainted) but the displayed value never advances,
    /// same "controlled with no listener does nothing" behavior as
    /// `Slider`/`Switch` today.
    pub fn on_change(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }
    /// Attach a programmatic [`EditController`] (D116) — app-constructed
    /// and passed in (the `FocusNode` precedent), reachable from OUTSIDE
    /// the widget tree entirely (a toolbar button's `on_press` has no
    /// access to this field's render-tree node otherwise). Optional: most
    /// fields never need one.
    pub fn controller(mut self, c: EditController) -> Self {
        self.controller = Some(c);
        self
    }
    /// The markdown/syntax-highlighting seam (D116 Step 5): a tokenizer
    /// that inspects the current value (and, when available, the char
    /// range that changed since the last call — `None` on the first call)
    /// and returns styled [`super::text_edit::Span`]s. Never applied to
    /// an obscured (password) field. This crate never learns what
    /// markdown is — the app brings the tokenizer.
    pub fn spans(mut self, f: impl Fn(&str, Option<(usize, usize)>) -> Vec<super::text_edit::Span> + Send + Sync + 'static) -> Self {
        self.spans = Some(Arc::new(f));
        self
    }
    /// Per-field caret override — width/color/corner radius/blink rate/
    /// shape (`Bar`/`Block`/`Underline`/`Custom`). Falls back to the
    /// theme's `CursorStyle` extension (`ThemeData::ext`/`with_ext`, D105)
    /// if set, then to [`CursorStyle::default`].
    pub fn cursor_style(mut self, s: CursorStyle) -> Self {
        self.cursor_style = Some(s);
        self
    }
    /// Which OS soft-keyboard layout a mobile host should show while this
    /// field is focused (D116 Step 6) — `Email`/`Numeric`/`Url`/`Phone`.
    /// Pure data on desktop (no hardware keyboard has "layouts" to pick);
    /// real effect is a mobile-host FFI concern (`rosace_core::keyboard_type()`,
    /// polled the same way camera permission is).
    pub fn keyboard_type(mut self, kt: rosace_core::KeyboardType) -> Self {
        self.keyboard_type = kt;
        self
    }
    /// Bind this field to a [`rosace_forms::FormField`] (D116 Phase 28
    /// Step 8) — the primary way to wire form validation. Sets the
    /// widget's initial value from `f.get()` and installs an `on_change`
    /// that writes back into the field (`f.set(v)`) AND immediately
    /// re-validates (`f.validate()`), so an inline error caption below
    /// the field and a submit button's `.disabled_if(!form.is_valid())`
    /// both update live as the user types — not just on submit. Calling
    /// `.on_change()` again AFTER `.field()` overrides this binding;
    /// call `.field()` last if you need both.
    pub fn field(mut self, f: rosace_forms::FormField) -> Self {
        self.value = f.get();
        let bound = f.clone();
        self.on_change = Some(Arc::new(move |v| {
            bound.set(v);
            bound.validate();
        }));
        // Validate immediately (every rebuild, not just on edit) so
        // `is_valid()`/an inline error reflect the CURRENT value even
        // before the user has touched the field — an empty Required
        // field must gate a submit button from the very start, not only
        // after the user has typed something once.
        f.validate();
        self.field = Some(f);
        self
    }
    /// Input filters (D116 Step 8) — applied to every edit (typed chars,
    /// paste, IME commit, controller ops) before it reaches `on_change`.
    /// See [`super::text_edit::InputFilter`].
    pub fn filters(mut self, filters: Vec<super::text_edit::InputFilter>) -> Self {
        self.filters = filters;
        self
    }
}

impl Default for TextInput {
    fn default() -> Self { Self::new() }
}

/// Extra vertical space reserved for an inline validation error caption
/// below a bound field (D116 Step 8) — pushes following siblings down,
/// same as any real form's error text.
pub(super) const ERROR_ROW_H: f32 = 18.0;

impl Widget for TextInput {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        let show_error = self.field.as_ref().is_some_and(|f| f.is_touched() && !f.is_valid());
        Size {
            width:  self.width.unwrap_or(super::avail_w(constraints)),
            height: self.height + if show_error { ERROR_ROW_H } else { 0.0 },
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(rosace_core::Role::TextInput)
            .label(&self.placeholder).value(&self.value));

        // Own persistent FocusNode (D112) — click-to-focus/Tab work with
        // zero required wiring; `.focused()` seeds ONLY the first paint.
        let focus = ctx.focus_node_seeded(self.focused);
        ctx.register_focus(focus.clone());
        let is_focused = focus.is_focused();

        // The INPUT BOX only — `ctx.rect` may be taller than `self.height`
        // when an error caption is reserved below it (D116 Step 8).
        let full_rect = ctx.rect;
        let r = Rect { origin: full_rect.origin, size: Size { width: full_rect.size.width, height: self.height } };

        let bg = self.background.unwrap_or(Color::rgb(15, 16, 28));
        let border = if is_focused {
            self.focus_color.unwrap_or(Color::rgb(110, 75, 210))
        } else {
            self.border_color.unwrap_or(Color::rgb(32, 35, 58))
        };

        draw_rounded_rect_pub(ctx, r, bg, self.radius);
        ctx.stroke_rrect(r, self.radius, border, if is_focused { 1.5 } else { 1.0 });

        let has_value = !self.value.is_empty();
        let display = if has_value {
            if self.obscure {
                "•".repeat(self.value.chars().count())
            } else {
                self.value.clone()
            }
        } else {
            self.placeholder.clone()
        };

        let text_color = if has_value {
            Color::rgb(220, 222, 240)
        } else {
            Color::rgb(80, 85, 118)
        };

        let line_h = ctx.font.line_height(self.font_size);
        let ty = ((r.size.height - line_h) / 2.0).max(0.0);

        // Horizontal scroll-into-view (D116 Step 3 — the `scroll_x` field's
        // long-declared-but-unwired half). Shift the content left just
        // enough to keep the caret inside the field when the value
        // overflows the visible width, the way every single-line editor
        // does. Persisted through `set_scroll_x` so it survives repaints
        // instead of snapping back to 0; recomputed and clamped here each
        // paint against the CURRENT value/caret so deleting text lets the
        // content scroll back. Both the exported hit-test/IME layout below
        // and every glyph draw are shifted by the SAME `scroll_x`, so a
        // click, the caret, and the OS candidate window can never drift.
        let state = ctx.text_edit();
        let inset = 10.0_f32;
        let visible_w = (r.size.width - inset * 2.0).max(0.0);
        let cursor_byte = char_byte_offset(&display, state.cursor());
        let caret_rel = ctx.font.measure_text(&display[..cursor_byte], self.font_size);
        let total_w = ctx.font.measure_text(&display, self.font_size);
        let mut scroll_x = state.scroll_x;
        if is_focused {
            if caret_rel < scroll_x {
                scroll_x = caret_rel;
            } else if caret_rel > scroll_x + visible_w {
                scroll_x = caret_rel - visible_w;
            }
        }
        scroll_x = scroll_x.clamp(0.0, (total_w - visible_w).max(0.0));
        if (scroll_x - state.scroll_x).abs() > f32::EPSILON {
            ctx.set_scroll_x(scroll_x);
        }

        // TextLayoutSnapshot (D116 layer 3) — built ONCE per paint, from
        // grapheme boundaries of the REAL value (obscured or not; dots
        // don't preserve multi-char grapheme clustering, so boundaries
        // always come from `self.value`, only the measured WIDTHS come
        // from `display`). Reused below for both the exported hit-test
        // seam and this widget's own caret/selection rendering, so the
        // two can never drift out of sync. Every boundary is shifted by
        // `-scroll_x` so the whole layout (caret, selection, hit-test)
        // moves as one when the field scrolls horizontally.
        let boundary_chars = grapheme_boundaries(&self.value);
        let boundary_x: Vec<f32> = boundary_chars
            .iter()
            .map(|&c| {
                let bx = char_byte_offset(&display, c);
                r.origin.x + inset - scroll_x + ctx.font.measure_text(&display[..bx], self.font_size)
            })
            .collect();
        let layout = TextLayoutSnapshot {
            lines: vec![LineLayout {
                char_range: (0, char_count(&self.value)),
                y: r.origin.y + ty,
                height: line_h,
                boundary_chars,
                boundary_x,
            }],
        };

        ctx.register_editable(EditableDecl {
            value: self.value.clone(),
            rect: r,
            multiline: false,
            obscure: self.obscure,
            on_change: self.on_change.clone().unwrap_or_else(|| Arc::new(|_| {})),
            controller: self.controller.clone(),
            layout: layout.clone(),
            filters: self.filters.clone(),
        });

        // Clip content to the field so scrolled-out glyphs (and any
        // overflow past either edge) are trimmed at the box. Popped before
        // the selection handles + validation caption below, which live
        // OUTSIDE the field bounds and must not be clipped.
        ctx.record(DrawCommand::PushClip { rect: r });

        if is_focused {
            // Keep frames flowing for the caret blink WHILE focused only
            // (D111's lesson: default-on continuous animation everywhere
            // is exactly the mistake that phase corrected — this is
            // conditional on real focus, not a blanket default).
            super::request_animation();

            if let Some((sel_start, sel_end)) = state.selection_range() {
                // Tint behind the glyphs — color from the theme's
                // SelectionStyle (D105 ext; flat default = the exact
                // pre-themeable look). Handles + the glass lens paint
                // AFTER the text, further down.
                let sel_style = ctx.theme.ext::<super::SelectionStyle>().cloned().unwrap_or_default();
                let x0 = layout.x_of(sel_start).unwrap_or(r.origin.x + 10.0);
                let x1 = layout.x_of(sel_end).unwrap_or(x0);
                ctx.fill_rect(Rect {
                    origin: Point { x: x0, y: r.origin.y + ty },
                    size: Size { width: x1 - x0, height: line_h },
                }, sel_style.highlight);
            }

            // IME preedit underline (D116 Step 6) — the universal
            // CJK-composition convention, marking the uncommitted text
            // that's still being composed.
            if let Some((ims, ime_)) = state.ime_range {
                let x0 = layout.x_of(ims).unwrap_or(r.origin.x + 10.0);
                let x1 = layout.x_of(ime_).unwrap_or(x0);
                ctx.fill_rect(Rect {
                    origin: Point { x: x0, y: r.origin.y + ty + line_h - 1.0 },
                    size: Size { width: (x1 - x0).max(1.0), height: 1.5 },
                }, text_color);
            }

            // Report this field's caret rect to the platform (D116 Step
            // 6) so the OS's CJK candidate window anchors near it instead
            // of wherever it defaults to.
            let cursor_x = layout.x_of(state.cursor()).unwrap_or(r.origin.x + 10.0);
            rosace_core::set_ime_cursor_area(Some(Rect {
                origin: Point { x: cursor_x, y: r.origin.y + ty },
                size: Size { width: 2.0, height: line_h },
            }));
            rosace_core::set_keyboard_type(self.keyboard_type);
        }

        // Styled spans (D116 Step 5) — the markdown/syntax-highlighting
        // seam. Never applied to obscured fields or the placeholder.
        if let Some(spans_fn) = self.spans.as_ref().filter(|_| has_value && !self.obscure) {
            let spans = spans_fn(&self.value, state.last_edit_range);
            let line = &layout.lines[0];
            for (rs, re, color, weight) in style_runs(&spans, line.char_range.0, line.char_range.1) {
                if rs >= re { continue; }
                let rb = char_byte_offset(&self.value, rs);
                let reb = char_byte_offset(&self.value, re);
                let run_x = line.x_at(rs);
                ctx.record(DrawCommand::DrawText {
                    text: self.value[rb..reb].to_string(),
                    origin: Point { x: run_x, y: r.origin.y + ty },
                    color: color.unwrap_or(text_color),
                    px: self.font_size,
                    weight: weight.unwrap_or(FontWeight::Regular),
                });
            }
        } else {
            ctx.text(&display, inset - scroll_x, ty, text_color, self.font_size);
        }

        // Caret (content — inside the clip so it's trimmed at the field
        // edge when the value is scrolled). Drawn before PopClip; the
        // selection chrome below is popped OUT so its grips can hang past
        // the box bottom.
        if is_focused && state.selection_range().is_none() {
            // Caret hidden while a selection is active (matches the
            // selection-highlight-instead-of-caret convention every
            // desktop text field uses).
            let style = self.cursor_style.clone()
                .unwrap_or_else(|| ctx.theme.ext::<CursorStyle>().cloned().unwrap_or_default());
            let t = super::anim_clock() - state.last_edit_at;
            let blink_on = t < 0.5 || (((t - 0.5) / style.blink_rate) as i64 % 2 == 0);
            if blink_on {
                let line = &layout.lines[0];
                let cursor_x = line.x_at(state.cursor());
                let cy = r.origin.y + ty;
                paint_caret(ctx, &style, cursor_x, cy, line_h, self.font_size, line, state.cursor());
            }
        }

        // Content done — release the clip so the selection handles (which
        // hang below the line) and the validation caption (below the field)
        // paint unclipped.
        ctx.record(DrawCommand::PopClip);

        // Selection chrome ABOVE the glyphs (D116 Step 7 handles + the
        // theme-driven glass lens): drawn after the text so the lens can
        // sample — and magnify — the glyphs themselves.
        if is_focused {
            if let Some((sel_start, sel_end)) = state.selection_range() {
                let sel_style = ctx.theme.ext::<super::SelectionStyle>().cloned().unwrap_or_default();
                let x0 = layout.x_of(sel_start).unwrap_or(r.origin.x + 10.0);
                let x1 = layout.x_of(sel_end).unwrap_or(x0);
                // Grip anchors stay at the line BOTTOM in both kinds —
                // `engine.rs`'s handle_anchor targets exactly that point,
                // so restyling must not move the draggable position.
                let handle_y = r.origin.y + ty + line_h;
                match sel_style.kind {
                    super::SelectionKind::Flat => {
                        ctx.fill_circle(Point { x: x0, y: handle_y }, 4.0, sel_style.handle);
                        ctx.fill_circle(Point { x: x1, y: handle_y }, 4.0, sel_style.handle);
                    }
                    super::SelectionKind::Glass => {
                        // Lens sized to the MAGNIFIED selection: `sel × zoom`
                        // about the selection center, so every zoomed glyph
                        // fits exactly inside the pill and nothing renders
                        // past the end bars (found live: a center-zoom over
                        // an unscaled rect pushed edge glyphs beyond the
                        // handle — the "t after the cursor" bug). The
                        // shader's `center + (uv-center)/zoom` sampling then
                        // lands exactly on the unscaled selection window.
                        //
                        // ALL geometry (pill, bars, grips) comes from
                        // `SelectionStyle::glass_lens` — the engine's
                        // handle-drag grab uses the SAME function, so the
                        // visible lollipops and the draggable anchors can
                        // never drift apart.
                        let g = sel_style.glass_lens(x0, x1, r.origin.y + ty, line_h);
                        let lens = Rect {
                            origin: Point { x: g.rect.0, y: g.rect.1 },
                            size: Size { width: g.rect.2, height: g.rect.3 },
                        };
                        // Full stadium radius — the real liquid-glass pill.
                        ctx.shader_fill(
                            lens,
                            rosace_shader::builtin::SELECTION_LENS,
                            super::SelectionStyle::lens_uniforms(g.rect.3 / 2.0, sel_style.zoom),
                        );
                        // Lollipops: an end bar at each pill edge with its
                        // grip hanging directly beneath — one connected
                        // object, cursors always after the last magnified
                        // glyph.
                        for x in [g.bar_x.0, g.bar_x.1] {
                            ctx.fill_rrect(Rect {
                                origin: Point { x: x - 1.0, y: g.rect.1 + 3.0 },
                                size: Size { width: 2.0, height: g.rect.3 - 6.0 },
                            }, 1.0, sel_style.handle);
                            ctx.fill_circle(Point { x, y: g.grip_y }, 4.5, sel_style.handle);
                        }
                        let _ = handle_y;
                    }
                }
            }
        }

        // Inline validation error (D116 Step 8) — shown only once the
        // field has been touched (real desktop/mobile convention: don't
        // flash "required" on a form the user hasn't even reached yet).
        // `Role::Alert` matches the one other place this framework
        // surfaces an error message (`Toast::error`).
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

/// Render the caret per `style.shape` (D116 Step 5) — shared by
/// `TextInput` and `TextArea` so both stay pixel-consistent.
// Both call sites already hold these values as locals with these exact
// names — a params struct would only re-name them.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_caret(
    ctx: &mut PaintCtx, style: &CursorStyle, x: f32, y: f32, line_h: f32, font_size: f32,
    line: &LineLayout, cursor: usize,
) {
    match &style.shape {
        CursorShape::Bar => {
            ctx.fill_rrect(Rect {
                origin: Point { x, y: y + 1.0 },
                size: Size { width: style.width, height: (font_size - 1.0).max(4.0) },
            }, style.corner_radius, style.color);
        }
        CursorShape::Block => {
            let idx = line.boundary_chars.iter().position(|&c| c == cursor);
            let next_x = idx.and_then(|i| line.boundary_x.get(i + 1).copied()).unwrap_or(x + 8.0);
            let width = (next_x - x).max(2.0);
            ctx.fill_rrect(Rect {
                origin: Point { x, y },
                size: Size { width, height: line_h },
            }, style.corner_radius, Color::rgba(style.color.r, style.color.g, style.color.b, 90));
        }
        CursorShape::Underline => {
            let idx = line.boundary_chars.iter().position(|&c| c == cursor);
            let next_x = idx.and_then(|i| line.boundary_x.get(i + 1).copied()).unwrap_or(x + 8.0);
            let width = (next_x - x).max(2.0);
            ctx.fill_rect(Rect {
                origin: Point { x, y: y + line_h - 2.0 },
                size: Size { width, height: 2.0 },
            }, style.color);
        }
        CursorShape::Custom(painter) => {
            let rect = Rect {
                origin: Point { x, y: y + 1.0 },
                size: Size { width: style.width, height: (font_size - 1.0).max(4.0) },
            };
            painter(ctx, rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_render::{FontCache, PictureRecorder};
    use rosace_theme::built_in;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use crate::tree::RenderTree;

    fn line() -> LineLayout {
        LineLayout {
            char_range: (0, 3),
            y: 0.0,
            height: 20.0,
            boundary_chars: vec![0, 1, 2, 3],
            boundary_x: vec![10.0, 18.0, 26.0, 34.0],
        }
    }

    fn make_ctx<'a>(recorder: &'a mut PictureRecorder, font: &'a FontCache) -> PaintCtx<'a> {
        let theme = built_in::dark_theme();
        PaintCtx::root(
            recorder,
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 200.0, height: 60.0 } },
            font,
            theme,
            Rc::new(RefCell::new(RenderTree::new())),
        )
    }

    #[test]
    fn bar_shape_paints_a_thin_filled_rrect() {
        let font = FontCache::embedded();
        let mut recorder = PictureRecorder::new();
        let mut ctx = make_ctx(&mut recorder, &font);
        let style = CursorStyle::default();
        paint_caret(&mut ctx, &style, 10.0, 0.0, 20.0, 11.0, &line(), 1);
        let picture = recorder.finish();
        match picture.commands.last().expect("must record a paint command") {
            DrawCommand::FillRRect { rect, .. } => {
                assert!(rect.size.width < 3.0, "Bar must be thin, got width {}", rect.size.width);
            }
            other => panic!("expected FillRRect for Bar, got {other:?}"),
        }
    }

    #[test]
    fn block_shape_paints_a_wider_rect_spanning_to_the_next_glyph_boundary() {
        let font = FontCache::embedded();
        let mut recorder = PictureRecorder::new();
        let mut ctx = make_ctx(&mut recorder, &font);
        let style = CursorStyle { shape: CursorShape::Block, ..Default::default() };
        // Cursor at char 0 (x=10.0); next boundary (char 1) is at x=18.0.
        paint_caret(&mut ctx, &style, 10.0, 0.0, 20.0, 11.0, &line(), 0);
        let picture = recorder.finish();
        match picture.commands.last().expect("must record a paint command") {
            DrawCommand::FillRRect { rect, .. } => {
                assert_eq!(rect.size.width, 8.0, "Block must span to the next glyph boundary (18.0 - 10.0)");
            }
            other => panic!("expected FillRRect for Block, got {other:?}"),
        }
    }

    #[test]
    fn underline_shape_paints_a_thin_rect_at_the_bottom_of_the_line() {
        let font = FontCache::embedded();
        let mut recorder = PictureRecorder::new();
        let mut ctx = make_ctx(&mut recorder, &font);
        let style = CursorStyle { shape: CursorShape::Underline, ..Default::default() };
        paint_caret(&mut ctx, &style, 10.0, 0.0, 20.0, 11.0, &line(), 0);
        let picture = recorder.finish();
        match picture.commands.last().expect("must record a paint command") {
            DrawCommand::FillRect { rect, .. } => {
                assert_eq!(rect.origin.y, 18.0, "Underline must sit at the bottom of the line (y + line_h - 2.0)");
                assert_eq!(rect.size.height, 2.0);
            }
            other => panic!("expected FillRect for Underline, got {other:?}"),
        }
    }

    #[test]
    fn custom_shape_delegates_to_the_app_supplied_painter() {
        let font = FontCache::embedded();
        let mut recorder = PictureRecorder::new();
        let mut ctx = make_ctx(&mut recorder, &font);
        let called = Arc::new(AtomicBool::new(false));
        let called2 = called.clone();
        let style = CursorStyle {
            shape: CursorShape::Custom(Arc::new(move |_ctx, _rect| {
                called2.store(true, Ordering::SeqCst);
            })),
            ..Default::default()
        };
        paint_caret(&mut ctx, &style, 10.0, 0.0, 20.0, 11.0, &line(), 0);
        assert!(called.load(Ordering::SeqCst), "Custom shape must invoke the app's painter, not a built-in default");
    }

    #[test]
    fn background_border_focus_color_builders_do_not_change_layout_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(rosace_layout::Constraints::loose(400.0, 400.0), &font, &theme);
        let base = TextInput::new().width(200.0);
        let customized = TextInput::new().width(200.0)
            .background(Color::rgb(10, 10, 10))
            .border(Color::rgb(200, 0, 0))
            .focus_color(Color::rgb(0, 200, 0));
        assert_eq!(base.layout(&ctx), customized.layout(&ctx));
    }

    /// Paint a focused, selection-carrying input under `theme` and return
    /// whether the picture contains a `ShaderFill` (the glass lens).
    fn selection_paints_a_lens(theme: rosace_theme::ThemeData) -> bool {
        use super::super::text_edit::Selection;
        let font = FontCache::embedded();
        let mut recorder = PictureRecorder::new();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        tree.borrow_mut().node_mut(RenderTree::ROOT).text_edit.selection =
            Selection::range(0, 5);
        let mut ctx = PaintCtx::root(
            &mut recorder,
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 300.0, height: 40.0 } },
            &font,
            theme,
            tree,
        );
        TextInput::new().value("hello world").focused().paint(&mut ctx);
        let picture = recorder.finish();
        picture.commands.iter().any(|c| matches!(c, DrawCommand::ShaderFill { .. }))
    }

    #[test]
    fn glass_selection_theme_paints_the_magnifier_lens() {
        let theme = rosace_theme::built_in::dark_theme()
            .with_ext(super::super::SelectionStyle::glass());
        assert!(selection_paints_a_lens(theme), "glass theme must emit the lens ShaderFill");
    }

    #[test]
    fn default_theme_selection_stays_flat_with_no_lens() {
        assert!(
            !selection_paints_a_lens(rosace_theme::built_in::dark_theme()),
            "no SelectionStyle registered must keep the flat look — zero shader quads"
        );
    }

    /// Paint a focused, single-line `TextInput` whose value overflows the
    /// field with the caret at the END, and return the `scroll_x` written
    /// back into the tree plus whether the content was clip-bracketed.
    fn paint_overflowing(width: f32, value: &str, focused: bool) -> (f32, bool, bool) {
        use super::super::text_edit::Selection;
        let font = FontCache::embedded();
        let mut recorder = PictureRecorder::new();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let len = value.chars().count();
        // Collapsed caret at the very end — the overflow case.
        tree.borrow_mut().node_mut(RenderTree::ROOT).text_edit.selection =
            Selection::range(len, len);
        let mut ctx = PaintCtx::root(
            &mut recorder,
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width, height: 40.0 } },
            &font,
            rosace_theme::built_in::dark_theme(),
            tree.clone(),
        );
        let mut input = TextInput::new().value(value);
        if focused {
            input = input.focused();
        }
        input.paint(&mut ctx);
        let picture = recorder.finish();
        let scroll_x = tree.borrow().node(RenderTree::ROOT).text_edit.scroll_x;
        let has_push = picture.commands.iter().any(|c| matches!(c, DrawCommand::PushClip { .. }));
        let has_pop = picture.commands.iter().any(|c| matches!(c, DrawCommand::PopClip));
        (scroll_x, has_push, has_pop)
    }

    #[test]
    fn caret_at_end_of_overflowing_value_scrolls_content_left_and_clips() {
        // A long value in a narrow field, caret at the end: the content
        // MUST shift left (scroll_x > 0) so the caret stays visible, and
        // the glyphs MUST be clip-bracketed to the field box.
        let (scroll_x, has_push, has_pop) =
            paint_overflowing(100.0, "the quick brown fox jumps over the lazy dog", true);
        assert!(scroll_x > 0.0, "overflowing focused field must scroll left, got scroll_x={scroll_x}");
        assert!(has_push && has_pop, "content must be bracketed by PushClip/PopClip");
    }

    #[test]
    fn short_value_never_scrolls() {
        // A value that fits leaves scroll_x at 0 — no gratuitous shift.
        let (scroll_x, _, _) = paint_overflowing(300.0, "hi", true);
        assert_eq!(scroll_x, 0.0, "a value that fits must not scroll");
    }
}
