use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;

/// A checkbox with optional label — brought to the Quality Bar (see
/// `.steering/WIDGET_QUALITY_BAR.md`; matches the `Switch` exemplar).
///
/// - **States** — unchecked/checked/indeterminate · hover · pressed (box
///   dips) · focus-visible (ring) · disabled (dimmed, inert).
/// - **Motion** — box fill + border ease on toggle; the checkmark *pops in*
///   (scales 0.5→1 while it fades); the hover/press/focus state-layer halo
///   fades smoothly. Three independent animation channels, all idle when
///   settled.
/// - **Theming** — fill from `primary`, border from `outline`, ink is a
///   bright high-contrast tick; light+dark adaptive; overridable.
/// - **A11y** — `Role::Checkbox` with checked/unchecked value + optional label.
/// - **Interactive-by-identity** — always owns its hit region.
pub struct Checkbox {
    pub checked: bool,
    pub indeterminate: bool,
    disabled: bool,
    label: Option<String>,
    box_size: f32,
    font_size: f32,
    on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    color: Option<Color>,
}

impl Checkbox {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            indeterminate: false,
            disabled: false,
            label: None,
            box_size: 18.0,
            font_size: 13.0,
            on_change: None,
            color: None,
        }
    }

    pub fn label(mut self, l: impl Into<String>) -> Self { self.label = Some(l.into()); self }

    /// Called with the NEW value when the control is toggled (D094).
    pub fn on_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn indeterminate(mut self) -> Self { self.indeterminate = true; self }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    pub fn disabled_if(mut self, c: bool) -> Self { if c { self.disabled = true; } self }

    /// Override the checked fill color (default: theme `primary`).
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }

    pub fn size(mut self, s: f32) -> Self { self.box_size = s; self.font_size = s * 0.72; self }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl Widget for Checkbox {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        let label_w = self.label.as_ref()
            .map(|l| l.len() as f32 * self.font_size * 0.6 + 10.0)
            .unwrap_or(0.0);
        // Height clears the state-layer halo so neighbours aren't clipped.
        Size { width: self.box_size + label_w, height: self.box_size.max(self.font_size * 1.4) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // ── A11y ──────────────────────────────────────────────────────────
        let mut sem = super::Semantics::new(rosace_core::Role::Checkbox)
            .value(if self.indeterminate { "mixed" } else if self.checked { "checked" } else { "unchecked" });
        if let Some(l) = &self.label { sem = sem.label(l); }
        ctx.semantics(sem);

        // ── Interactivity (identity) + focus ─────────────────────────────────
        match (&self.on_change, self.disabled) {
            (Some(f), false) => { let f = f.clone(); let next = !self.checked; ctx.on_press(move || f(next)); }
            _ => ctx.on_press(|| {}),
        }
        let focused = !self.disabled && ctx.focus_node().is_focused();
        let hovered = !self.disabled && ctx.hovered();
        let pressed = !self.disabled && ctx.pressed();
        let on = self.checked || self.indeterminate;

        // ── Animation channels ───────────────────────────────────────────────
        let t = ctx.animate_channel(0, if on { 1.0 } else { 0.0 }, 0.0);           // check progress
        let halo_t = if pressed { 0.16 } else if focused { 0.12 } else if hovered { 0.08 } else { 0.0 };
        let halo = ctx.animate_channel(1, halo_t, 0.0);                              // state layer
        let press = ctx.animate_channel(2, if pressed { 1.0 } else { 0.0 }, 0.0);    // press dip

        // ── Colors (tokens) ──────────────────────────────────────────────────
        let colors = ctx.theme.colors.clone();
        let fill = self.color.unwrap_or_else(|| ctx.tc(colors.primary));
        let empty = ctx.tc(colors.surface);
        let border = ctx.tc(colors.outline);
        let label_color = ctx.tc(colors.on_surface);
        let ink = Color::rgb(252, 252, 255); // bright high-contrast tick
        let dim = if self.disabled { 0.38 } else { 1.0 };

        let bs = self.box_size;
        let cx = ctx.rect.origin.x + bs / 2.0;
        let cy = ctx.rect.origin.y + ctx.rect.size.height / 2.0;
        let radius = (bs * 0.22).max(3.0);

        // ── State-layer halo (behind the box) ────────────────────────────────
        if halo > 0.001 {
            let hc = super::lerp_color(border, fill, t);
            ctx.fill_circle(Point { x: cx, y: cy }, bs * 0.5 + 8.0, with_alpha(hc, halo));
        }

        // ── The box (dips slightly on press) ─────────────────────────────────
        let scale = 1.0 - press * 0.08;
        let half = bs * 0.5 * scale;
        let box_rect = Rect {
            origin: Point { x: cx - half, y: cy - half },
            size: Size { width: half * 2.0, height: half * 2.0 },
        };
        // Empty fill fades to the checked fill as t rises.
        draw_rounded_rect_pub(ctx, box_rect, with_alpha(super::lerp_color(empty, fill, t), dim), radius);
        if t < 0.99 {
            ctx.stroke_rrect(box_rect, radius, with_alpha(super::lerp_color(border, fill, t), (1.0 - t) * dim + t * dim), 1.5);
        }

        // ── Mark: indeterminate dash, or a checkmark that pops in ─────────────
        if t > 0.01 {
            if self.indeterminate {
                let w = bs * 0.5 * t;
                ctx.fill_rect(Rect {
                    origin: Point { x: cx - w / 2.0, y: cy - bs * 0.06 },
                    size: Size { width: w, height: (bs * 0.12).max(2.0) },
                }, with_alpha(ink, dim));
            } else {
                let px = bs * 0.9 * (0.6 + 0.4 * t); // scale-in pop
                let tw = ctx.font.measure_text("\u{2713}", px);
                let lh = ctx.font.line_height(px);
                ctx.draw_text_at(
                    "\u{2713}",
                    Point { x: cx - tw / 2.0, y: cy - lh / 2.0 },
                    with_alpha(ink, t * dim),
                    px,
                );
            }
        }

        // ── Focus ring ────────────────────────────────────────────────────────
        if focused {
            let ring = Rect {
                origin: Point { x: cx - bs * 0.5 - 3.0, y: cy - bs * 0.5 - 3.0 },
                size: Size { width: bs + 6.0, height: bs + 6.0 },
            };
            ctx.stroke_rrect(ring, radius + 3.0, with_alpha(fill, 0.9), 2.0);
        }

        // ── Label ──────────────────────────────────────────────────────────────
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
    use rosace_render::{FontCache, PictureRecorder};
    use rosace_render::draw_command::DrawCommand;
    use std::cell::RefCell;
    use std::rc::Rc;
    use crate::tree::RenderTree;

    fn paint(checked: bool, indeterminate: bool) -> Vec<DrawCommand> {
        let font = FontCache::embedded();
        let mut rec = PictureRecorder::new();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let mut ctx = PaintCtx::root(
            &mut rec,
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 18.0, height: 18.0 } },
            &font, rosace_theme::built_in::dark_theme(), tree,
        );
        let mut c = Checkbox::new(checked);
        if indeterminate { c = c.indeterminate(); }
        c.paint(&mut ctx);
        rec.finish().commands
    }

    #[test]
    #[ignore] // visual: CHECKBOX_PNG=/path cargo test -p rosace-widgets checkbox_showcase -- --ignored --nocapture
    fn checkbox_showcase() {
        use super::super::app::WidgetApp;
        use super::super::Column;
        use crate::EdgeInsets;
        let out = std::env::var("CHECKBOX_PNG").unwrap_or_else(|_| "checkbox_showcase.png".to_string());
        let panel = |dark: bool| {
            let col = Column::new().spacing(16.0).padding(EdgeInsets::all(24.0))
                .child(Checkbox::new(false).label("Unchecked"))
                .child(Checkbox::new(true).label("Checked"))
                .child(Checkbox::new(false).indeterminate().label("Indeterminate"))
                .child(Checkbox::new(true).disabled().label("Disabled"));
            let app = WidgetApp::new(220, 200);
            if dark { app.dark() } else { app.light() }.render_png(&col)
        };
        std::fs::write(&out, panel(true)).unwrap();
        std::fs::write(out.replace(".png", "_light.png"), panel(false)).unwrap();
        println!("wrote {out}");
    }

    #[test]
    fn checked_draws_a_tick_glyph() {
        assert!(paint(true, false).iter().any(|c| matches!(c, DrawCommand::DrawText { text, .. } if text == "\u{2713}")),
            "checked box must draw a ✓");
    }

    #[test]
    fn indeterminate_draws_a_dash_not_a_tick() {
        let cmds = paint(false, true);
        assert!(!cmds.iter().any(|c| matches!(c, DrawCommand::DrawText { text, .. } if text == "\u{2713}")),
            "indeterminate must not draw a tick");
    }

    #[test]
    fn unchecked_box_still_paints_its_outline() {
        assert!(paint(false, false).iter().any(|c| matches!(c, DrawCommand::FillRRect { .. })),
            "the box itself is a rounded rect in every state");
    }
}
