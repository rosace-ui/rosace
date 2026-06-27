//! [`Padding`] — a layout helper that adds insets around a child widget.

/// Per-side insets (top / right / bottom / left) in pixels.
#[derive(Debug, Clone, Copy)]
pub struct EdgeInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeInsets {
    /// Uniform insets: all four sides set to `v`.
    pub fn all(v: f32) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }

    /// Symmetric insets: `vertical` for top/bottom, `horizontal` for left/right.
    pub fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }

    /// Explicit per-side insets.
    pub fn only(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self { top, right, bottom, left }
    }
}

/// A layout container that adds space around its child.
///
/// `Padding` is a pure layout helper — it calculates the outer size and the
/// child's origin offset.  The caller is responsible for rendering the child at
/// the returned position.
pub struct Padding {
    pub insets: EdgeInsets,
    pub inner_width: f32,
    pub inner_height: f32,
}

impl Padding {
    /// Creates a [`Padding`] wrapper with the given insets and child dimensions.
    pub fn new(insets: EdgeInsets, inner_width: f32, inner_height: f32) -> Self {
        Self { insets, inner_width, inner_height }
    }

    /// Returns the total outer (width, height) including insets.
    pub fn outer_size(&self) -> (f32, f32) {
        (
            self.inner_width  + self.insets.left + self.insets.right,
            self.inner_height + self.insets.top  + self.insets.bottom,
        )
    }

    /// Returns the (x, y) origin at which the child should be drawn,
    /// given the container's top-left corner `(x, y)`.
    pub fn child_origin(&self, x: f32, y: f32) -> (f32, f32) {
        (x + self.insets.left, y + self.insets.top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_insets_all_sets_all_sides() {
        let e = EdgeInsets::all(10.0);
        assert_eq!(e.top, 10.0);
        assert_eq!(e.right, 10.0);
        assert_eq!(e.bottom, 10.0);
        assert_eq!(e.left, 10.0);
    }

    #[test]
    fn edge_insets_symmetric_sets_correct_sides() {
        let e = EdgeInsets::symmetric(8.0, 16.0);
        assert_eq!(e.top, 8.0);
        assert_eq!(e.bottom, 8.0);
        assert_eq!(e.left, 16.0);
        assert_eq!(e.right, 16.0);
    }

    #[test]
    fn padding_outer_size_adds_insets() {
        let p = Padding::new(EdgeInsets::all(10.0), 100.0, 50.0);
        let (w, h) = p.outer_size();
        assert_eq!(w, 120.0); // 100 + 10 + 10
        assert_eq!(h, 70.0);  // 50 + 10 + 10
    }

    #[test]
    fn padding_child_origin_offsets_by_insets() {
        let p = Padding::new(EdgeInsets::only(5.0, 0.0, 0.0, 8.0), 100.0, 50.0);
        let (cx, cy) = p.child_origin(20.0, 30.0);
        assert_eq!(cx, 28.0); // 20 + left(8)
        assert_eq!(cy, 35.0); // 30 + top(5)
    }
}
