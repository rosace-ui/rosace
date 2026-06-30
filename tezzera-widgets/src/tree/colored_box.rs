use tezzera_core::types::Size;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w, avail_h};

/// A rectangle filled with a solid color — no child, no frills.
pub struct ColoredBox {
    pub color: Color,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl ColoredBox {
    pub fn new(color: Color) -> Self { Self { color, width: None, height: None } }
    pub fn size(mut self, w: f32, h: f32) -> Self { self.width = Some(w); self.height = Some(h); self }
}

impl Widget for ColoredBox {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size {
            width:  self.width.unwrap_or(avail_w(constraints)),
            height: self.height.unwrap_or(avail_h(constraints)),
        }
    }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill(self.color); }
}
