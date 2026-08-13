use rosace_core::types::{Point, Rect, Size};

/// Inset amounts on each edge (logical pixels).
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeInsets {
    pub fn all(v: f32) -> Self { Self { top: v, right: v, bottom: v, left: v } }
    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self { top: vertical, bottom: vertical, left: horizontal, right: horizontal }
    }
    pub fn horizontal(h: f32) -> Self { Self { left: h, right: h, ..Default::default() } }
    pub fn vertical(v: f32) -> Self { Self { top: v, bottom: v, ..Default::default() } }
    pub fn only(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self { top, right, bottom, left }
    }

    pub fn total_h(&self) -> f32 { self.left + self.right }
    pub fn total_v(&self) -> f32 { self.top + self.bottom }

    /// Shrink a rect by these insets.
    pub fn shrink(&self, r: Rect) -> Rect {
        Rect {
            origin: Point { x: r.origin.x + self.left, y: r.origin.y + self.top },
            size: Size {
                width:  (r.size.width  - self.total_h()).max(0.0),
                height: (r.size.height - self.total_v()).max(0.0),
            },
        }
    }

    /// Grow a size by these insets.
    pub fn grow(&self, s: Size) -> Size {
        Size { width: s.width + self.total_h(), height: s.height + self.total_v() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
    }

    /// `only` takes top, right, bottom, left — CSS order, not alphabetical
    /// and not `left` first. Every constructor is pinned here because a
    /// silent transposition would shift padding on one axis in every app
    /// using it, and look like a design choice rather than a bug.
    #[test]
    fn constructors_put_each_value_on_the_edge_it_names() {
        let a = EdgeInsets::all(4.0);
        assert_eq!((a.top, a.right, a.bottom, a.left), (4.0, 4.0, 4.0, 4.0));

        // symmetric(HORIZONTAL, VERTICAL) — this argument order is the one
        // most likely to be swapped by a caller or a refactor.
        let s = EdgeInsets::symmetric(10.0, 20.0);
        assert_eq!((s.left, s.right), (10.0, 10.0), "first arg is horizontal");
        assert_eq!((s.top, s.bottom), (20.0, 20.0), "second arg is vertical");

        let h = EdgeInsets::horizontal(6.0);
        assert_eq!((h.left, h.right, h.top, h.bottom), (6.0, 6.0, 0.0, 0.0));

        let v = EdgeInsets::vertical(6.0);
        assert_eq!((v.top, v.bottom, v.left, v.right), (6.0, 6.0, 0.0, 0.0));

        let o = EdgeInsets::only(1.0, 2.0, 3.0, 4.0);
        assert_eq!((o.top, o.right, o.bottom, o.left), (1.0, 2.0, 3.0, 4.0));

        let d = EdgeInsets::default();
        assert_eq!((d.top, d.right, d.bottom, d.left), (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn totals_sum_the_opposing_edges() {
        let e = EdgeInsets::only(1.0, 2.0, 4.0, 8.0);
        assert_eq!(e.total_h(), 10.0, "left + right");
        assert_eq!(e.total_v(), 5.0, "top + bottom");
    }

    #[test]
    fn shrink_moves_the_origin_in_and_pulls_the_size_in_from_both_sides() {
        let r = EdgeInsets::all(10.0).shrink(rect(100.0, 200.0, 300.0, 400.0));
        assert_eq!((r.origin.x, r.origin.y), (110.0, 210.0));
        assert_eq!((r.size.width, r.size.height), (280.0, 380.0));
    }

    /// The `.max(0.0)` clamps in `shrink` are the load-bearing part.
    ///
    /// Padding wider than the rect is not exotic — it happens the moment a
    /// window is dragged narrow, or a phone rotates. Without the clamp the
    /// rect gets a negative size, which propagates into child layout and
    /// eventually into a renderer that has no meaning for it. The origin
    /// still moves in, which is correct: the box collapses at the inset
    /// corner rather than inverting through it.
    #[test]
    fn padding_larger_than_the_rect_collapses_to_zero_never_negative() {
        let r = EdgeInsets::all(50.0).shrink(rect(0.0, 0.0, 30.0, 20.0));
        assert_eq!((r.size.width, r.size.height), (0.0, 0.0));
        assert_eq!((r.origin.x, r.origin.y), (50.0, 50.0));

        // One axis over, one under — the clamp must be per-axis.
        let mixed = EdgeInsets::symmetric(100.0, 5.0).shrink(rect(0.0, 0.0, 40.0, 200.0));
        assert_eq!(mixed.size.width, 0.0, "horizontal collapses");
        assert_eq!(mixed.size.height, 190.0, "vertical is untouched");
    }

    #[test]
    fn grow_is_the_inverse_of_shrink_on_size() {
        let e = EdgeInsets::only(3.0, 5.0, 7.0, 11.0);
        let inner = e.shrink(rect(0.0, 0.0, 100.0, 100.0)).size;
        let back = e.grow(inner);
        assert_eq!((back.width, back.height), (100.0, 100.0));
    }

    /// `grow` deliberately has NO clamp: a negative inset is a legitimate
    /// outset (a focus ring drawn outside its box), and clamping it would
    /// silently break that. Recorded so nobody "fixes" the asymmetry.
    #[test]
    fn grow_allows_negative_insets_as_an_outset() {
        let e = EdgeInsets::all(-4.0);
        let g = e.grow(Size { width: 100.0, height: 50.0 });
        assert_eq!((g.width, g.height), (92.0, 42.0));
    }
}
