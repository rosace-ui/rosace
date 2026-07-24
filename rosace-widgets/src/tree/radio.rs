use std::sync::Arc;
use rosace_core::types::{Point, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// A single radio button (ring + filled dot) — brought to the Quality Bar
/// (matches the `Switch`/`Checkbox` exemplars). Single-select is the app's
/// job: bind several radios to one `Atom<T>` and compare; distinct behavior
/// from `Checkbox` (mutually exclusive), so not a duplicate.
///
/// - **States** — selected/unselected · hover · pressed (dot dips) ·
///   focus-visible (ring) · disabled (dimmed, inert).
/// - **Motion** — the dot *pops in* (scales while the ring recolors); the
///   hover/press/focus state-layer halo fades on its own channel.
/// - **Theming** — ring/dot from `outline`→`primary` tokens; overridable.
/// - **A11y** — `Role::Radio` + selected value + optional label.
/// - **Interactive-by-identity** — always owns its hit region.
pub struct Radio {
    selected: bool,
    disabled: bool,
    label: Option<String>,
    size: f32,
    font_size: f32,
    color: Option<Color>,
    on_select: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Radio {
    pub fn new(selected: bool) -> Self {
        Self { selected, disabled: false, label: None, size: 20.0, font_size: 13.0, color: None, on_select: None }
    }
    pub fn size(mut self, s: f32) -> Self { self.size = s; self.font_size = s * 0.65; self }
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn label(mut self, l: impl Into<String>) -> Self { self.label = Some(l.into()); self }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    pub fn disabled_if(mut self, c: bool) -> Self { if c { self.disabled = true; } self }
    pub fn on_select(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f)); self
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl Widget for Radio {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        let label_w = self.label.as_ref()
            .map(|l| l.len() as f32 * self.font_size * 0.6 + 10.0)
            .unwrap_or(0.0);
        Size { width: self.size + label_w, height: self.size.max(self.font_size * 1.4) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let mut sem = super::Semantics::new(rosace_core::Role::Radio)
            .value(if self.selected { "selected" } else { "not selected" });
        if let Some(l) = &self.label { sem = sem.label(l); }
        ctx.semantics(sem);

        // Interactive-by-identity: always own the hit region.
        match (&self.on_select, self.disabled) {
            (Some(cb), false) => ctx.register_hit(Arc::clone(cb)),
            _ => ctx.register_hit(Arc::new(|| {})),
        }
        let focused = !self.disabled && ctx.focus_node().is_focused();
        let hovered = !self.disabled && ctx.hovered();
        let pressed = !self.disabled && ctx.pressed();

        // Channels: 0=select progress, 1=halo, 2=press dip.
        let t = ctx.animate_channel(0, if self.selected { 1.0 } else { 0.0 }, 0.0);
        let halo_t = if pressed { 0.16 } else if focused { 0.12 } else if hovered { 0.08 } else { 0.0 };
        let halo = ctx.animate_channel(1, halo_t, 0.0);
        let press = ctx.animate_channel(2, if pressed { 1.0 } else { 0.0 }, 0.0);

        let colors = ctx.theme.colors.clone();
        let accent = self.color.unwrap_or_else(|| ctx.tc(colors.primary));
        let outline = ctx.tc(colors.outline);
        let label_color = ctx.tc(colors.on_surface);
        let dim = if self.disabled { 0.4 } else { 1.0 };

        let bs = self.size;
        let cx = ctx.rect.origin.x + bs / 2.0;
        let cy = ctx.rect.origin.y + ctx.rect.size.height / 2.0;
        let center = Point { x: cx, y: cy };

        // State-layer halo.
        if halo > 0.001 {
            ctx.fill_circle(center, bs * 0.5 + 7.0, with_alpha(super::lerp_color(outline, accent, t), halo));
        }

        // Ring (outline→accent as it selects).
        let ring = super::lerp_color(outline, accent, t);
        ctx.fill_arc(center, bs / 2.0 - 1.5, 2.0, 0.0, 360.0, with_alpha(ring, dim));

        // Inner dot: pops in (scale 0→1) and dips slightly on press.
        if t > 0.01 {
            let dot_r = (bs / 4.0) * t * (1.0 - press * 0.12);
            ctx.fill_circle(center, dot_r, with_alpha(accent, dim));
        }

        // Focus ring.
        if focused {
            ctx.fill_arc(center, bs / 2.0 + 3.0, 2.0, 0.0, 360.0, with_alpha(accent, 0.9));
        }

        // Label.
        if let Some(label) = &self.label {
            let line_h = ctx.font.line_height(self.font_size);
            let ty = ((ctx.rect.size.height - line_h) / 2.0).max(0.0);
            ctx.text(label, bs + 10.0, ty, with_alpha(label_color, dim), self.font_size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::Rect;
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
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 20.0, height: 20.0 } },
            &font, rosace_theme::built_in::dark_theme(), tree,
        );
        Radio::new(selected).paint(&mut ctx);
        rec.finish().commands
    }

    #[test]
    #[ignore] // FAMILY_PNG=/path cargo test -p rosace-widgets control_family_showcase -- --ignored --nocapture
    fn control_family_showcase() {
        use super::super::app::WidgetApp;
        use super::super::{Column, Switch, Checkbox, Slider};
        use crate::EdgeInsets;
        let out = std::env::var("FAMILY_PNG").unwrap_or_else(|_| "control_family.png".to_string());
        let panel = |dark: bool| {
            let col = Column::new().spacing(20.0).padding(EdgeInsets::all(26.0))
                .child(Switch::new(true))
                .child(Checkbox::new(true).label("Checkbox"))
                .child(Slider::new(0.6).width(200.0))
                .child(Radio::new(true).label("Selected"))
                .child(Radio::new(false).label("Unselected"));
            let app = WidgetApp::new(260, 260);
            if dark { app.dark() } else { app.light() }.render_png(&col)
        };
        std::fs::write(&out, panel(true)).unwrap();
        std::fs::write(out.replace(".png", "_light.png"), panel(false)).unwrap();
        println!("wrote {out}");
    }

    #[test]
    fn selected_draws_an_inner_dot() {
        assert!(paint(true).iter().any(|c| matches!(c, DrawCommand::FillCircle { .. })),
            "a selected radio has a filled dot");
    }

    #[test]
    fn unselected_has_no_inner_dot_but_still_a_ring() {
        let cmds = paint(false);
        assert!(cmds.iter().any(|c| matches!(c, DrawCommand::FillArc { .. })), "the ring is always drawn");
        assert!(!cmds.iter().any(|c| matches!(c, DrawCommand::FillCircle { .. })),
            "an unselected radio has no dot (t=0)");
    }
}
