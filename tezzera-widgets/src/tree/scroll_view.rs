use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};

/// Scroll direction.
#[derive(Debug, Clone, Copy, Default)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

/// A scrollable viewport. The child can exceed the available size; content
/// is painted at `offset` and clipped to the viewport bounds.
///
/// In a real runtime, offset would be driven by gesture/scroll events.
/// Here it is set statically (useful for snapshot demos).
pub struct ScrollView {
    child: BoxedWidget,
    pub offset: f32,
    pub axis: ScrollAxis,
    pub show_scrollbar: bool,
    pub scrollbar_color: Color,
}

impl ScrollView {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            offset: 0.0,
            axis: ScrollAxis::Vertical,
            show_scrollbar: true,
            scrollbar_color: Color::rgb(50, 55, 85),
        }
    }
    pub fn offset(mut self, o: f32) -> Self { self.offset = o; self }
    pub fn axis(mut self, a: ScrollAxis) -> Self { self.axis = a; self }
    pub fn no_scrollbar(mut self) -> Self { self.show_scrollbar = false; self }
}

impl Widget for ScrollView {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        // ScrollView claims all available space.
        Size { width: avail_w(constraints), height: avail_h(constraints) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let vp = ctx.rect;

        // Measure child with unconstrained axis
        let child_constraints = match self.axis {
            ScrollAxis::Vertical   => Constraints::loose(vp.size.width,  f32::INFINITY),
            ScrollAxis::Horizontal => Constraints::loose(f32::INFINITY, vp.size.height),
            ScrollAxis::Both       => Constraints::loose(f32::INFINITY, f32::INFINITY),
        };
        let child_size = self.child.layout(&ctx.layout_ctx(child_constraints));

        // Paint child at offset
        let (ox, oy) = match self.axis {
            ScrollAxis::Vertical   => (0.0, -self.offset),
            ScrollAxis::Horizontal => (-self.offset, 0.0),
            ScrollAxis::Both       => (-self.offset, -self.offset),
        };

        let child_rect = Rect {
            origin: Point { x: vp.origin.x + ox, y: vp.origin.y + oy },
            size: child_size,
        };
        self.child.paint(&mut ctx.child(child_rect));

        // Scrollbar (vertical)
        if self.show_scrollbar && matches!(self.axis, ScrollAxis::Vertical | ScrollAxis::Both) {
            let ratio = (vp.size.height / child_size.height.max(1.0)).min(1.0);
            if ratio < 1.0 {
                let bar_h = vp.size.height * ratio;
                let bar_y = vp.origin.y + (self.offset / child_size.height) * vp.size.height;
                ctx.fill_rect(Rect {
                    origin: Point { x: vp.origin.x + vp.size.width - 4.0, y: bar_y },
                    size: Size { width: 3.0, height: bar_h },
                }, self.scrollbar_color);
            }
        }
    }
}
