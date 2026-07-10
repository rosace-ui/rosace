use crate::route::{NavigationDecision, Route};

/// A navigation guard. Called before every navigation action (D028).
///
/// Return [`NavigationDecision::Allow`] to proceed, [`NavigationDecision::Block`]
/// to stay on the current screen, or [`NavigationDecision::RedirectTo`] to go
/// to a different route path.
pub trait NavigationGuard<R: Route>: Send + Sync + 'static {
    fn before_navigate(&self, from: Option<&R>, to: &R) -> NavigationDecision;
}

/// A guard that always allows navigation (default, no-op).
pub struct AllowAllGuard;

impl<R: Route> NavigationGuard<R> for AllowAllGuard {
    fn before_navigate(&self, _from: Option<&R>, _to: &R) -> NavigationDecision {
        NavigationDecision::Allow
    }
}

/// A guard that blocks navigation whenever `condition` returns `true`.
///
/// # Example
/// ```rust,ignore
/// let nav = Navigator::new(Screen::Home)
///     .with_guard(BlockWhenGuard::new(|| has_unsaved_changes()));
/// ```
pub struct BlockWhenGuard<F: Fn() -> bool + Send + Sync + 'static> {
    condition: F,
}

impl<F: Fn() -> bool + Send + Sync + 'static> BlockWhenGuard<F> {
    pub fn new(condition: F) -> Self {
        Self { condition }
    }
}

impl<R: Route, F: Fn() -> bool + Send + Sync + 'static> NavigationGuard<R> for BlockWhenGuard<F> {
    fn before_navigate(&self, _from: Option<&R>, _to: &R) -> NavigationDecision {
        if (self.condition)() {
            NavigationDecision::Block
        } else {
            NavigationDecision::Allow
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Screen {
        Home,
        Detail,
    }
    impl Route for Screen {}

    #[test]
    fn allow_all_guard_always_allows() {
        let guard = AllowAllGuard;
        let decision = guard.before_navigate(Some(&Screen::Home), &Screen::Detail);
        assert_eq!(decision, NavigationDecision::Allow);
    }

    #[test]
    fn block_when_guard_blocks_when_condition_true() {
        let guard = BlockWhenGuard::new(|| true);
        let decision = guard.before_navigate(Some(&Screen::Home), &Screen::Detail);
        assert_eq!(decision, NavigationDecision::Block);
    }

    #[test]
    fn block_when_guard_allows_when_condition_false() {
        let guard = BlockWhenGuard::new(|| false);
        let decision = guard.before_navigate(Some(&Screen::Home), &Screen::Detail);
        assert_eq!(decision, NavigationDecision::Allow);
    }
}
