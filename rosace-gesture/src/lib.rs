//! Gesture recognition for ROSACE.
//!
//! Converts raw `InputEvent`s into high-level gesture events.
//!
//! # Example
//! ```rust,ignore
//! use rosace_gesture::{TapRecognizer, GestureRecognizer, GestureEvent};
//!
//! let mut tap = TapRecognizer::new();
//! // In render loop:
//! for ev in events {
//!     if let Some(gesture) = tap.on_event(ev, dt) {
//!         match gesture {
//!             GestureEvent::Tap { x, y } => println!("tapped at ({x}, {y})"),
//!             GestureEvent::DoubleTap { x, y } => println!("double-tapped"),
//!             _ => {}
//!         }
//!     }
//! }
//! ```

pub mod drag;
pub mod event;
pub mod pinch;
pub mod recognizer;
pub mod swipe;
pub mod tap;

pub use drag::DragRecognizer;
pub use event::{DragPhase, GestureEvent, SwipeDirection};
pub use pinch::PinchRecognizer;
pub use recognizer::GestureRecognizer;
pub use swipe::SwipeRecognizer;
pub use tap::TapRecognizer;
