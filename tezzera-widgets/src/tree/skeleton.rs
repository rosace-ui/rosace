use tezzera_core::types::Size;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// A shimmering loading placeholder — a rounded block with a highlight band
/// that sweeps across it. Use while content loads. Self-animating.
pub struct Skeleton {
    width: Option<f32>,
    height: f32,
    radius: f32,
}

impl Skeleton {
    pub fn new() -> Self { Self { width: None, height: 16.0, radius: 6.0 } }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    /// A circular avatar-sized skeleton.
    pub fn circle(size: f32) -> Self { Self { width: Some(size), height: size, radius: size / 2.0 } }
}

impl Default for Skeleton { fn default() -> Self { Self::new() } }

impl Widget for Skeleton {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = self.width.unwrap_or_else(|| super::avail_w(ctx.constraints));
        ctx.constraints.constrain(Size { width: w, height: self.height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let base = Color::rgba(255, 255, 255, 18);
        let hi   = Color::rgba(255, 255, 255, 42);
        // A horizontal gradient whose bright side tracks a sweeping phase.
        let phase = (super::anim_clock() * 0.8).fract(); // 0..1
        let (from, to) = if phase < 0.5 { (hi, base) } else { (base, hi) };
        ctx.fill_gradient(r, self.radius, from, to, false);
        ctx.request_animation();
    }
}
