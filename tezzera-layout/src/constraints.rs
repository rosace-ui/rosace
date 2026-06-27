//! Re-exports of [`Constraints`] and [`AxisBound`] from `tezzera-core`.
//!
//! Helper methods (`max_width_f32`, `max_height_f32`, `constrain`, `is_tight`)
//! are defined directly on `Constraints` inside `tezzera-core` so that they are
//! available across the whole workspace without orphan-rule issues.

/// An axis bound used inside [`Constraints`].
pub use tezzera_core::render_object::AxisBound;

/// Layout constraints passed down the render tree during the measure pass.
///
/// See also the helper methods added in `tezzera-core`:
/// - [`Constraints::max_width_f32`]
/// - [`Constraints::max_height_f32`]
/// - [`Constraints::constrain`]
/// - [`Constraints::is_tight`]
pub use tezzera_core::render_object::Constraints;
