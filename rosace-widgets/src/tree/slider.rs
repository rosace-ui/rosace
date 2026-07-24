use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w};
use super::container::draw_rounded_rect_pub;

/// A horizontal slider — brought to the Quality Bar (matches the `Switch`
/// exemplar; see `.steering/WIDGET_QUALITY_BAR.md`).
///
/// - **States** — idle · hover (thumb grows) · pressed/dragging (grows more) ·
///   focus-visible (ring) · disabled (dimmed, inert).
/// - **Motion** — the fill + thumb track the value *instantly* (they follow the
///   finger, never lag); the thumb's grow and its hover/press/focus state-layer
///   halo ease smoothly on their own channels.
/// - **Elevation** — the bright knob casts a soft shadow over a rounded pill
///   track with a colored fill.
/// - **A11y** — `Role::Slider` with the current value; drag works through the
///   engine's positional press (interactive-by-identity — an unwired slider
///   still absorbs so it can't pan a scroll view behind it).
pub struct Slider {
    pub value: f32, // normalized 0..1
    pub min: f32,
    pub max: f32,
    disabled: bool,
    height: f32,
    width: Option<f32>,
    track_color: Option<Color>,
    fill_color: Option<Color>,
    thumb_color: Option<Color>,
    on_change: Option<Arc<dyn Fn(f32) + Send + Sync>>,
}

impl Slider {
    pub fn new(value: f32) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            min: 0.0,
            max: 1.0,
            disabled: false,
            height: 24.0,
            width: None,
            track_color: None,
            fill_color: None,
            thumb_color: None,
            on_change: None,
        }
    }
    pub fn range(mut self, min: f32, max: f32, value: f32) -> Self {
        self.min = min; self.max = max;
        self.value = ((value - min) / (max - min)).clamp(0.0, 1.0);
        self
    }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    pub fn disabled_if(mut self, c: bool) -> Self { if c { self.disabled = true; } self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = Some(c); self }
    pub fn fill_color(mut self, c: Color) -> Self { self.fill_color = Some(c); self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }

    /// Called with the new value (in `min..max`) on click or drag.
    pub fn on_change(mut self, f: impl Fn(f32) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl Widget for Slider {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        Size { width: self.width.unwrap_or(avail_w(ctx.constraints)), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(rosace_core::Role::Slider)
            .value(format!("{:.2}", self.min + self.value * (self.max - self.min))));

        // ── Interactivity (identity) — a positional press is draggable via the
        //     engine's active_drag. Disabled still absorbs. ────────────────────
        let base_r = 9.0;
        let r = ctx.rect;
        let usable = (r.size.width - base_r * 2.0).max(1.0);
        match (&self.on_change, self.disabled) {
            (Some(f), false) => {
                let f = f.clone();
                let (min, max, x0) = (self.min, self.max, r.origin.x + base_r);
                ctx.on_press_at(move |px, _| {
                    let t = ((px - x0) / usable).clamp(0.0, 1.0);
                    f(min + t * (max - min));
                });
            }
            _ => ctx.on_press_at(|_, _| {}),
        }
        let focused = !self.disabled && ctx.focus_node().is_focused();
        let hovered = !self.disabled && ctx.hovered();
        let pressed = !self.disabled && ctx.pressed();

        // ch0: hover/press/focus halo. ch1: thumb grow. (Position is NOT
        // animated — it follows the value/finger instantly.)
        let halo_t = if pressed { 0.18 } else if focused { 0.12 } else if hovered { 0.08 } else { 0.0 };
        let halo = ctx.animate_channel(0, halo_t, 0.0);
        let grow = ctx.animate_channel(1, if pressed { 1.0 } else if hovered { 0.5 } else { 0.0 }, 0.0);
        let thumb_r = base_r + grow * 2.0;

        // ── Colors ────────────────────────────────────────────────────────────
        let colors = ctx.theme.colors.clone();
        let track = self.track_color.unwrap_or_else(|| ctx.tc(colors.surface_variant));
        let fill = self.fill_color.unwrap_or_else(|| ctx.tc(colors.primary));
        let thumb = self.thumb_color.unwrap_or_else(|| Color::rgb(250, 250, 252));
        let shadow = ctx.tc(colors.shadow);
        let dim = if self.disabled { 0.4 } else { 1.0 };

        let cy = r.origin.y + r.size.height / 2.0;
        let track_h = 6.0;
        let tr = track_h / 2.0;
        let cx = r.origin.x + base_r + usable * self.value;

        // ── Track (rounded pill) + fill ──────────────────────────────────────
        draw_rounded_rect_pub(ctx, Rect {
            origin: Point { x: r.origin.x, y: cy - tr }, size: Size { width: r.size.width, height: track_h },
        }, with_alpha(track, dim), tr);
        let fill_w = cx - r.origin.x;
        if fill_w > tr {
            draw_rounded_rect_pub(ctx, Rect {
                origin: Point { x: r.origin.x, y: cy - tr }, size: Size { width: fill_w, height: track_h },
            }, with_alpha(fill, dim), tr);
        }

        // ── State-layer halo behind the thumb ────────────────────────────────
        if halo > 0.001 {
            ctx.fill_circle(Point { x: cx, y: cy }, thumb_r + 8.0, with_alpha(fill, halo));
        }

        // ── Thumb: elevation shadow then the bright knob ─────────────────────
        let d = thumb_r * 2.0;
        ctx.fill_shadow_rrect(
            Rect { origin: Point { x: cx - thumb_r, y: cy - thumb_r + 1.0 }, size: Size { width: d, height: d } },
            thumb_r, with_alpha(shadow, 0.3 * dim), 4.0,
        );
        ctx.fill_circle(Point { x: cx, y: cy }, thumb_r, with_alpha(thumb, dim));

        // ── Focus ring ────────────────────────────────────────────────────────
        if focused {
            ctx.stroke_rrect(
                Rect { origin: Point { x: cx - thumb_r - 3.0, y: cy - thumb_r - 3.0 }, size: Size { width: d + 6.0, height: d + 6.0 } },
                thumb_r + 3.0, with_alpha(fill, 0.9), 2.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;
    use rosace_render::{FontCache, PictureRecorder};
    use rosace_render::draw_command::DrawCommand;
    use std::cell::RefCell;
    use std::rc::Rc;
    use crate::tree::RenderTree;

    #[test]
    fn customization_builders_do_not_change_layout_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let base = Slider::new(0.5);
        let customized = Slider::new(0.5).height(30.0).fill_color(Color::rgb(255, 0, 0));
        assert_eq!(base.layout(&ctx).width, customized.layout(&ctx).width);
        assert_eq!(customized.layout(&ctx).height, 30.0);
    }

    #[test]
    #[ignore] // SLIDER_PNG=/path cargo test -p rosace-widgets slider_showcase -- --ignored --nocapture
    fn slider_showcase() {
        use super::super::app::WidgetApp;
        use super::super::Column;
        use crate::EdgeInsets;
        let out = std::env::var("SLIDER_PNG").unwrap_or_else(|_| "slider_showcase.png".to_string());
        let panel = |dark: bool| {
            let col = Column::new().spacing(22.0).padding(EdgeInsets::all(26.0))
                .child(Slider::new(0.2).width(220.0))
                .child(Slider::new(0.5).width(220.0))
                .child(Slider::new(0.85).width(220.0))
                .child(Slider::new(0.6).width(220.0).disabled());
            let app = WidgetApp::new(280, 200);
            if dark { app.dark() } else { app.light() }.render_png(&col)
        };
        std::fs::write(&out, panel(true)).unwrap();
        std::fs::write(out.replace(".png", "_light.png"), panel(false)).unwrap();
        println!("wrote {out}");
    }

    fn thumb_x(value: f32) -> f32 {
        let font = FontCache::embedded();
        let mut rec = PictureRecorder::new();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let mut ctx = PaintCtx::root(
            &mut rec,
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 200.0, height: 24.0 } },
            &font, rosace_theme::built_in::dark_theme(), tree,
        );
        Slider::new(value).paint(&mut ctx);
        // Last FillCircle is the thumb.
        rec.finish().commands.into_iter().rev().find_map(|c| match c {
            DrawCommand::FillCircle { center, .. } => Some(center.x),
            _ => None,
        }).expect("a thumb circle")
    }

    #[test]
    fn thumb_tracks_value_left_to_right() {
        assert!(thumb_x(0.0) < thumb_x(0.5), "thumb moves right as value grows");
        assert!(thumb_x(0.5) < thumb_x(1.0), "thumb keeps moving right");
    }

    #[test]
    fn thumb_stays_within_the_track() {
        assert!(thumb_x(0.0) >= 0.0 && thumb_x(1.0) <= 200.0, "thumb never overflows the track");
    }
}
