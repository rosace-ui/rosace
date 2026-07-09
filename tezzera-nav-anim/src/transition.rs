//! Re-exports the screen-transition spring physics, which moved to
//! `tezzera-nav` in D108/Phase 26 Step 3 so `ScreenNav` (the type real apps
//! actually use) could drive it directly without a circular crate
//! dependency (`tezzera-nav-anim` already depends on `tezzera-nav`).
//! `NavigatorAnimated` (unwired to anything real) still compiles against
//! these same public names.

pub use tezzera_nav::transition::{ScreenTransition, SlideDirection, TransitionStyle};
