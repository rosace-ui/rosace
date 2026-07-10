//! Animation system for ROSACE — pure math, driven by external dt.
//!
//! # Example
//! ```rust,ignore
//! use rosace_anim::{Tween, Easing, AnimationController};
//!
//! let tween = Tween::new(0.0_f32, 100.0, 0.5, Easing::EaseInOut);
//! let mut ctrl = AnimationController::new(tween);
//! ctrl.start();
//! let v = ctrl.tick(0.25); // ~50.0 (at halfway through 0.5s)
//! ```

pub mod easing;
pub mod lerp;
pub mod timeline;
pub mod tween;

pub use easing::{easing_fn, Easing};
pub use lerp::Lerp;
pub use timeline::{Keyframe, Timeline};
pub use tween::{AnimationController, AnimationState, Tween};
