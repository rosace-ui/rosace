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

        let bg = Color::rgb(15, 16, 28);
        let border = if is_focused { Color::rgb(110, 75, 210) } else { Color::rgb(32, 35, 58) };

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

        // TextLayoutSnapshot (D116 layer 3) — built ONCE per paint, from
        // grapheme boundaries of the REAL value (obscured or not; dots
        // don't preserve multi-char grapheme clustering, so boundaries
        // always come from `self.value`, only the measured WIDTHS come
        // from `display`). Reused below for both the exported hit-test
        // seam and this widget's own caret/selection rendering, so the
        // two can never drift out of sync.
        let boundary_chars = grapheme_boundaries(&self.value);
        let boundary_x: Vec<f32> = boundary_chars
            .iter()
            .map(|&c| {
                let bx = char_byte_offset(&display, c);
                r.origin.x + 10.0 + ctx.font.measure_text(&display[..bx], self.font_size)
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

        let state = ctx.text_edit();

        if is_focused {
            // Keep frames flowing for the caret blink WHILE focused only
            // (D111's lesson: default-on continuous animation everywhere
            // is exactly the mistake that phase corrected — this is
            // conditional on real focus, not a blanket default).
            super::request_animation();

            if let Some((sel_start, sel_end)) = state.selection_range() {
                let x0 = layout.x_of(sel_start).unwrap_or(r.origin.x + 10.0);
                let x1 = layout.x_of(sel_end).unwrap_or(x0);
                ctx.fill_rect(Rect {
                    origin: Point { x: x0, y: r.origin.y + ty },
                    size: Size { width: x1 - x0, height: line_h },
                }, Color::rgba(110, 75, 210, 90));

                // Draggable selection handles (D116 Step 7) — the touch
                // convention (also mouse-draggable: `engine.rs` grabs
                // whichever is nearest a MouseDown within
                // `HANDLE_HIT_RADIUS`). A small grip below each endpoint.
                let handle_y = r.origin.y + ty + line_h;
                ctx.fill_circle(Point { x: x0, y: handle_y }, 4.0, Color::rgb(180, 160, 255));
                ctx.fill_circle(Point { x: x1, y: handle_y }, 4.0, Color::rgb(180, 160, 255));
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
            ctx.text(&display, 10.0, ty, text_color, self.font_size);
        }

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
}
