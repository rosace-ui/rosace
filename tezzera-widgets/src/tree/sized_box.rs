use tezzera_core::types::Size;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};

/// A box with an exact size, optionally containing a child.
pub struct SizedBox {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub child: Option<BoxedWidget>,
}

impl SizedBox {
    pub fn new() -> Self { Self { width: None, height: None, child: None } }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = Some(h); self }
    pub fn size(mut self, w: f32, h: f32) -> Self { self.width = Some(w); self.height = Some(h); self }
    pub fn square(s: f32) -> Self { Self::new().size(s, s) }
    pub fn child(mut self, w: impl Widget + 'static) -> Self { self.child = Some(Box::new(w)); self }
    /// Invisible fixed-size gap (no child).
    pub fn gap(w: f32, h: f32) -> Self { Self::new().size(w, h) }
}

impl Default for SizedBox {
    fn default() -> Self { Self::new() }
}

impl Widget for SizedBox {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        let child_size = self.child.as_ref()
            .map(|c| c.layout(ctx))
            .unwrap_or(Size { width: 0.0, height: 0.0 });
        constraints.constrain(Size {
            width:  self.width.unwrap_or(child_size.width),
            height: self.height.unwrap_or(child_size.height),
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        if let Some(child) = &self.child {
            child.paint(ctx);
        }
    }
}
