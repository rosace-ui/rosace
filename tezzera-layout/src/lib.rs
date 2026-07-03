//! `tezzera-layout` — Phase 1 layout engine and widgets for TEZZERA.
//!
//! This crate provides:
//! - The [`Flexure`] constraint-based layout engine.
//! - Constraint helpers re-exported from `tezzera-core` ([`Constraints`], [`AxisBound`]).
//! - Sizing enumerations ([`Width`], [`Height`]).
//! - Alignment enumerations ([`MainAxisAlignment`], [`CrossAxisAlignment`]).
//! - A full set of Phase 1 layout widgets (see [`widgets`]).

pub mod alignment;
pub mod constraints;
pub mod flexure;
pub mod layout_result;
pub mod sizing;
pub mod widgets;

pub use alignment::{CrossAxisAlignment, MainAxisAlignment};
pub use constraints::{AxisBound, Constraints};
pub use flexure::Flexure;
pub use layout_result::LayoutResult;
pub use sizing::{Height, Width};
pub use widgets::flex::{layout_column, layout_row};
pub use widgets::{
    aspect_ratio::AspectRatio,
    flex::{Flex, FlexDirection},
    grid::Grid,
    wrap::Wrap,
};

#[cfg(test)]
mod tests {
    use tezzera_core::types::Size;

    use crate::{
        alignment::{CrossAxisAlignment, MainAxisAlignment},
        constraints::Constraints,
        sizing::Width,
        widgets::{flex::{layout_column, layout_row}, grid::Grid, wrap::Wrap},
    };

    #[test]
    fn column_stacks_children_vertically() {
        let child_sizes = vec![
            Size { width: 100.0, height: 30.0 },
            Size { width: 80.0, height: 40.0 },
        ];
        let result = layout_column(
            Constraints::loose(400.0, 600.0), &child_sizes,
            MainAxisAlignment::Start, CrossAxisAlignment::Start, 8.0,
        );
        // Second child should be placed below first + spacing.
        assert_eq!(result.child_positions[1].y, 30.0 + 8.0);
    }

    #[test]
    fn row_stacks_children_horizontally() {
        let child_sizes = vec![
            Size { width: 50.0, height: 20.0 },
            Size { width: 60.0, height: 20.0 },
        ];
        let result = layout_row(
            Constraints::loose(400.0, 100.0), &child_sizes,
            MainAxisAlignment::Start, CrossAxisAlignment::Start, 4.0,
        );
        assert_eq!(result.child_positions[1].x, 50.0 + 4.0);
    }

    #[test]
    fn constraints_constrain_clamps_correctly() {
        let c = Constraints::loose(100.0, 100.0);
        let big = Size { width: 200.0, height: 200.0 };
        let clamped = c.constrain(big);
        assert_eq!(clamped.width, 100.0);
        assert_eq!(clamped.height, 100.0);
    }

    #[test]
    fn constraints_loose_has_zero_min() {
        let c = Constraints::loose(800.0, 600.0);
        assert_eq!(c.min_width, 0.0);
        assert_eq!(c.min_height, 0.0);
    }

    #[test]
    fn fraction_width_is_of_available_space() {
        let w = Width::Fraction(0.5);
        let bound = w.to_axis_bound(200.0);
        assert!(
            matches!(bound, tezzera_core::render_object::AxisBound::Bounded(v) if (v - 100.0).abs() < 0.001)
        );
    }

    #[test]
    fn grid_positions_children_in_rows() {
        let grid = Grid::new(3).spacing(0.0);
        let sizes = vec![Size { width: 50.0, height: 50.0 }; 6];
        let result = grid.layout(Constraints::loose(200.0, 400.0), &sizes);
        // 4th item (index 3) should be in the second row.
        assert_eq!(result.child_positions[3].x, 0.0);
        assert!(result.child_positions[3].y > 0.0);
    }

    #[test]
    fn wrap_wraps_to_next_line_when_full() {
        let wrap = Wrap::new().spacing(0.0);
        let sizes = vec![Size { width: 60.0, height: 30.0 }; 4];
        let result = wrap.layout(Constraints::loose(150.0, 400.0), &sizes);
        // With width 150 and items 60 wide, only 2 fit per row; 3rd item wraps.
        assert!(result.child_positions[2].y > 0.0);
    }
}
