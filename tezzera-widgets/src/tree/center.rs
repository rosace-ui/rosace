use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};

/// Centers its child within the available space.
pub struct Center {
    child: BoxedWidget,
}

impl Center {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { child: Box::new(child) }
    }
}

impl Widget for Center {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        let child_size = self.child.layout(ctx);
        Size {
            width:  avail_w(constraints).max(child_size.width),
            height: avail_h(constraints).max(child_size.height),
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let child_size = self.child.layout(&ctx.layout_ctx(Constraints::loose(ctx.rect.size.width, ctx.rect.size.height)));
        let dx = ((ctx.rect.size.width  - child_size.width)  / 2.0).max(0.0);
        let dy = ((ctx.rect.size.height - child_size.height) / 2.0).max(0.0);
        let child_rect = Rect {
            origin: Point { x: ctx.rect.origin.x + dx, y: ctx.rect.origin.y + dy },
            size: child_size,
        };
        self.child.paint(&mut ctx.child(child_rect));
    }
}
