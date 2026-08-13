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
    /// `None` = read from the active theme's `typography.body_medium`
    /// (D127 "environment" track — see `Checkbox::resolved_font_size`'s doc
    /// for the reasoning).
    font_size: Option<f32>,
    color: Option<Color>,
    on_select: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Radio {
    pub fn new(selected: bool) -> Self {
        Self { selected, disabled: false, label: None, size: 20.0, font_size: None, color: None, on_select: None }
    }
    pub fn size(mut self, s: f32) -> Self { self.size = s; self.font_size = Some(s * 0.65); self }
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn label(mut self, l: impl Into<String>) -> Self { self.label = Some(l.into()); self }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    pub fn disabled_if(mut self, c: bool) -> Self { if c { self.disabled = true; } self }
    pub fn on_select(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f)); self
    }

    fn resolved_font_size(&self, theme: &rosace_theme::ThemeData) -> f32 {
        self.font_size.unwrap_or(theme.typography.body_medium.size)
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl Widget for Radio {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let font_size = self.resolved_font_size(ctx.theme);
        // Measured, not estimated — see `Checkbox::layout` for the reasoning.
        let label_w = self.label.as_ref()
            .map(|l| ctx.font.measure_text(l, font_size) + 10.0)
            .unwrap_or(0.0);
        // Both axes clear the minimum tap target (Quality Bar §6) — a 20px
        // radio was the smallest control in the library.
        Size {
            width:  (self.size + label_w).max(super::MIN_TAP_TARGET),
            height: self.size.max(font_size * 1.4).max(super::MIN_TAP_TARGET),
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let mut sem = super::SemanticsProps::new(rosace_core::Role::Radio)
            .value(if self.selected { "selected" } else { "not selected" });
        if let Some(l) = &self.label { sem = sem.label(l); }
        ctx.semantics(sem);
        let font_size = self.resolved_font_size(&ctx.theme);

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
        // Anchored left when a label follows it, centred otherwise.
        let cx = if self.label.is_some() {
            ctx.rect.origin.x + bs / 2.0
        } else {
            ctx.rect.origin.x + ctx.rect.size.width / 2.0
        };
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
            let line_h = ctx.font.line_height(font_size);
            let ty = ((ctx.rect.size.height - line_h) / 2.0).max(0.0);
            ctx.text(label, bs + 10.0, ty, with_alpha(label_color, dim), font_size);
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

    /// The label width used to be `len() * size * 0.6`, which cannot tell
    /// "WWWWW" from "iiiii" — same len(), very different widths — so a wide
    /// label silently overlapped whatever sat beside it. It also ignored
    /// `MediaQuery::text_scale`, since only `measure_text` applies that.
    #[test]
    fn label_width_is_measured_per_glyph_not_estimated_from_len() {
        let font = FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let c = rosace_layout::Constraints::loose(400.0, 400.0);
        let ctx = LayoutCtx::new(c, &font, &theme);

        let wide = Radio::new(false).label("WWWWW").layout(&ctx).width;
        let narrow = Radio::new(false).label("iiiii").layout(&ctx).width;
        assert!(wide > narrow, "same len(), different glyphs: {wide} vs {narrow}");

        let bare = Radio::new(false).layout(&ctx).width;
        assert!(narrow > bare, "a labelled control must be wider than a bare one");
    }


    /// Quality Bar §6: a control must reserve at least `MIN_TAP_TARGET`,
    /// even though its visual is much smaller. The extra is transparent
    /// padding — reserved in LAYOUT rather than by inflating the hit rect,
    /// because overlapping hit rects would let registration order decide
    /// which of two adjacent controls a tap lands on.
    #[test]
    fn an_unlabelled_control_still_reserves_a_full_tap_target() {
        let font = FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let c = rosace_layout::Constraints::loose(400.0, 400.0);
        let ctx = LayoutCtx::new(c, &font, &theme);

        let s = Radio::new(false).layout(&ctx);
        assert!(s.width >= crate::tree::MIN_TAP_TARGET, "width {} too small", s.width);
        assert!(s.height >= crate::tree::MIN_TAP_TARGET, "height {} too small", s.height);

        // A label makes it wider, never narrower.
        let labelled = Radio::new(false).label("Remember me").layout(&ctx);
        assert!(labelled.width > s.width);
        assert!(labelled.height >= crate::tree::MIN_TAP_TARGET);
    }

}
