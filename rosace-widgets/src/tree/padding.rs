use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};

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

