use tezzera_core::types::Size;
use super::{Widget, LayoutCtx, PaintCtx};

/// A fixed-size gap (invisible). Use inside Row/Column.
pub struct Spacer {
    pub width: f32,
    pub height: f32,
}

impl Spacer {
    pub fn new(size: f32) -> Self { Self { width: size, height: size } }
    pub fn w(width: f32) -> Self { Self { width, height: 0.0 } }
    pub fn h(height: f32) -> Self { Self { width: 0.0, height } }
}

impl Widget for Spacer {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        Size { width: self.width, height: self.height }
    }
    fn paint(&self, _ctx: &mut PaintCtx) {}
}

/// Fills remaining space in a Row or Column (flex weight 1 by default).
///
/// Wrap any widget with `Expanded::new(child)` to make it fill leftover space.
pub struct Expanded {
    pub factor: f32,
    pub child: Option<Box<dyn Widget>>,
}

impl Expanded {
    /// Empty space filler (no child).
    pub fn empty() -> Self { Self { factor: 1.0, child: None } }

    /// Expand `child` to fill available space.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { factor: 1.0, child: Some(Box::new(child)) }
    }

    pub fn with_factor(mut self, f: f32) -> Self { self.factor = f; self }
}

impl Widget for Expanded {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        if let Some(child) = &self.child {
            child.layout(ctx)
        } else {
            Size { width: constraints.max_width_f32().min(0.0), height: constraints.max_height_f32().min(0.0) }
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        if let Some(child) = &self.child {
            child.paint(ctx);
        }
    }

    fn flex_factor(&self) -> f32 { self.factor }
}
