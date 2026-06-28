//! IME (Input Method Editor) support for TEZZERA.
//!
//! Provides the data model for CJK and other complex script input.
//! Platform OS integration (winit IME events) is deferred to v1.0.
//!
//! # Example
//! ```rust,ignore
//! use tezzera_ime::{NoopIme, ImeHandler, ImeEvent};
//!
//! let mut ime = NoopIme::new();
//! ime.on_ime_event(&ImeEvent::Enabled);
//! ime.on_ime_event(&ImeEvent::Preedit {
//!     text: "にほん".to_string(),
//!     cursor_range: Some((0, 9)),
//! });
//! ime.on_ime_event(&ImeEvent::Commit("日本".to_string()));
//! assert_eq!(ime.last_committed(), Some("日本"));
//! ```

pub mod composition;
pub mod event;
pub mod handler;
pub mod state;

pub use composition::ImeComposition;
pub use event::ImeEvent;
pub use handler::{ImeHandler, NoopIme};
pub use state::ImeState;
