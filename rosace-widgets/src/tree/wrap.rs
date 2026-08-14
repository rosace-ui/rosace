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
            ctx.paint_child(rect, &*child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_render::FontCache;
    use crate::tree::Container;

    fn env() -> (FontCache, rosace_theme::ThemeData) {
        (FontCache::embedded(), rosace_theme::built_in::dark_theme())
    }

    fn boxes(sizes: &[(f32, f32)]) -> Vec<BoxedWidget> {
        sizes.iter()
            .map(|(w, h)| Box::new(Container::new().width(*w).height(*h)) as BoxedWidget)
            .collect()
    }

    fn arrange_at(w: f32, spacing: f32, run_spacing: f32, sizes: &[(f32, f32)]) -> (Vec<Rect>, Size) {
        let (font, theme) = env();
        let ctx = LayoutCtx::new(Constraints::loose(w, 600.0), &font, &theme);
        Wrap::new().spacing(spacing).run_spacing(run_spacing)
            .children(boxes(sizes))
            .arrange(&ctx, w)
    }

    #[test]
    fn children_flow_onto_a_new_row_when_the_next_one_will_not_fit() {
        // 3 x 100 wide + 10 spacing into 250: two fit (100, 110), the third
        // would end at 320, so it wraps.
        let (rects, size) = arrange_at(250.0, 10.0, 8.0, &[(100.0, 40.0); 3]);
        assert_eq!(rects[0].origin, Point { x: 0.0, y: 0.0 });
        assert_eq!(rects[1].origin, Point { x: 110.0, y: 0.0 });
        assert_eq!(rects[2].origin, Point { x: 0.0, y: 48.0 }, "row 2, offset by run_spacing");
        assert_eq!(size.height, 88.0, "two rows of 40 plus one 8px run gap");
    }

    /// The tallest child sets the row height, and the NEXT row clears it.
    #[test]
    fn a_row_is_as_tall_as_its_tallest_child() {
        let (rects, size) = arrange_at(250.0, 10.0, 0.0, &[(100.0, 20.0), (100.0, 70.0), (100.0, 20.0)]);
        assert_eq!(rects[2].origin.y, 70.0, "cleared the tall child, not the short one");
        assert_eq!(size.height, 90.0);
    }

    /// A single child wider than the container must not wrap onto a row of
    /// its own before it has started — `x > 0.0` is what guards that, and
    /// without it the first child would always be pushed to row two.
    #[test]
    fn a_child_wider_than_the_container_still_starts_on_the_first_row() {
        let (rects, _) = arrange_at(100.0, 10.0, 0.0, &[(400.0, 30.0)]);
        assert_eq!(rects[0].origin, Point { x: 0.0, y: 0.0 });
    }

    #[test]
    fn an_empty_wrap_is_zero_sized_and_does_not_panic() {
        let (rects, size) = arrange_at(300.0, 10.0, 10.0, &[]);
        assert!(rects.is_empty());
        assert_eq!((size.width, size.height), (0.0, 0.0));
    }

    /// `layout` returns the FULL available width by design (see its comment):
    /// returning the post-wrap content width would make the parent allocate a
    /// narrower box, which paint would then re-wrap into — over-wrapping.
    #[test]
    fn layout_claims_the_full_width_and_only_the_height_is_content_derived() {
        let (font, theme) = env();
        let ctx = LayoutCtx::new(Constraints::loose(500.0, 600.0), &font, &theme);
        let w = Wrap::new().spacing(10.0).children(boxes(&[(100.0, 40.0), (100.0, 40.0)]));
        let size = w.layout(&ctx);
        assert_eq!(size.width, 500.0, "full width, not the 210 the children occupy");
        assert_eq!(size.height, 40.0, "one row");
    }
}
