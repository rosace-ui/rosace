pub mod clock;
pub mod controller;
pub mod easing;
pub mod keyframe;
pub mod lerp;
pub mod spring;
pub mod tween;
pub mod spring_hook;

pub use clock::{frame_dt, set_frame_dt};
pub use controller::{AnimationController, AnimationState};
pub use easing::Easing;
pub use keyframe::{Keyframe, KeyframeStop};
pub use lerp::Lerp;
pub use spring::Spring;
pub use tween::Tween;
pub use spring_hook::{use_spring, Animated, SpringController, SpringState};
