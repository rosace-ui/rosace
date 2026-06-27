//! [`Width`] and [`Height`] sizing enumerations for declarative widget sizing.

use tezzera_core::render_object::AxisBound;

/// How a widget sizes itself on the *horizontal* axis.
#[derive(Debug, Clone)]
pub enum Width {
    /// Exactly `f32` logical pixels wide.
    Fixed(f32),
    /// Expand to fill all available width.
    Fill,
    /// Shrink to fit intrinsic content width.
    Shrink,
    /// A fraction `0.0–1.0` of the parent's available width.
    Fraction(f32),
    /// At least `f32` pixels wide; may grow larger.
    Min(f32),
    /// At most `f32` pixels wide; may be smaller.
    Max(f32),
    /// Clamped between `(min, max)` pixels.
    Range(f32, f32),
}

/// How a widget sizes itself on the *vertical* axis.
#[derive(Debug, Clone)]
pub enum Height {
    /// Exactly `f32` logical pixels tall.
    Fixed(f32),
    /// Expand to fill all available height.
    Fill,
    /// Shrink to fit intrinsic content height.
    Shrink,
    /// A fraction `0.0–1.0` of the parent's available height.
    Fraction(f32),
    /// At least `f32` pixels tall; may grow larger.
    Min(f32),
    /// At most `f32` pixels tall; may be smaller.
    Max(f32),
    /// Clamped between `(min, max)` pixels.
    Range(f32, f32),
}

impl Width {
    /// Convert to an [`AxisBound`] given the parent's `available` width in logical pixels.
    pub fn to_axis_bound(&self, available: f32) -> AxisBound {
        match self {
            Width::Fixed(v) => AxisBound::Bounded(*v),
            Width::Fill => AxisBound::Bounded(available),
            Width::Shrink => AxisBound::Shrink,
            Width::Fraction(f) => AxisBound::Bounded(available * f),
            Width::Min(v) => AxisBound::Bounded(*v),
            Width::Max(v) => AxisBound::Bounded(*v),
            Width::Range(_, max) => AxisBound::Bounded(*max),
        }
    }
}

impl Height {
    /// Convert to an [`AxisBound`] given the parent's `available` height in logical pixels.
    pub fn to_axis_bound(&self, available: f32) -> AxisBound {
        match self {
            Height::Fixed(v) => AxisBound::Bounded(*v),
            Height::Fill => AxisBound::Bounded(available),
            Height::Shrink => AxisBound::Shrink,
            Height::Fraction(f) => AxisBound::Bounded(available * f),
            Height::Min(v) => AxisBound::Bounded(*v),
            Height::Max(v) => AxisBound::Bounded(*v),
            Height::Range(_, max) => AxisBound::Bounded(*max),
        }
    }
}
