//! [`Grid`] — a fixed-column grid layout.

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

/// A widget that arranges its children in a grid with a fixed number of columns.
///
/// Row heights are determined by the tallest child in each row; column widths
/// by the widest child in each column.  Use [`spacing`](Self::spacing) to add
/// a uniform gap between cells on both axes.
#[derive(Debug, Clone)]
pub struct Grid {
    children: Vec<Element>,
    columns: usize,
    spacing: f32,
}

impl Grid {
    /// Create a new `Grid` with the given number of `columns` and zero spacing.
    pub fn new(columns: usize) -> Self {
        Self {
            children: Vec::new(),
            columns: columns.max(1),
            spacing: 0.0,
        }
    }

    /// Set the gap in logical pixels between cells (applied horizontally and vertically).
    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }

    /// Perform the Measure + Place passes and return a [`LayoutResult`].
    ///
    /// `child_sizes` must be in the same order as children appended via
    /// [`ChildContainer::child`] / [`ChildContainer::children`].
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

        let n = child_sizes.len();
        let result = if n == 0 {
            LayoutResult {
                size: constraints.constrain(Size {
                    width: 0.0,
                    height: 0.0,
                }),
                child_positions: vec![],
            }
        } else {
            let cols = self.columns;
            let rows = n.div_ceil(cols);

            // Maximum width per column.
            let mut col_widths = vec![0.0_f32; cols];
            for (i, size) in child_sizes.iter().enumerate() {
                let col = i % cols;
                col_widths[col] = col_widths[col].max(size.width);
            }

            // Maximum height per row.
            let mut row_heights = vec![0.0_f32; rows];
            for (i, size) in child_sizes.iter().enumerate() {
                let row = i / cols;
                row_heights[row] = row_heights[row].max(size.height);
            }

            // X offset for each column.
            let mut col_offsets = vec![0.0_f32; cols];
            for c in 1..cols {
                col_offsets[c] = col_offsets[c - 1] + col_widths[c - 1] + self.spacing;
            }

            // Y offset for each row.
            let mut row_offsets = vec![0.0_f32; rows];
            for r in 1..rows {
                row_offsets[r] = row_offsets[r - 1] + row_heights[r - 1] + self.spacing;
            }

            let mut positions = Vec::with_capacity(n);
            for i in 0..n {
                let col = i % cols;
                let row = i / cols;
                positions.push(Point {
                    x: col_offsets[col],
                    y: row_offsets[row],
                });
            }

            let total_w: f32 =
                col_widths.iter().sum::<f32>() + self.spacing * (cols - 1) as f32;
            let total_h: f32 =
                row_heights.iter().sum::<f32>() + self.spacing * (rows - 1) as f32;

            LayoutResult {
                size: constraints.constrain(Size {
                    width: total_w,
                    height: total_h,
                }),
                child_positions: positions,
            }
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

impl ChildContainer for Grid {
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

impl From<Grid> for Element {
    fn from(g: Grid) -> Self {
        Element::Native(NativeElement {
            tag: "Grid",
            payload: None,
            children: g.children,
            key: None,
        })
    }
}
