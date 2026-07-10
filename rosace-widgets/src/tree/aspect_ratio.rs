use rosace_core::types::Size;
use rosace_layout::Constraints;
use super::{Widget, Children, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};

/// Sizes its child to a fixed width:height `ratio` (e.g. 16.0/9.0), fitting
/// within the available space. Lays out something new (D095).
pub struct AspectRatio {
    ratio: f32,
    child: BoxedWidget,
}

impl AspectRatio {
    pub fn new(ratio: f32, child: impl Widget + 'static) -> Self {
        Self { ratio: ratio.max(0.01), child: Box::new(child) }
    }

    fn box_size(&self, c: &Constraints) -> Size {
        let (aw, ah) = (avail_w(*c), avail_h(*c));
        // Prefer full width; if that overflows height, clamp by height.
        let mut w = if aw.is_finite() { aw } else { ah * self.ratio };
        let mut h = w / self.ratio;
        if ah.is_finite() && h > ah { h = ah; w = h * self.ratio; }
        Size { width: w, height: h }
    }
}

impl Widget for AspectRatio {
    fn children(&self) -> Children<'_> { Children::One(&*self.child) }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(self.box_size(&ctx.constraints))
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        self.child.paint(&mut ctx.child(r));
    }
}
