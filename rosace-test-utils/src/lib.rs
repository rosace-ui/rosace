//! Test utilities for ROSACE widget and rendering tests.
//!
//! Provides `WidgetEnv` (a headless render canvas), `EventSim` (input event
//! simulation), and `SnapshotAssert` (PNG pixel comparison).
//!
//! This crate is test infrastructure — use it only in `[dev-dependencies]`.
//!
//! # Example
//! ```rust,ignore
//! use rosace_test_utils::{WidgetEnv, EventSim, SnapshotAssert};
//!
//! let mut env = WidgetEnv::new(320, 240);
//! env.render_text("Hello", 10.0, 30.0, 16.0);
//! let png = env.encode_png();
//! let color = env.pixel_at(10, 15);
//!
//! let events = EventSim::tap(50.0, 50.0);
//! assert_eq!(events.len(), 2);
//! ```

pub mod env;
pub mod eventsim;
pub mod snapshot;

pub use env::WidgetEnv;
pub use eventsim::EventSim;
pub use snapshot::SnapshotAssert;
