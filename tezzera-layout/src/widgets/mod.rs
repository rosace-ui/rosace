//! Layout math for TEZZERA (D095).
//!
//! The element-based widget structs were removed — `tezzera-widgets::tree`
//! is the canonical widget set. This crate keeps the pure algorithms:
//! flex (layout_column/layout_row), grid, wrap, aspect_ratio.

pub mod aspect_ratio;
pub mod flex;
pub mod grid;
pub mod wrap;
