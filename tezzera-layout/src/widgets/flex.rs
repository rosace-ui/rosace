//! [`Flex`] — the generic flex container used internally by [`Column`] and [`Row`].
//!
//! [`Column`]: crate::widgets::column::Column
//! [`Row`]: crate::widgets::row::Row

use tezzera_core::child_container::ChildContainer;
use tezzera_core::element::{Element, NativeElement};
use tezzera_core::render_object::AxisBound;
use tezzera_core::types::{Point, Size};
use tezzera_trace::{
    event::{ComponentId, TezzeraTrace, TraceConstraints},
    trace,
};

use crate::alignment::{CrossAxisAlignment, MainAxisAlignment};
use crate::constraints::Constraints;
use crate::layout_result::LayoutResult;

/// The axis along which a [`Flex`] container arranges its children.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlexDirection {
    /// Arrange children left-to-right (horizontal main axis).
    Row,
    /// Arrange children top-to-bottom (vertical main axis).
    Column,
}

/// A generic flex container that drives the layout of [`Column`] and [`Row`].
///
/// Prefer the higher-level [`Column`] and [`Row`] widgets for typical use.
/// Use `Flex` directly when you need runtime direction switching.
///
/// [`Column`]: crate::widgets::column::Column
/// [`Row`]: crate::widgets::row::Row
#[derive(Debug, Clone)]
pub struct Flex {
    /// The primary layout direction.
    pub direction: FlexDirection,
    /// Child elements in declaration order.
    pub children: Vec<Element>,
    /// How children are distributed along the main axis.
    pub main_axis_alignment: MainAxisAlignment,
    /// How children are aligned on the cross axis.
    pub cross_axis_alignment: CrossAxisAlignment,
    /// Pixels of gap between consecutive children.
    pub spacing: f32,
}

impl Flex {
    /// Create a new `Flex` with the given `direction` and default alignments.
    pub fn new(direction: FlexDirection) -> Self {
        Self {
            direction,
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

    /// Set how children are distributed along the main axis.
    pub fn main_axis_alignment(mut self, a: MainAxisAlignment) -> Self {
        self.main_axis_alignment = a;
        self
    }

    /// Set how children are aligned on the cross axis.
    pub fn cross_axis_alignment(mut self, a: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = a;
        self
    }

    /// Perform the Measure + Place passes and return a [`LayoutResult`].
    ///
    /// `child_sizes` must be in the same order as [`Self::children`].
    /// Emits [`TezzeraTrace::LayoutStart`] / [`TezzeraTrace::LayoutEnd`] events.
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

        let result = self.layout_inner(constraints, child_sizes);

        trace!(TezzeraTrace::LayoutEnd {
            component: ComponentId(0),
            size: result.size,
            duration: start.elapsed(),
        });

        result
    }

    /// Inner layout without trace emissions — used by [`Column`] and [`Row`]
    /// which emit their own traces.
    ///
    /// [`Column`]: crate::widgets::column::Column
    /// [`Row`]: crate::widgets::row::Row
    pub(crate) fn layout_inner(
        &self,
        constraints: Constraints,
        child_sizes: &[Size],
    ) -> LayoutResult {
        match self.direction {
            FlexDirection::Column => layout_column(
                constraints,
                child_sizes,
                self.main_axis_alignment,
                self.cross_axis_alignment,
                self.spacing,
            ),
            FlexDirection::Row => layout_row(
                constraints,
                child_sizes,
                self.main_axis_alignment,
                self.cross_axis_alignment,
                self.spacing,
            ),
        }
    }
}

/// Core column layout algorithm shared by [`Flex`] and [`Column`].
pub(crate) fn layout_column(
    constraints: Constraints,
    child_sizes: &[Size],
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
    spacing: f32,
) -> LayoutResult {
    let n = child_sizes.len();
    if n == 0 {
        return LayoutResult {
            size: constraints.constrain(Size {
                width: 0.0,
                height: 0.0,
            }),
            child_positions: vec![],
        };
    }

    let max_w = constraints.max_width_f32();
    let max_h = constraints.max_height_f32();

    // Cross axis (width): max of child widths, or full available if Stretch.
    let content_width = child_sizes
        .iter()
        .map(|s| s.width)
        .fold(0.0_f32, f32::max);
    let container_width = match cross_axis_alignment {
        CrossAxisAlignment::Stretch if max_w.is_finite() => max_w,
        _ => content_width,
    }
    .max(constraints.min_width)
    .min(max_w);

    // Main axis (height): sum of heights + spacing.
    let total_child_height: f32 = child_sizes.iter().map(|s| s.height).sum();
    let total_spacing = spacing * (n - 1) as f32;
    let content_height = total_child_height + total_spacing;
    let container_height = content_height
        .max(constraints.min_height)
        .min(max_h);

    let extra = (container_height - content_height).max(0.0);
    let (initial_offset, between_gap) = distribute_extra(main_axis_alignment, extra, n);

    let mut positions = Vec::with_capacity(n);
    let mut y = initial_offset;

    for (i, child_size) in child_sizes.iter().enumerate() {
        let x = cross_offset(cross_axis_alignment, container_width, child_size.width);
        positions.push(Point { x, y });
        y += child_size.height;
        if i + 1 < n {
            y += spacing + between_gap;
        }
    }

    LayoutResult {
        size: Size {
            width: container_width,
            height: container_height,
        },
        child_positions: positions,
    }
}

/// Core row layout algorithm shared by [`Flex`] and [`Row`].
pub(crate) fn layout_row(
    constraints: Constraints,
    child_sizes: &[Size],
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
    spacing: f32,
) -> LayoutResult {
    let n = child_sizes.len();
    if n == 0 {
        return LayoutResult {
            size: constraints.constrain(Size {
                width: 0.0,
                height: 0.0,
            }),
            child_positions: vec![],
        };
    }

    let max_w = constraints.max_width_f32();
    let max_h = constraints.max_height_f32();

    // Cross axis (height): max of child heights, or full available if Stretch.
    let content_height = child_sizes
        .iter()
        .map(|s| s.height)
        .fold(0.0_f32, f32::max);
    let container_height = match cross_axis_alignment {
        CrossAxisAlignment::Stretch if max_h.is_finite() => max_h,
        _ => content_height,
    }
    .max(constraints.min_height)
    .min(max_h);

    // Main axis (width): sum of widths + spacing.
    let total_child_width: f32 = child_sizes.iter().map(|s| s.width).sum();
    let total_spacing = spacing * (n - 1) as f32;
    let content_width = total_child_width + total_spacing;
    let container_width = content_width
        .max(constraints.min_width)
        .min(max_w);

    let extra = (container_width - content_width).max(0.0);
    let (initial_offset, between_gap) = distribute_extra(main_axis_alignment, extra, n);

    let mut positions = Vec::with_capacity(n);
    let mut x = initial_offset;

    for (i, child_size) in child_sizes.iter().enumerate() {
        let y = cross_offset(cross_axis_alignment, container_height, child_size.height);
        positions.push(Point { x, y });
        x += child_size.width;
        if i + 1 < n {
            x += spacing + between_gap;
        }
    }

    LayoutResult {
        size: Size {
            width: container_width,
            height: container_height,
        },
        child_positions: positions,
    }
}

/// Compute `(initial_offset, per_gap_extra)` for a given main-axis alignment.
///
/// - `extra`: total remaining space after placing children and fixed spacing.
/// - `n`: number of children.
pub(crate) fn distribute_extra(
    alignment: MainAxisAlignment,
    extra: f32,
    n: usize,
) -> (f32, f32) {
    match alignment {
        MainAxisAlignment::Start => (0.0, 0.0),
        MainAxisAlignment::Center => (extra / 2.0, 0.0),
        MainAxisAlignment::End => (extra, 0.0),
        MainAxisAlignment::SpaceBetween => {
            let gap = if n > 1 {
                extra / (n - 1) as f32
            } else {
                0.0
            };
            (0.0, gap)
        }
        MainAxisAlignment::SpaceAround => {
            let unit = if n > 0 { extra / n as f32 } else { 0.0 };
            (unit / 2.0, unit)
        }
        MainAxisAlignment::SpaceEvenly => {
            let unit = extra / (n + 1) as f32;
            (unit, unit)
        }
    }
}

/// Compute the cross-axis offset for one child given the container size and child size.
pub(crate) fn cross_offset(alignment: CrossAxisAlignment, container: f32, child: f32) -> f32 {
    match alignment {
        CrossAxisAlignment::Start | CrossAxisAlignment::Stretch | CrossAxisAlignment::Baseline => {
            0.0
        }
        CrossAxisAlignment::Center => (container - child) / 2.0,
        CrossAxisAlignment::End => container - child,
    }
}

impl ChildContainer for Flex {
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

impl From<Flex> for Element {
    fn from(f: Flex) -> Self {
        Element::Native(NativeElement {
            tag: "Flex",
            children: f.children,
        })
    }
}
