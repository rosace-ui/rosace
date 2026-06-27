//! Momentum scroll view for TEZZERA.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use tezzera_scroll::{ScrollView, ScrollDirection, ScrollPhysics};
//!
//! let mut sv = ScrollView::new(400.0, 600.0)
//!     .content_size(400.0, 2000.0)
//!     .direction(ScrollDirection::Vertical)
//!     .physics(ScrollPhysics::Momentum { friction: 0.92 });
//!
//! // On scroll input:
//! sv.on_scroll(0.0, 15.0);
//!
//! // Each frame:
//! sv.tick(0.016);
//!
//! // Translate content by the returned offset before drawing:
//! let [_ox, _oy] = sv.scroll_offset();
//! ```

pub mod controller;
pub mod physics;
pub mod scrollbar;
pub mod scroll_view;

pub use controller::ScrollController;
pub use physics::{
    clamp_offset, snap_to_page, MomentumState, ScrollDirection, ScrollPhysics,
};
pub use scrollbar::{render_scrollbar, ScrollbarMetrics};
pub use scroll_view::ScrollView;
