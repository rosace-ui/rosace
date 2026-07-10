use std::sync::Arc;

use crate::guard::NavigationGuard;
use crate::route::{NavigationDecision, Route};
use crate::stack::NavigationStack;

/// A handle to a navigation stack.
///
/// Cheap to clone — all clones share the same underlying stack (D027). The
/// navigator is the primary API surface for application code: push, pop,
/// replace, and reset.
///
/// Navigation guards (D028) are consulted before every push; guards are
/// optional and can be attached via [`with_guard`].
///
/// [`with_guard`]: Navigator::with_guard
#[derive(Clone)]
pub struct Navigator<R: Route> {
    stack: Arc<NavigationStack<R>>,
    guard: Option<Arc<dyn NavigationGuard<R>>>,
}

impl<R: Route> Navigator<R> {
    /// Creates a new navigator with `root` as the bottom of the stack.
    pub fn new(root: R) -> Self {
        Self {
            stack: Arc::new(NavigationStack::new(root)),
            guard: None,
        }
    }

    /// Attaches a navigation guard. Returns `self` for builder chaining.
    ///
    /// Only one guard can be active at a time; calling this again replaces
    /// the previous one.
    pub fn with_guard(mut self, guard: impl NavigationGuard<R>) -> Self {
        self.guard = Some(Arc::new(guard));
        self
    }

    fn check_guard(&self, to: &R) -> NavigationDecision {
        match &self.guard {
            None => NavigationDecision::Allow,
            Some(g) => g.before_navigate(self.stack.current().as_ref(), to),
        }
    }

    /// Pushes `route` onto the stack.
    ///
    /// Returns `true` if navigation succeeded, `false` if blocked by a guard.
    ///
    /// When a guard returns [`NavigationDecision::RedirectTo`] the route is
    /// still pushed as-is (Phase 3 will implement URL redirect resolution).
    pub fn push(&self, route: R) -> bool {
        match self.check_guard(&route) {
            NavigationDecision::Allow => {
                self.stack.push(route);
                true
            }
            NavigationDecision::Block => false,
            NavigationDecision::RedirectTo(_) => {
                // Phase 3: resolve the redirect path to a typed route.
                // For now treat as Allow so the original push proceeds.
                self.stack.push(route);
                true
            }
        }
    }

    /// Goes back one screen.
    ///
    /// Returns `false` if already at the root (cannot pop root).
    pub fn pop(&self) -> bool {
        self.stack.pop()
    }

    /// Replaces the current screen without adding a history entry.
    pub fn replace(&self, route: R) {
        self.stack.replace(route);
    }

    /// Clears the stack and sets `route` as the new root.
    pub fn reset_to(&self, route: R) {
        self.stack.reset_to(route);
    }

    /// Returns the current route, or `None` if the stack is empty.
    pub fn current(&self) -> Option<R> {
        self.stack.current()
    }

    /// Returns `true` when back navigation is possible.
    pub fn can_go_back(&self) -> bool {
        self.stack.can_go_back()
    }

    /// Stack depth (number of routes including root).
    pub fn depth(&self) -> usize {
        self.stack.depth()
    }

    /// Full stack snapshot, bottom-to-top.
    pub fn stack(&self) -> Vec<R> {
        self.stack.stack()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::guard::BlockWhenGuard;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Screen {
        Home,
        Detail,
        Settings,
        Profile,
    }
    impl Route for Screen {}

    #[test]
    fn navigator_push_and_pop() {
        let nav = Navigator::new(Screen::Home);
        nav.push(Screen::Detail);
        assert_eq!(nav.current(), Some(Screen::Detail));
        assert!(nav.pop());
        assert_eq!(nav.current(), Some(Screen::Home));
    }

    #[test]
    fn navigator_guard_blocks_navigation() {
        let nav = Navigator::new(Screen::Home).with_guard(BlockWhenGuard::new(|| true));
        let pushed = nav.push(Screen::Detail);
        assert!(!pushed);
        assert_eq!(nav.current(), Some(Screen::Home));
    }

    #[test]
    fn navigator_guard_allows_navigation() {
        let nav = Navigator::new(Screen::Home).with_guard(BlockWhenGuard::new(|| false));
        let pushed = nav.push(Screen::Detail);
        assert!(pushed);
        assert_eq!(nav.current(), Some(Screen::Detail));
    }

    #[test]
    fn navigator_replace_does_not_add_depth() {
        let nav = Navigator::new(Screen::Home);
        nav.push(Screen::Detail);
        let depth_before = nav.depth();
        nav.replace(Screen::Settings);
        assert_eq!(nav.depth(), depth_before);
        assert_eq!(nav.current(), Some(Screen::Settings));
    }

    #[test]
    fn navigator_reset_to_clears_stack() {
        let nav = Navigator::new(Screen::Home);
        nav.push(Screen::Detail);
        nav.push(Screen::Settings);
        nav.reset_to(Screen::Profile);
        assert_eq!(nav.depth(), 1);
        assert_eq!(nav.current(), Some(Screen::Profile));
    }

    #[test]
    fn navigator_clone_shares_stack() {
        let nav = Navigator::new(Screen::Home);
        let nav2 = nav.clone();
        nav.push(Screen::Detail);
        // Both handles see the same state.
        assert_eq!(nav2.current(), Some(Screen::Detail));
    }
}
