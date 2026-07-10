//! Alignment enumerations for flex-style layout axes.

/// How children are distributed along the *main* axis of a flex container.
///
/// The main axis of a [`Column`](crate::widgets::column::Column) is vertical;
/// for a [`Row`](crate::widgets::row::Row) it is horizontal.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum MainAxisAlignment {
    /// Pack children at the start of the main axis.
    #[default]
    Start,
    /// Center children on the main axis.
    Center,
    /// Pack children at the end of the main axis.
    End,
    /// Distribute remaining space evenly *between* children (no leading/trailing space).
    SpaceBetween,
    /// Distribute remaining space evenly *around* children (half-space at each edge).
    SpaceAround,
    /// Distribute remaining space evenly *between and around* children.
    SpaceEvenly,
}

/// How children are aligned on the *cross* axis of a flex container.
///
/// The cross axis of a [`Column`](crate::widgets::column::Column) is horizontal;
/// for a [`Row`](crate::widgets::row::Row) it is vertical.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CrossAxisAlignment {
    /// Align children at the start of the cross axis.
    #[default]
    Start,
    /// Center children on the cross axis.
    Center,
    /// Align children at the end of the cross axis.
    End,
    /// Stretch children to fill the cross axis.
    Stretch,
    /// Align children by their text baseline (falls back to [`Start`](Self::Start)
    /// in Phase 1 before text metrics are available).
    Baseline,
}

