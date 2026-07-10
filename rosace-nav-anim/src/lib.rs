//! Animated screen transitions for ROSACE navigation.
//!
//! # Example
//! ```rust,ignore
//! use rosace_nav_anim::{NavigatorAnimated, TransitionStyle, SlideDirection};
//!
//! let mut nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
//! nav.push_animated(Screen::Detail, TransitionStyle::Slide(SlideDirection::Right));
//!
//! // In render loop (dt = 1.0/60.0):
//! let (ex, ey, ox, oy, progress, done) = nav.update(dt);
//! ```

pub mod navigator;
pub mod transition;

pub use navigator::NavigatorAnimated;
pub use transition::{ScreenTransition, SlideDirection, TransitionStyle};
