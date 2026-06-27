//! [`SizedBox`] — a fixed-size container.

use tezzera_core::element::{Element, NativeElement};
use tezzera_core::render_object::AxisBound;
use tezzera_core::types::Size;
use tezzera_trace::{
    event::{ComponentId, TezzeraTrace, TraceConstraints},
    trace,
};

use crate::constraints::Constraints;
use crate::layout_result::LayoutResult;

/// A widget that forces its child into a specific size.
///
/// Omitting `width` or `height` lets the constraint's maximum fill in.
/// The resolved size is always clamped to the incoming [`Constraints`].
#[derive(Debug, Clone)]
pub struct SizedBox {
    width: Option<f32>,
    height: Option<f32>,
    child: Option<Element>,
}

impl SizedBox {
    /// Create a new `SizedBox` with no fixed dimensions and no child.
    pub fn new() -> Self {
        Self {
            width: None,
            height: None,
            child: None,
        }
    }

    /// Set the fixed width in logical pixels.
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    /// Set the fixed height in logical pixels.
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }

    /// Set the single child element.
    pub fn child(mut self, e: impl Into<Element>) -> Self {
        self.child = Some(e.into());
        self
    }

    /// Perform the Measure pass and return a [`LayoutResult`].
    ///
    /// The box's size is `(width, height)` if set, otherwise the constraint
    /// maximum on each axis, clamped to `constraints`.
    ///
    /// Emits [`TezzeraTrace::LayoutStart`] and [`TezzeraTrace::LayoutEnd`] events.
    pub fn layout(&self, constraints: Constraints) -> LayoutResult {
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

        let w = self.width.unwrap_or_else(|| constraints.max_width_f32());
        let h = self.height.unwrap_or_else(|| constraints.max_height_f32());
        let size = constraints.constrain(Size {
            width: w,
            height: h,
        });

        let result = LayoutResult {
            size,
            child_positions: vec![],
        };

        trace!(TezzeraTrace::LayoutEnd {
            component: ComponentId(0),
            size: result.size,
            duration: start.elapsed(),
        });

        result
    }
}

impl Default for SizedBox {
    fn default() -> Self {
        Self::new()
    }
}

impl From<SizedBox> for Element {
    fn from(sb: SizedBox) -> Self {
        let children = sb.child.map(|c| vec![c]).unwrap_or_default();
        Element::Native(NativeElement {
            tag: "SizedBox",
            children,
        })
    }
}
