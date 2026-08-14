use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
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
        ctx.paint_child(rect, &*self.child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_render::{FontCache, PictureRecorder};
    use crate::tree::{Container, PaintCtx, RenderTree};
    use std::{cell::RefCell, rc::Rc};

    /// Paint a `Positioned` into a 400x300 stack and return the rect its
    /// child was actually given.
    fn child_rect(p: Positioned) -> Rect {
        let font = FontCache::embedded();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let mut rec = PictureRecorder::new();
        {
            let mut ctx = PaintCtx::root(
                &mut rec,
                Rect { origin: Point { x: 0.0, y: 0.0 },
                       size: Size { width: 400.0, height: 300.0 } },
                &font,
                rosace_theme::built_in::dark_theme(),
                tree.clone(),
            );
            p.paint(&mut ctx);
        }
        let t = tree.borrow();
        let root = t.node(RenderTree::ROOT);
        let child = *root.children.first().expect("Positioned must paint a child node");
        t.node(child).cached_rect.expect("the child node must have a rect")
    }

    fn box64() -> Container { Container::new().width(64.0).height(32.0) }

    /// Every combination of anchors, because each takes a different branch
    /// and the wrong one is a silent misplacement rather than an error.
    #[test]
    fn anchors_place_the_child_against_the_edges_they_name() {
        let tl = child_rect(Positioned::new(box64()).top(8.0).left(12.0));
        assert_eq!((tl.origin.x, tl.origin.y), (12.0, 8.0));
        assert_eq!((tl.size.width, tl.size.height), (64.0, 32.0), "size still measured");

        // right/bottom are offsets from the FAR edge, so the origin has to
        // account for the child's own size: 400 - 12 - 64 = 324.
        let br = child_rect(Positioned::new(box64()).bottom(8.0).right(12.0));
        assert_eq!(br.origin.x, 400.0 - 12.0 - 64.0);
        assert_eq!(br.origin.y, 300.0 - 8.0 - 32.0);

        // No anchors at all: the stack's own origin.
        let none = child_rect(Positioned::new(box64()));
        assert_eq!((none.origin.x, none.origin.y), (0.0, 0.0));
    }

    /// Opposite anchors DERIVE the size — this is the branch that makes
    /// `.top(0).bottom(0)` a "stretch to fill" idiom, and it overrides the
    /// child's measured size.
    #[test]
    fn opposite_anchors_stretch_the_child_between_them() {
        let r = child_rect(Positioned::new(box64()).left(20.0).right(30.0).top(10.0).bottom(40.0));
        assert_eq!(r.size.width, 400.0 - 20.0 - 30.0, "width derived from left+right");
        assert_eq!(r.size.height, 300.0 - 10.0 - 40.0, "height derived from top+bottom");
        assert_eq!((r.origin.x, r.origin.y), (20.0, 10.0));
    }

    /// An explicit size beats a derived one.
    #[test]
    fn an_explicit_size_wins_over_opposite_anchors() {
        let r = child_rect(Positioned::new(box64()).left(0.0).right(0.0).width(100.0));
        assert_eq!(r.size.width, 100.0);
    }

    /// Anchors that overlap must collapse the box, not invert it. A stack
    /// narrower than its own insets is ordinary on a resized window.
    #[test]
    fn overlapping_anchors_collapse_to_zero_never_negative() {
        let r = child_rect(Positioned::new(box64()).left(300.0).right(300.0).top(200.0).bottom(200.0));
        assert_eq!(r.size.width, 0.0);
        assert_eq!(r.size.height, 0.0);
    }

    /// `left` wins when both are given WITHOUT a derived size — documenting
    /// the precedence rather than leaving it to be discovered.
    #[test]
    fn left_takes_precedence_over_right_for_placement() {
        let r = child_rect(Positioned::new(box64()).left(5.0).right(5.0).width(64.0));
        assert_eq!(r.origin.x, 5.0, "anchored from the left edge");
    }
}
