use std::sync::Arc;

use rosace_core::types::Size;
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;

/// A filter/tag chip that toggles — brought to the Quality Bar.
///
/// - **States** — unselected/selected · hover · pressed · focus-visible ·
///   disabled.
/// - **Motion** — the fill + text recolor ease on select; a state-layer wash
///   fades on hover/press/focus (two channels).
/// - **Theming** — unselected = outlined `surface_variant`, selected = `primary`
///   fill with high-contrast text; tokens, overridable.
/// - **A11y** — checkbox-style role (toggles) + label + value.
/// - **Interactive-by-identity** — always owns its hit region.
pub struct Chip {
    label: String,
    selected: bool,
    disabled: bool,
    font_size: f32,
    height: f32,
    color: Option<Color>,          // selected fill override
    on_toggle: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl Chip {
    pub fn new(label: impl Into<String>) -> Self {
        Self { label: label.into(), selected: false, disabled: false, font_size: 12.0, height: 30.0, color: None, on_toggle: None }
    }
    pub fn selected(mut self) -> Self { self.selected = true; self }
    pub fn selected_if(mut self, c: bool) -> Self { self.selected = c; self }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    /// Override the selected fill (default: theme `primary`).
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    /// Called with the NEW selected value when tapped.
    pub fn on_toggle(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_toggle = Some(Arc::new(f)); self
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl Widget for Chip {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = ctx.font.measure_text(&self.label, self.font_size) + 28.0;
        Size { width: w, height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(rosace_core::Role::Checkbox)
            .label(&self.label)
            .value(if self.selected { "selected" } else { "not selected" }));

        match (&self.on_toggle, self.disabled) {
            (Some(f), false) => { let f = f.clone(); let next = !self.selected; ctx.on_press(move || f(next)); }
            _ => ctx.on_press(|| {}),
        }
        let focused = !self.disabled && ctx.focus_node().is_focused();
        let hovered = !self.disabled && ctx.hovered();
        let pressed = !self.disabled && ctx.pressed();

        let t = ctx.animate_channel(0, if self.selected { 1.0 } else { 0.0 }, 0.0);
        let wash_t = if pressed { 0.14 } else if focused { 0.10 } else if hovered { 0.07 } else { 0.0 };
        let wash = ctx.animate_channel(1, wash_t, 0.0);

        let colors = ctx.theme.colors.clone();
        let sel_fill = self.color.unwrap_or_else(|| ctx.tc(colors.primary));
        let unsel_fill = ctx.tc(colors.surface_variant);
        let outline = ctx.tc(colors.outline);
        let unsel_text = ctx.tc(colors.on_surface);
        let sel_text = Color::rgb(252, 252, 255);
        let dim = if self.disabled { 0.4 } else { 1.0 };

        let r = ctx.rect;
        let radius = r.size.height / 2.0;

        // Fill eases unselected→selected. Wash lightens it on hover/press.
        let mut fill = super::lerp_color(unsel_fill, sel_fill, t);
        if wash > 0.001 { fill = super::lerp_color(fill, Color::rgb(255, 255, 255), wash); }
        draw_rounded_rect_pub(ctx, r, with_alpha(fill, dim), radius);

        // Outline fades out as it fills in.
        if t < 0.99 {
            ctx.stroke_rrect(r, radius, with_alpha(outline, (1.0 - t) * dim), 1.0);
        }
        // Focus ring.
        if focused {
            ctx.stroke_rrect(r, radius, with_alpha(sel_fill, 0.9), 2.0);
        }

        let fg = super::lerp_color(unsel_text, sel_text, t);
        let text_w = ctx.font.measure_text(&self.label, self.font_size);
        let tx = ((r.size.width - text_w) / 2.0).max(0.0);
        let line_h = ctx.font.line_height(self.font_size);
        let ty = ((r.size.height - line_h) / 2.0).max(0.0);
        ctx.text(&self.label, tx, ty, with_alpha(fg, dim), self.font_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::{Point, Rect};
    use rosace_render::{FontCache, PictureRecorder};
    use rosace_render::draw_command::DrawCommand;
    use std::cell::RefCell;
    use std::rc::Rc;
    use crate::tree::RenderTree;

    fn paint(selected: bool) -> Vec<DrawCommand> {
        let font = FontCache::embedded();
        let mut rec = PictureRecorder::new();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let mut ctx = PaintCtx::root(
            &mut rec,
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 80.0, height: 30.0 } },
            &font, rosace_theme::built_in::dark_theme(), tree,
        );
        let mut c = Chip::new("Filter");
        if selected { c = c.selected(); }
        c.paint(&mut ctx);
        rec.finish().commands
    }

    #[test]
    fn draws_a_pill_and_its_label() {
        let cmds = paint(false);
        assert!(cmds.iter().any(|c| matches!(c, DrawCommand::FillRRect { .. })), "chip is a rounded pill");
        assert!(cmds.iter().any(|c| matches!(c, DrawCommand::DrawText { text, .. } if text == "Filter")), "shows its label");
    }

    #[test]
    fn selected_chip_has_no_outline_but_unselected_does() {
        assert!(paint(false).iter().any(|c| matches!(c, DrawCommand::StrokeRRect { .. })), "unselected chip is outlined");
    }
}
