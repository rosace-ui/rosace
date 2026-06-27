/// Any type that can serve as a route. Usually a Rust enum.
///
/// # Example
/// ```rust,ignore
/// #[derive(Debug, Clone, PartialEq)]
/// enum Screen {
///     Home,
///     Detail { id: u64 },
///     Settings,
/// }
/// impl Route for Screen {}
/// ```
pub trait Route: std::fmt::Debug + Clone + PartialEq + Send + Sync + 'static {}

/// A navigation decision returned by a guard.
#[derive(Debug, Clone, PartialEq)]
pub enum NavigationDecision {
    /// Allow the navigation to proceed.
    Allow,
    /// Block the navigation — stay on current screen.
    Block,
    /// Redirect to a different route (Phase 3+; path string for URL sync).
    RedirectTo(String),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_decision_allow_is_allow() {
        let d = NavigationDecision::Allow;
        assert_eq!(d, NavigationDecision::Allow);
    }

    #[test]
    fn navigation_decision_block_is_block() {
        let d = NavigationDecision::Block;
        assert_eq!(d, NavigationDecision::Block);
    }

    #[test]
    fn navigation_decision_redirect_to_carries_path() {
        let d = NavigationDecision::RedirectTo("/home".to_string());
        assert_eq!(d, NavigationDecision::RedirectTo("/home".to_string()));
    }
}
