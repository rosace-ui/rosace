//! [`Column`] — a vertical flex container.

use tezzera_core::child_container::ChildContainer;
use tezzera_core::element::{Element, NativeElement};
use tezzera_core::render_object::AxisBound;
use tezzera_core::types::Size;
use tezzera_trace::{
    event::{ComponentId, TezzeraTrace, TraceConstraints},
    trace,
};

use crate::alignment::{CrossAxisAlignment, MainAxisAlignment};
use crate::constraints::Constraints;
use crate::layout_result::LayoutResult;
use crate::widgets::flex::layout_column;

/// A widget that stacks its children vertically (top to bottom).
///
/// Spacing, main-axis alignment, and cross-axis alignment are all configurable
/// via the builder API.  Pass pre-measured child sizes to [`Self::layout`] to
/// get back a [`LayoutResult`] containing the resolved container size and each
/// child's position.
#[derive(Debug, Clone)]
pub struct Column {
    children: Vec<Element>,
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
    spacing: f32,
}

impl Column {
    /// Create a new `Column` with default alignments and zero spacing.
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            main_axis_alignment: MainAxisAlignment::default(),
            cross_axis_alignment: CrossAxisAlignment::default(),
            spacing: 0.0,
        }
    }

    /// Set the gap in logical pixels between consecutive children.
    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }

    /// Set how children are distributed along the vertical main axis.
    pub fn main_axis_alignment(mut self, a: MainAxisAlignment) -> Self {
        self.main_axis_alignment = a;
        self
    }

    /// Set how children are aligned on the horizontal cross axis.
    pub fn cross_axis_alignment(mut self, a: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = a;
        self
    }

    /// Perform the Measure + Place passes and return a [`LayoutResult`].
    ///
    /// `child_sizes` must be in the same order as children appended via
    /// [`ChildContainer::child`] / [`ChildContainer::children`].
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

        let result = layout_column(
            constraints,
            child_sizes,
            self.main_axis_alignment,
            self.cross_axis_alignment,
            self.spacing,
        );

        trace!(TezzeraTrace::LayoutEnd {
            component: ComponentId(0),
            size: result.size,
            duration: start.elapsed(),
        });

        result
    }
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl ChildContainer for Column {
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

impl From<Column> for Element {
    fn from(c: Column) -> Self {
        Element::Native(NativeElement {
            tag: "Column",
            children: c.children,
        })
    }
}
