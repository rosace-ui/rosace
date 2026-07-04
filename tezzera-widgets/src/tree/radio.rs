use std::sync::Arc;
use tezzera_core::types::{Point, Size};
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// A single radio button (ring + filled dot when selected). Single-select is
/// the app's job: bind several radios to one `Atom<T>` and compare — distinct
/// behavior from Checkbox (mutually exclusive), so not a duplicate.
pub struct Radio {
    selected: bool,
    size: f32,
    color: Option<Color>,
    on_select: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Radio {
    pub fn new(selected: bool) -> Self {
        Self { selected, size: 20.0, color: None, on_select: None }
    }
    pub fn size(mut self, s: f32) -> Self { self.size = s; self }
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn on_select(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f)); self
    }
}

impl Widget for Radio {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(Size { width: self.size, height: self.size })
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let c = self.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.primary));
        let center = Point { x: r.origin.x + self.size / 2.0, y: r.origin.y + self.size / 2.0 };
        let t = ctx.animate_to(if self.selected { 1.0 } else { 0.0 }, 0.0);
        let ring = super::lerp_color(ctx.tc(ctx.theme.colors.outline), c, t);
        ctx.fill_arc(center, self.size / 2.0 - 1.5, 2.0, 0.0, 360.0, ring);
        if t > 0.01 {
            ctx.fill_circle(center, self.size / 4.0 * t, c);
        }
        if let Some(cb) = &self.on_select {
            ctx.register_hit(Arc::clone(cb));
        }
    }
}
