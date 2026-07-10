//! [`AspectRatio`] — constrains a child to a fixed width-to-height ratio.

use rosace_core::element::{Element, NativeElement};
#[cfg(debug_assertions)]
use rosace_core::render_object::AxisBound;
use rosace_core::types::Size;
#[cfg(debug_assertions)]
use rosace_trace::{
    event::{ComponentId, RosaceTrace, TraceConstraints},
    trace,
};

use crate::constraints::Constraints;

/// A widget that forces its child into a fixed aspect ratio (`width / height`).
///
/// The widget attempts to use the maximum available width first.  If the
/// resulting height would exceed the constraint it falls back to fitting within
/// the maximum height instead.
#[derive(Debug, Clone)]
pub struct AspectRatio {
    /// Desired ratio expressed as `width / height`.  Must be positive.
    ratio: f32,
    child: Option<Element>,
}

impl AspectRatio {
    /// Create an `AspectRatio` with the given `ratio` (`width / height`).
    pub fn new(ratio: f32) -> Self {
        Self { ratio, child: None }
    }

    /// Set the single child element.
    pub fn child(mut self, e: impl Into<Element>) -> Self {
        self.child = Some(e.into());
        self
    }

    /// Compute the size that satisfies [`Self::ratio`] within `constraints`.
    ///
    /// Tries width-first; if the resulting height exceeds the constraint maximum
    /// it retries height-first.
    ///
    /// Emits [`RosaceTrace::LayoutStart`] and [`RosaceTrace::LayoutEnd`] events.
    pub fn layout(&self, constraints: Constraints) -> Size {
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
        let max_h = constraints.max_height_f32();
        let ratio = self.ratio.max(f32::EPSILON);

        // Try fitting within max width first.
        let width = if max_w.is_finite() { max_w } else { max_h * ratio };
        let height = width / ratio;

        let size = if !max_h.is_finite() || height <= max_h {
            constraints.constrain(Size { width, height })
        } else {
            // Fall back to fitting within max height.
            let height = max_h;
            let width = height * ratio;
            constraints.constrain(Size { width, height })
        };

        #[cfg(debug_assertions)]
        trace!(RosaceTrace::LayoutEnd {
            component: ComponentId(0),
            size,
            duration: start.elapsed(),
        });

        size
    }
}

impl From<AspectRatio> for Element {
    fn from(ar: AspectRatio) -> Self {
        let children = ar.child.map(|c| vec![c]).unwrap_or_default();
        Element::Native(NativeElement {
            tag: "AspectRatio",
            payload: None,
            children,
            key: None,
        })
    }
}
