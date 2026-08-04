use rosace_core::types::{Point, Rect, Size};
use rosace_render::{Color, DrawCommand};
use super::{Widget, LayoutCtx, PaintCtx};

/// A shimmering loading placeholder — a rounded block with a soft highlight
/// band that sweeps left→right across it. Self-animating.
pub struct Skeleton {
    width: Option<f32>,
    height: f32,
    radius: f32,
    /// Base hue the shimmer is drawn in (white by default) — the actual
    /// painted colors are this RGB at varying alpha (see `paint`), so
    /// overriding it doesn't need separate base/highlight builders.
    color: Color,
    /// Sweep top-to-bottom instead of left-to-right.
    vertical: bool,
    /// Set by [`Self::circle`] — the shimmer band would bleed past the
    /// circle's curve (the renderer has no rounded-clip primitive), so
    /// circles get a breathing-alpha pulse instead (see `paint`).
    is_circle: bool,
}

impl Skeleton {
    pub fn new() -> Self { Self { width: None, height: 16.0, radius: 6.0, color: Color::rgb(255, 255, 255), vertical: false, is_circle: false } }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    /// Shimmer hue (white by default).
    pub fn color(mut self, c: Color) -> Self { self.color = c; self }
    /// Sweep the shimmer top-to-bottom instead of the default left-to-right.
    pub fn vertical(mut self, v: bool) -> Self { self.vertical = v; self }
    /// A circular avatar-sized skeleton.
    pub fn circle(size: f32) -> Self { Self { width: Some(size), height: size, radius: size / 2.0, color: Color::rgb(255, 255, 255), vertical: false, is_circle: true } }
}

impl Default for Skeleton { fn default() -> Self { Self::new() } }

impl Widget for Skeleton {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = self.width.unwrap_or_else(|| super::avail_w(ctx.constraints));
        ctx.constraints.constrain(Size { width: w, height: self.height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let (cr, cg, cb) = (self.color.r, self.color.g, self.color.b);
        let base = Color::rgba(cr, cg, cb, 30);
        let hi = Color::rgba(cr, cg, cb, 95);

        // Base block.
        ctx.fill_rrect(r, self.radius, base);

        let phase = (super::anim_clock() / 1.3).fract(); // 0..1, continuous

        if self.is_circle {
            // PushClip is an axis-aligned rect — clipping a sweeping band to
            // it leaves the highlight painting square corners outside the
            // circle's actual curve (looked like a barcode scanner). Circles
            // get a breathing-alpha pulse instead: same painted shape as the
            // base (fill_rrect with radius = size/2), so it can't leak.
            let pulse = 1.0 - (phase * 2.0 - 1.0).abs(); // 0 -> 1 -> 0 triangle wave
            let alpha = (hi.a as f32 * pulse) as u8;
            ctx.fill_rrect(r, self.radius, Color::rgba(cr, cg, cb, alpha));
        } else {
            let clear = Color::rgba(cr, cg, cb, 0);
            // A soft highlight band that sweeps across, clipped to the shape.
            ctx.record(DrawCommand::PushClip { rect: r });
            if self.vertical {
                let bh = (r.size.height * 0.35).max(24.0);
                let y = r.origin.y - bh + (r.size.height + bh) * phase; // enters top, exits bottom
                let half = bh / 2.0;
                ctx.fill_gradient(
                    Rect { origin: Point { x: r.origin.x, y }, size: Size { width: r.size.width, height: half } },
                    0.0, clear, hi, true);
                ctx.fill_gradient(
                    Rect { origin: Point { x: r.origin.x, y: y + half }, size: Size { width: r.size.width, height: half } },
                    0.0, hi, clear, true);
            } else {
                let bw = (r.size.width * 0.35).max(24.0);
                let x = r.origin.x - bw + (r.size.width + bw) * phase; // enters left, exits right
                let half = bw / 2.0;
                // Symmetric band: transparent → highlight → transparent (two ramps).
                ctx.fill_gradient(
                    Rect { origin: Point { x, y: r.origin.y }, size: Size { width: half, height: r.size.height } },
                    0.0, clear, hi, false);
                ctx.fill_gradient(
                    Rect { origin: Point { x: x + half, y: r.origin.y }, size: Size { width: half, height: r.size.height } },
                    0.0, hi, clear, false);
            }
            ctx.record(DrawCommand::PopClip);
        }
        ctx.request_animation();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn color_builder_does_not_change_layout_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let base = Skeleton::new().width(100.0).height(20.0);
        let customized = Skeleton::new().width(100.0).height(20.0).color(Color::rgb(200, 50, 50));
        assert_eq!(base.layout(&ctx), customized.layout(&ctx));
    }
}
