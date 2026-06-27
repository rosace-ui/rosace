//! [`Stack`] — a Z-axis overlay container.

use tezzera_core::child_container::ChildContainer;
use tezzera_core::element::{Element, NativeElement};
use tezzera_core::render_object::AxisBound;
use tezzera_core::types::{Point, Size};
use tezzera_trace::{
    event::{ComponentId, TezzeraTrace, TraceConstraints},
    trace,
};

use crate::constraints::Constraints;
use crate::layout_result::LayoutResult;

/// A widget that overlays all of its children on the Z axis.
///
/// Every child is placed at the origin `(0, 0)`.  The resolved size of the
/// stack is the bounding box of all child sizes.
#[derive(Debug, Clone)]
pub struct Stack {
    children: Vec<Element>,
}

impl Stack {
    /// Create a new empty `Stack`.
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    /// Perform the Measure + Place passes and return a [`LayoutResult`].
    ///
    /// All children are placed at the origin.  The stack's size equals the
    /// maximum width and height across all children, clamped to `constraints`.
    ///
    /// Emits [`TezzeraTrace::LayoutStart`] and [`TezzeraTrace::LayoutEnd`] events.
    pub fn layout(&self, constraints: Constraints, child_sizes: &[Size]) -> LayoutResult {
        let start = std::time::Instant::now();

        trace!(TezzeraTrace::LayoutStart {
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

        let max_w = child_sizes
            .iter()
            .map(|s| s.width)
            .fold(0.0_f32, f32::max);
        let max_h = child_sizes
            .iter()
            .map(|s| s.height)
            .fold(0.0_f32, f32::max);
        let size = constraints.constrain(Size {
            width: max_w,
            height: max_h,
        });
        let child_positions = child_sizes
            .iter()
            .map(|_| Point { x: 0.0, y: 0.0 })
            .collect();

        let result = LayoutResult {
            size,
            child_positions,
        };

        trace!(TezzeraTrace::LayoutEnd {
            component: ComponentId(0),
            size: result.size,
            duration: start.elapsed(),
        });

        result
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

impl ChildContainer for Stack {
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

impl From<Stack> for Element {
    fn from(s: Stack) -> Self {
        Element::Native(NativeElement {
            tag: "Stack",
            children: s.children,
        })
    }
}
