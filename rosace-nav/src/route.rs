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

/// A route that also has a PATH — the URL form of a screen.
///
/// Implemented by the `#[routes]` macro rather than by hand; writing the two
/// directions separately is how they drift apart.
///
/// ```rust,ignore
/// #[routes]
/// #[derive(Debug, Clone, PartialEq)]
/// enum Screen {
///     #[route("/")]             Home,
///     #[route("/user/:id")]     User { id: u64 },
///     #[route("/widget/:kind")] Widget(WidgetKind),
/// }
///
/// Screen::User { id: 7 }.to_path()      // "/user/7"
/// Screen::from_path("/user/7")          // Some(User { id: 7 })
/// Screen::from_path("/user/seven")      // None — `id` is a u64
/// ```
///
/// This is what makes a deep link possible: an incoming path from an OS URL
/// scheme, an Android intent or a browser address bar becomes a typed route,
/// or is rejected. Nothing downstream ever handles a stringly-typed screen.
pub trait RoutePath: Route + Sized {
    /// This route's path, e.g. `/user/7`.
    fn to_path(&self) -> String;

    /// Parse a path into a route, or `None` if nothing matches.
    ///
    /// `None` covers both "no pattern matches this shape" and "a parameter
    /// would not parse as its declared type" — from the caller's side those
    /// are the same thing: this is not a route in this app.
    fn from_path(path: &str) -> Option<Self>;
}

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
