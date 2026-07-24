use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;

/// A toggle switch — the reference "premium widget" (Q0 quality-bar exemplar).
///
/// Ships beautiful with zero config and stays fully overridable. It layers
/// every state a real toggle needs on top of one animated value:
///
/// - **Motion** — the thumb *slides* (theme-eased) between off/on; because the
///   thumb radius is derived from that same 0→1 position, it also *grows* as it
///   settles on, Material-3 style, for free (one node scalar, see `animate_to`).
/// - **States** — idle · hover · pressed (thumb stretches) · focus-visible
///   (ring + state-layer halo) · disabled (dimmed, inert) · on/off.
/// - **Elevation** — the thumb casts a soft drop shadow so it reads as a
///   physical knob above the track.
/// - **Theming** — track/thumb/halo all come from theme tokens (`primary`,
///   `surface_variant`, `on_primary`, `outline`, `shadow`); light+dark adapt
///   automatically. Any of them can be overridden per-instance.
/// - **A11y** — `Role::Switch` with an on/off value, a Tab-focusable node, and
///   an optional label for screen readers.
/// - **Interactive-by-identity** — always owns its hit region, wired or not, so
///   a tap can never fall through to a pannable surface behind it.
pub struct Switch {
    /// The current value.
    pub on: bool,
    disabled: bool,
    label: Option<String>,
    width: f32,
    height: f32,
    on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    on_color: Option<Color>,
    off_color: Option<Color>,
    thumb_color: Option<Color>,
}

impl Switch {
    pub fn new(on: bool) -> Self {
        Self {
            on,
            disabled: false,
            label: None,
            width: 44.0,
            height: 24.0,
            on_change: None,
            on_color: None,
            off_color: None,
            thumb_color: None,
        }
    }

    /// Called with the NEW value when the switch is toggled (D094).
    pub fn on_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    /// Non-interactive, dimmed, and inert (still absorbs the tap so nothing
    /// behind it reacts).
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    pub fn disabled_if(mut self, c: bool) -> Self { if c { self.disabled = true; } self }

    /// Accessibility label announced by screen readers alongside the on/off value.
    pub fn label(mut self, l: impl Into<String>) -> Self { self.label = Some(l.into()); self }

    /// Override the track size (default 44×24). Proportions stay tasteful at
    /// any reasonable size.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Override the on-track color (default: theme `primary`).
    pub fn on_color(mut self, c: Color) -> Self { self.on_color = Some(c); self }
    /// Override the off-track color (default: theme `surface_variant`).
    pub fn off_color(mut self, c: Color) -> Self { self.off_color = Some(c); self }
    /// Override the thumb color (default: theme `on_primary` on / `outline` off).
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl Widget for Switch {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(Size { width: self.width, height: self.height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // ── A11y ────────────────────────────────────────────────────────────
        let mut sem = super::Semantics::new(rosace_core::Role::Switch)
            .value(if self.on { "on" } else { "off" });
        if let Some(l) = &self.label { sem = sem.label(l); }
        ctx.semantics(sem);

        // ── Interactivity (identity) + keyboard focus ─────────────────────────
        // Always own the hit region; a disabled switch absorbs but does nothing.
        let toggle: Arc<dyn Fn() + Send + Sync> = match (&self.on_change, self.disabled) {
            (Some(f), false) => { let f = f.clone(); let next = !self.on; Arc::new(move || f(next)) }
            _ => Arc::new(|| {}),
        };
        ctx.register_hit(toggle);
        let focused = !self.disabled && ctx.focus_node().is_focused();

        // ── One animated value drives everything ─────────────────────────────
        let t = ctx.animate_to(if self.on { 1.0 } else { 0.0 }, 0.0); // 0=off, 1=on
        let hovered = !self.disabled && ctx.hovered();
        let pressed = !self.disabled && ctx.pressed();

        let colors = &ctx.theme.colors;
        let on_track  = self.on_color.unwrap_or_else(|| ctx.tc(colors.primary));
        let off_track = self.off_color.unwrap_or_else(|| ctx.tc(colors.surface_variant));
        // The knob is a bright, high-contrast disc in BOTH states (the iOS
        // model) — it reads as a physical thumb over any track colour, in
        // light or dark, and its drop shadow keeps it defined even on a
        // low-contrast off-track. Overridable via `.thumb_color(..)`.
        let thumb_c   = self.thumb_color.unwrap_or_else(|| Color::rgb(250, 250, 252));
        let outline   = ctx.tc(colors.outline);
        let shadow    = ctx.tc(colors.shadow);

        let dim = if self.disabled { 0.38 } else { 1.0 };
        let r = ctx.rect;
        let radius = r.size.height / 2.0;

        // ── Track (color eased between off/on; off-state gets an outline that
        //     fades out as it turns on) ────────────────────────────────────────
        let track = super::lerp_color(off_track, on_track, t);
        draw_rounded_rect_pub(ctx, r, with_alpha(track, dim), radius);
        if t < 1.0 {
            ctx.stroke_rrect(r, radius, with_alpha(outline, (1.0 - t) * dim), 1.0);
        }

        // ── Thumb geometry (position + radius both come from `t`) ────────────
        let pad = 2.0;
        let base_r = (r.size.height / 2.0) - pad;      // fills the track minus padding
        let off_r = base_r - 2.0;                       // smaller when off (M3)
        let on_r = base_r;                              // full when on
        let mut thumb_r = off_r + (on_r - off_r) * t;
        if pressed { thumb_r += 1.5; }                  // press "stretch"
        else if hovered { thumb_r += 0.5; }

        let cy = r.origin.y + r.size.height / 2.0;
        let off_cx = r.origin.x + pad + base_r;
        let on_cx = r.origin.x + r.size.width - pad - base_r;
        let cx = off_cx + (on_cx - off_cx) * t;

        // ── State layer: a translucent halo behind the thumb on hover/press/
        //     focus — the Material-3 signal that a control is live ─────────────
        let halo = if pressed { 0.16 } else if focused { 0.12 } else if hovered { 0.08 } else { 0.0 };
        if halo > 0.0 {
            let halo_color = super::lerp_color(outline, on_track, t);
            ctx.fill_circle(Point { x: cx, y: cy }, thumb_r + 8.0, with_alpha(halo_color, halo));
        }

        // ── Thumb elevation (soft drop shadow) then the thumb itself ─────────
        let d = thumb_r * 2.0;
        ctx.fill_shadow_rrect(
            Rect { origin: Point { x: cx - thumb_r, y: cy - thumb_r + 1.0 }, size: Size { width: d, height: d } },
            thumb_r,
            with_alpha(shadow, 0.28 * dim),
            4.0,
        );
        ctx.fill_circle(Point { x: cx, y: cy }, thumb_r, with_alpha(thumb_c, dim));

        // Keep the frame flowing while focused so the ring/halo stay live.
        if focused { ctx.request_animation(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_render::{FontCache, PictureRecorder};
    use rosace_render::draw_command::DrawCommand;
    use std::cell::RefCell;
    use std::rc::Rc;
    use crate::tree::RenderTree;

    fn paint_switch(on: bool, disabled: bool) -> Vec<DrawCommand> {
        let font = FontCache::embedded();
        let mut rec = PictureRecorder::new();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let mut ctx = PaintCtx::root(
            &mut rec,
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 44.0, height: 24.0 } },
            &font,
            rosace_theme::built_in::dark_theme(),
            tree,
        );
        let mut s = Switch::new(on);
        if disabled { s = s.disabled(); }
        s.paint(&mut ctx);
        rec.finish().commands
    }

    #[test]
    fn default_size_is_the_premium_track() {
        let font = FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(rosace_layout::Constraints::loose(200.0, 200.0), &font, &theme);
        assert_eq!(Switch::new(false).layout(&ctx), Size { width: 44.0, height: 24.0 });
    }

    #[test]
    fn paints_track_thumb_shadow_and_thumb() {
        let cmds = paint_switch(true, false);
        assert!(cmds.iter().any(|c| matches!(c, DrawCommand::DrawShadow { .. })),
            "thumb must cast an elevation shadow");
        let circles = cmds.iter().filter(|c| matches!(c, DrawCommand::FillCircle { .. })).count();
        assert!(circles >= 1, "thumb is a filled circle");
    }

    #[test]
    #[ignore] // visual showcase — run explicitly: cargo test -p rosace-widgets switch_showcase -- --ignored --nocapture
    fn switch_showcase() {
        use super::super::app::WidgetApp;
        use super::super::Column;
        use crate::EdgeInsets;
        let out = std::env::var("SWITCH_PNG").unwrap_or_else(|_| "switch_showcase.png".to_string());
        let panel = |dark: bool| {
            let col = Column::new().spacing(22.0).padding(EdgeInsets::all(28.0))
                .child(Switch::new(false))
                .child(Switch::new(true))
                .child(Switch::new(false).disabled())
                .child(Switch::new(true).disabled());
            let app = WidgetApp::new(120, 220);
            if dark { app.dark() } else { app.light() }.render_png(&col)
        };
        std::fs::write(&out, panel(true)).unwrap();
        let light = out.replace(".png", "_light.png");
        std::fs::write(&light, panel(false)).unwrap();
        println!("wrote {out} and {light}");
    }

    #[test]
    fn on_and_off_thumbs_land_at_different_x() {
        // The thumb travels: its circle center x must differ between states.
        let cx = |on: bool| {
            paint_switch(on, false).into_iter().find_map(|c| match c {
                DrawCommand::FillCircle { center, .. } => Some(center.x),
                _ => None,
            }).expect("a thumb circle")
        };
        assert!(cx(true) > cx(false), "on-thumb must sit right of off-thumb");
    }
}
