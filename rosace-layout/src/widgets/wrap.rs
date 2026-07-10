//! [`Wrap`] — a wrapping layout that flows children into multiple rows.

use rosace_core::child_container::ChildContainer;
use rosace_core::element::{Element, NativeElement};
#[cfg(debug_assertions)]
use rosace_core::render_object::AxisBound;
use rosace_core::types::{Point, Size};
#[cfg(debug_assertions)]
use rosace_trace::{
    event::{ComponentId, RosaceTrace, TraceConstraints},
    trace,
};

use crate::constraints::Constraints;
use crate::layout_result::LayoutResult;

/// A widget that lays out children left-to-right and wraps to a new row when
/// the available width is exhausted.
///
/// - [`spacing`](Self::spacing) controls horizontal space between items.
/// - [`run_spacing`](Self::run_spacing) controls vertical space between rows.
#[derive(Debug, Clone)]
pub struct Wrap {
    children: Vec<Element>,
    spacing: f32,
    run_spacing: f32,
}

impl Wrap {
    /// Create a new `Wrap` with zero spacing and run-spacing.
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            spacing: 0.0,
            run_spacing: 0.0,
        }
    }

    /// Set the horizontal gap in logical pixels between items in the same row.
    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }

    /// Set the vertical gap in logical pixels between rows.
    pub fn run_spacing(mut self, s: f32) -> Self {
        self.run_spacing = s;
        self
    }

    /// Perform the Measure + Place passes and return a [`LayoutResult`].
    ///
    /// Items are placed left-to-right; when the next item would exceed the
    /// constraint's maximum width the layout wraps to a new row.
    ///
    /// Emits [`RosaceTrace::LayoutStart`] and [`RosaceTrace::LayoutEnd`] events.
    pub fn layout(&self, constraints: Constraints, child_sizes: &[Size]) -> LayoutResult {
        #[cfg(debug_assertions)]
        let start = std::time::Instant::now();

        #[cfg(debug_assertions)]
        trace!(RosaceTrace::LayoutStart {
            component: ComponentId(0),
            constraints: TraceConstraints {
                min_width: constraints.min_width,
                max_width: match &constraints.max_width {
                    AxisBound::Bounded(v) => Some(*v),
                    _ => None,
                },
                min_height: constraints.min_height,
                max_height: match &constraints.max_height {
                    AxisBound::Bounded(v) => Some(*v),
                    _ => None,
                },
            },
        });

        let max_w = constraints.max_width_f32();
        let n = child_sizes.len();
        let mut positions = Vec::with_capacity(n);

        // Current insertion point within the active row.
        let mut cursor_x = 0.0_f32;
        // Top of the active row.
        let mut cursor_y = 0.0_f32;
        // Tallest item in the active row.
        let mut row_height = 0.0_f32;
        // Whether the active row is empty.
        let mut is_first_in_row = true;

        for size in child_sizes {
            // Width needed to place this item (including gap if not first).
            let x_with_gap = if is_first_in_row {
                0.0
            } else {
                cursor_x + self.spacing
            };

            // Wrap if the item would overflow the row (but always place if the
            // row is empty — otherwise infinitely wide items would loop forever).
            if !is_first_in_row && x_with_gap + size.width > max_w {
                cursor_y += row_height + self.run_spacing;
                cursor_x = 0.0;
                row_height = 0.0;
                is_first_in_row = true;
            }

            let pos_x = if is_first_in_row {
                0.0
            } else {
                cursor_x + self.spacing
            };

            positions.push(Point { x: pos_x, y: cursor_y });
            cursor_x = pos_x + size.width;
            row_height = row_height.max(size.height);
            is_first_in_row = false;
        }

        let total_height = cursor_y + row_height;
        // Use the constrained maximum width as the container width when finite.
        let total_width = if max_w.is_finite() { max_w } else { cursor_x };
        let size = constraints.constrain(Size {
            width: total_width,
            height: total_height,
        });

        let result = LayoutResult {
            size,
            child_positions: positions,
        };

        #[cfg(debug_assertions)]
        trace!(RosaceTrace::LayoutEnd {
            component: ComponentId(0),
            size: result.size,
            duration: start.elapsed(),
        });

        result
    }
}

impl Default for Wrap {
    fn default() -> Self {
        Self::new()
    }
}

impl ChildContainer for Wrap {
    fn child(mut self, element: impl Into<Element>) -> Self {
        self.children.push(element.into());
        self
    }

    fn children<E: Into<Element>>(mut self, elements: Vec<E>) -> Self {
        self.children
            .extend(elements.into_iter().map(Into::into));
        self
    }

    fn prepend(mut self, element: impl Into<Element>) -> Self {
        self.children.insert(0, element.into());
        self
    }
}

impl From<Wrap> for Element {
    fn from(w: Wrap) -> Self {
        Element::Native(NativeElement {
            tag: "Wrap",
            payload: None,
            children: w.children,
            key: None,
        })
    }
}
