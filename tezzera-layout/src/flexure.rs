//! [`Flexure`] — TEZZERA's constraint-based layout engine.

use tezzera_core::types::Size;

use crate::constraints::Constraints;
use crate::layout_result::LayoutResult;

/// TEZZERA's constraint-based layout engine.
///
/// Runs a three-pass layout:
/// 1. **Measure** — top-down constraint propagation (driven by this struct)
/// 2. **Place** — bottom-up size resolution (driven by this struct)
/// 3. **Paint** — handled by `tezzera-render`
///
/// In Phase 1 the engine is invoked manually per-widget; automatic tree
/// traversal is wired up in a later step.
pub struct Flexure;

impl Flexure {
    /// Run a layout pass for a *leaf* node that has no children.
    ///
    /// The `natural_size` is clamped to `constraints` and returned inside a
    /// [`LayoutResult`] with an empty `child_positions` list.
    pub fn layout_leaf(constraints: Constraints, natural_size: Size) -> LayoutResult {
        let size = constraints.constrain(natural_size);
        LayoutResult {
            size,
            child_positions: vec![],
        }
    }
}
