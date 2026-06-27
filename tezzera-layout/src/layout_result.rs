//! The [`LayoutResult`] type returned by every widget's `layout` method.

use tezzera_core::types::{Point, Size};

/// Result of a single layout pass for one widget.
///
/// Holds the widget's resolved [`Size`] and the 2-D origin [`Point`] of each
/// child, in the same order the children were supplied to `layout`.
#[derive(Debug, Clone)]
pub struct LayoutResult {
    /// The resolved size of the widget after applying constraints.
    pub size: Size,
    /// The position of each child relative to this widget's origin.
    ///
    /// `child_positions[i]` corresponds to `child_sizes[i]` passed into `layout`.
    pub child_positions: Vec<Point>,
}
