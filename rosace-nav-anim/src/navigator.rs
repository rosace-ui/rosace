use rosace_nav::{Navigator, Route};
use crate::transition::{ScreenTransition, TransitionStyle};

/// A Navigator<R> augmented with animated transitions.
///
/// Usage:
/// ```rust,ignore
/// let mut nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
/// nav.push_animated(Screen::Detail, TransitionStyle::Slide(SlideDirection::Right));
/// // In your render loop:
/// let (ex, ey, ox, oy, progress, done) = nav.update(dt);
/// // Draw previous screen at offset (ox, oy), current at (ex, ey).
/// ```
pub struct NavigatorAnimated<R: Route> {
    inner: Navigator<R>,
    transition: ScreenTransition,
    previous: Option<R>,
}

impl<R: Route> NavigatorAnimated<R> {
    pub fn new(root: R, viewport_w: f32, viewport_h: f32) -> Self {
        Self {
            inner: Navigator::new(root),
            transition: ScreenTransition::new(viewport_w, viewport_h),
            previous: None,
        }
    }

    /// Push a route with an animated transition.
    pub fn push_animated(&mut self, route: R, style: TransitionStyle) {
        self.previous = self.inner.current();
        self.inner.push(route);
        self.transition.trigger(style);
    }

    /// Pop the current route with an animated transition.
    pub fn pop_animated(&mut self, style: TransitionStyle) -> bool {
        let current = self.inner.current();
        let popped = self.inner.pop();
        if popped {
            self.previous = current;
            self.transition.trigger(style);
        }
        popped
    }

    /// Instant push without animation.
    pub fn push(&mut self, route: R) {
        self.previous = self.inner.current();
        self.inner.push(route);
    }

    /// Instant pop.
    pub fn pop(&mut self) -> bool { self.inner.pop() }

    /// Advance transition physics. Returns (enter_dx, enter_dy, exit_dx, exit_dy, progress, is_complete).
    pub fn update(&mut self, dt: f32) -> (f32, f32, f32, f32, f32, bool) {
        self.transition.update(dt)
    }

    /// Current route.
    pub fn current(&self) -> Option<R> { self.inner.current() }

    /// Previous route (for rendering the outgoing screen).
    pub fn previous(&self) -> Option<&R> { self.previous.as_ref() }

    /// Whether a transition is running.
    pub fn is_transitioning(&self) -> bool { self.transition.is_active() }

    /// Stack depth.
    pub fn depth(&self) -> usize { self.inner.depth() }

    /// Can go back.
    pub fn can_go_back(&self) -> bool { self.inner.can_go_back() }

    /// Full stack.
    pub fn stack(&self) -> Vec<R> { self.inner.stack() }

    /// Resize viewport.
    pub fn set_viewport(&mut self, w: f32, h: f32) { self.transition.set_viewport(w, h); }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transition::SlideDirection;
    use rosace_nav::Route;

    #[derive(Debug, Clone, PartialEq)]
    enum Screen {
        Home,
        Detail,
        Settings,
    }
    impl Route for Screen {}

    #[test]
    fn nav_animated_new() {
        let nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
        assert_eq!(nav.current(), Some(Screen::Home));
        assert!(!nav.is_transitioning());
        assert_eq!(nav.depth(), 1);
    }

    #[test]
    fn nav_animated_push_sets_current() {
        let mut nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
        nav.push(Screen::Detail);
        assert_eq!(nav.current(), Some(Screen::Detail));
        assert_eq!(nav.previous(), Some(&Screen::Home));
    }

    #[test]
    fn nav_animated_push_animated_activates_transition() {
        let mut nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
        nav.push_animated(Screen::Detail, TransitionStyle::Slide(SlideDirection::Right));
        assert_eq!(nav.current(), Some(Screen::Detail));
        assert_eq!(nav.previous(), Some(&Screen::Home));
        assert!(nav.is_transitioning());
    }

    #[test]
    fn nav_animated_pop_goes_back() {
        let mut nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
        nav.push(Screen::Detail);
        let popped = nav.pop();
        assert!(popped);
        assert_eq!(nav.current(), Some(Screen::Home));
    }

    #[test]
    fn nav_animated_pop_animated_activates_transition() {
        let mut nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
        nav.push(Screen::Detail);
        let popped = nav.pop_animated(TransitionStyle::Slide(SlideDirection::Left));
        assert!(popped);
        assert_eq!(nav.current(), Some(Screen::Home));
        assert!(nav.is_transitioning());
    }

    #[test]
    fn nav_animated_update_returns_tuple() {
        let mut nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
        nav.push_animated(Screen::Detail, TransitionStyle::Slide(SlideDirection::Right));
        let (ex, ey, ox, oy, progress, complete) = nav.update(1.0 / 60.0);
        // Transition is ongoing — should not be complete yet
        assert!(!complete);
        // Enter x starts at -640, moves toward 0
        let _ = (ex, ey, ox, oy, progress);
    }

    #[test]
    fn nav_animated_depth() {
        let mut nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
        assert_eq!(nav.depth(), 1);
        nav.push(Screen::Detail);
        assert_eq!(nav.depth(), 2);
        nav.push(Screen::Settings);
        assert_eq!(nav.depth(), 3);
        nav.pop();
        assert_eq!(nav.depth(), 2);
    }

    #[test]
    fn nav_animated_can_go_back() {
        let mut nav = NavigatorAnimated::new(Screen::Home, 640.0, 480.0);
        assert!(!nav.can_go_back());
        nav.push(Screen::Detail);
        assert!(nav.can_go_back());
        nav.pop();
        assert!(!nav.can_go_back());
    }
}
