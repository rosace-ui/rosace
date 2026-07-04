use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use super::{Widget, Children, LayoutCtx, PaintCtx, BoxedWidget};

/// Absolutely places a child inside a [`Stack`](super::stack::Stack) using
/// edge anchors. Receives the full stack rect and positions its child from
/// the given top/left/right/bottom (+ optional explicit width/height).
///
/// A child with no anchors fills the stack (the default Stack behavior).
pub struct Positioned {
    child: BoxedWidget,
    top: Option<f32>, left: Option<f32>, right: Option<f32>, bottom: Option<f32>,
    width: Option<f32>, height: Option<f32>,
}

impl Positioned {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { child: Box::new(child), top: None, left: None, right: None, bottom: None, width: None, height: None }
    }
    pub fn top(mut self, v: f32) -> Self { self.top = Some(v); self }
    pub fn left(mut self, v: f32) -> Self { self.left = Some(v); self }
    pub fn right(mut self, v: f32) -> Self { self.right = Some(v); self }
    pub fn bottom(mut self, v: f32) -> Self { self.bottom = Some(v); self }
    pub fn width(mut self, v: f32) -> Self { self.width = Some(v); self }
    pub fn height(mut self, v: f32) -> Self { self.height = Some(v); self }
}

impl Widget for Positioned {
    fn children(&self) -> Children<'_> { Children::One(&*self.child) }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // Fills the stack; the Stack sizes itself from non-positioned children.
        self.child.layout(ctx)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let s = ctx.rect; // full stack rect
        // Resolve size: explicit, else derived from opposite anchors, else measured.
        let measured = self.child.layout(&ctx.layout_ctx(Constraints::loose(s.size.width, s.size.height)));
        let w = self.width.or_else(|| match (self.left, self.right) {
            (Some(l), Some(r)) => Some((s.size.width - l - r).max(0.0)),
            _ => None,
        }).unwrap_or(measured.width);
        let h = self.height.or_else(|| match (self.top, self.bottom) {
            (Some(t), Some(b)) => Some((s.size.height - t - b).max(0.0)),
            _ => None,
        }).unwrap_or(measured.height);

        let x = match (self.left, self.right) {
            (Some(l), _) => s.origin.x + l,
            (None, Some(r)) => s.origin.x + s.size.width - r - w,
            (None, None) => s.origin.x,
        };
        let y = match (self.top, self.bottom) {
            (Some(t), _) => s.origin.y + t,
            (None, Some(b)) => s.origin.y + s.size.height - b - h,
            (None, None) => s.origin.y,
        };
        let rect = Rect { origin: Point { x, y }, size: Size { width: w, height: h } };
        self.child.paint(&mut ctx.child(rect));
    }
}
