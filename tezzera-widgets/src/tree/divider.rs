use tezzera_core::types::Size;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w, avail_h};

/// A thin separator line — horizontal or vertical.
pub struct Divider {
    pub vertical: bool,
    pub thickness: f32,
    pub color: Color,
    pub indent: f32,
}

impl Divider {
    /// A horizontal divider — the common case (D093: `new()` must exist
    /// wherever named constructors exist).
    pub fn new() -> Self {
        Self::horizontal()
    }

    pub fn horizontal() -> Self {
        Self { vertical: false, thickness: 1.0, color: Color::rgba(0, 0, 0, 0), indent: 0.0 }
    }
    pub fn vertical() -> Self {
        Self { vertical: true, thickness: 1.0, color: Color::rgba(0, 0, 0, 0), indent: 0.0 }
    }
    pub fn color(mut self, c: Color) -> Self { self.color = c; self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn indent(mut self, i: f32) -> Self { self.indent = i; self }
}

impl Widget for Divider {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        if self.vertical {
            Size { width: self.thickness, height: avail_h(constraints) }
        } else {
            Size { width: avail_w(constraints), height: self.thickness }
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        use tezzera_core::types::{Point, Rect};
        let color = if self.color.a == 0 { ctx.tc(ctx.theme.colors.outline) } else { self.color };
        let r = ctx.rect;
        let rect = if self.vertical {
            Rect { origin: Point { x: r.origin.x, y: r.origin.y + self.indent }, size: Size { width: self.thickness, height: (r.size.height - self.indent).max(0.0) } }
        } else {
            Rect { origin: Point { x: r.origin.x + self.indent, y: r.origin.y }, size: Size { width: (r.size.width - self.indent).max(0.0), height: self.thickness } }
        };
        ctx.fill_rect(rect, color);
    }
}
