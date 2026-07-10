use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use super::{Widget, Children, LayoutCtx, PaintCtx, BoxedWidget, avail_w};

/// Flows children left→right, wrapping to the next line when the row is full
/// — chip clouds, tag lists, toolbars. Lays out something new (D095).
pub struct Wrap {
    spacing: f32,
    run_spacing: f32,
    children: Vec<BoxedWidget>,
}

impl Wrap {
    pub fn new() -> Self { Self { spacing: 8.0, run_spacing: 8.0, children: Vec::new() } }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }
    pub fn run_spacing(mut self, s: f32) -> Self { self.run_spacing = s; self }
    pub fn child(mut self, w: impl Widget + 'static) -> Self { self.children.push(Box::new(w)); self }
    pub fn children(mut self, ws: Vec<BoxedWidget>) -> Self { self.children.extend(ws); self }

    /// Returns (per-child rect origins relative to 0,0, total size).
    fn arrange(&self, ctx: &LayoutCtx, max_w: f32) -> (Vec<Rect>, Size) {
        let mut rects = Vec::with_capacity(self.children.len());
        let (mut x, mut y, mut row_h, mut widest) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for c in &self.children {
            let s = c.layout(&ctx.with_constraints(Constraints::loose(max_w, f32::INFINITY)));
            if x > 0.0 && x + s.width > max_w {
                x = 0.0; y += row_h + self.run_spacing; row_h = 0.0;
            }
            rects.push(Rect { origin: Point { x, y }, size: s });
            x += s.width + self.spacing;
            row_h = row_h.max(s.height);
            widest = widest.max(x - self.spacing);
        }
        (rects, Size { width: widest.min(max_w), height: y + row_h })
    }
}

impl Default for Wrap { fn default() -> Self { Self::new() } }

impl Widget for Wrap {
    fn children(&self) -> Children<'_> { Children::Many(&self.children) }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // Occupy the FULL available width (like Grid) — children keep their
        // natural size and flow onto new lines. Returning the post-wrap
        // content width would make the parent allocate a narrower box, and
        // paint would then re-wrap into it (over-wrapping). Only the height
        // is content-derived.
        let w = avail_w(ctx.constraints);
        let (_, size) = self.arrange(ctx, w);
        ctx.constraints.constrain(Size { width: w, height: size.height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let (rects, _) = self.arrange(&ctx.layout_ctx(Constraints::loose(r.size.width, r.size.height)), r.size.width);
        for (child, rel) in self.children.iter().zip(rects) {
            let rect = Rect {
                origin: Point { x: r.origin.x + rel.origin.x, y: r.origin.y + rel.origin.y },
                size: rel.size,
            };
            child.paint(&mut ctx.child(rect));
        }
    }
}
