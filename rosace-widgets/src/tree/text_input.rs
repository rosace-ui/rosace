use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;
use super::text_edit::{char_byte_offset, EditableDecl};

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
}

impl Default for TextInput {
    fn default() -> Self { Self::new() }
}

impl Widget for TextInput {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size {
            width:  self.width.unwrap_or(super::avail_w(constraints)),
            height: self.height,
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

        let r = ctx.rect;
        ctx.register_editable(EditableDecl {
            value: self.value.clone(),
            rect: r,
            multiline: false,
            obscure: self.obscure,
            on_change: self.on_change.clone().unwrap_or_else(|| Arc::new(|_| {})),
        });

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

        if is_focused {
            // Keep frames flowing for the caret blink WHILE focused only
            // (D111's lesson: default-on continuous animation everywhere
            // is exactly the mistake that phase corrected — this is
            // conditional on real focus, not a blanket default).
            super::request_animation();

            let state = ctx.text_edit();
            if let Some((sel_start, sel_end)) = state.selection_range() {
                // Selection highlight — measured against the DISPLAY
                // string (obscured text measures dot widths, not the
                // hidden value's real widths).
                let bs = char_byte_offset(&display, sel_start);
                let be = char_byte_offset(&display, sel_end);
                let x0 = ctx.font.measure_text(&display[..bs], self.font_size);
                let x1 = ctx.font.measure_text(&display[..be], self.font_size);
                ctx.fill_rect(Rect {
                    origin: Point { x: r.origin.x + 10.0 + x0, y: r.origin.y + ty },
                    size: Size { width: x1 - x0, height: line_h },
                }, Color::rgba(110, 75, 210, 90));
            }

            ctx.text(&display, 10.0, ty, text_color, self.font_size);

            // Caret hidden while a selection is active (matches the
            // selection-highlight-instead-of-caret convention every
            // desktop text field uses).
            if state.selection_range().is_none() {
                let t = super::anim_clock() - state.last_edit_at;
                let blink_on = t < 0.5 || (((t - 0.5) / 0.53) as i64 % 2 == 0);
                if blink_on {
                    let cb = char_byte_offset(&display, state.cursor);
                    let cursor_x = ctx.font.measure_text(&display[..cb], self.font_size);
                    ctx.fill_rect(Rect {
                        origin: Point { x: r.origin.x + 10.0 + cursor_x, y: r.origin.y + ty + 1.0 },
                        size: Size { width: 1.5, height: (self.font_size - 1.0).max(4.0) },
                    }, Color::rgb(180, 160, 255));
                }
            }
        } else {
            ctx.text(&display, 10.0, ty, text_color, self.font_size);
        }
    }
}
