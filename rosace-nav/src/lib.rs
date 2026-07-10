//! Navigation router for ROSACE (D026–D031).
//!
//! Routes are typed Rust enums (D026) — never stringly-typed. Each navigator
//! owns an independent history stack (D027). Navigation guards are consulted
//! before every transition (D028). Keep-alive memory keeps navigated-away
//! screens alive until the stack is reset (D030). URL sync for web targets is
//! stubbed here and completed in Phase 3 (D031).
//!
//! # Quick start
//!
//! ```rust,ignore
//! use rosace_nav::{Navigator, Route};
//!
//! #[derive(Debug, Clone, PartialEq)]
//! enum Screen { Home, Detail { id: u64 }, Settings }
//! impl Route for Screen {}
//!
//! let nav = Navigator::new(Screen::Home);
//! nav.push(Screen::Detail { id: 42 });
//! assert_eq!(nav.current(), Some(Screen::Detail { id: 42 }));
//! nav.pop();
//! assert_eq!(nav.current(), Some(Screen::Home));
//! ```

pub mod guard;
pub mod history;
pub mod navigator;
pub mod route;
pub mod screen_nav;
pub mod stack;
pub mod transition;

pub use guard::{AllowAllGuard, BlockWhenGuard, NavigationGuard};
pub use history::{HistoryEntry, KeepAliveRegistry};
pub use navigator::Navigator;
pub use route::{NavigationDecision, Route};
pub use screen_nav::ScreenNav;
pub use stack::NavigationStack;
pub use transition::{ScreenTransition, SlideDirection, TransitionStyle};

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::guard::BlockWhenGuard;
    use crate::{NavigationDecision, Navigator, Route};

    #[derive(Debug, Clone, PartialEq)]
    enum AppScreen {
        Splash,
        Login,
        Home,
        Profile { user_id: u64 },
        Settings,
        NotFound,
    }
    impl Route for AppScreen {}

    /// Verifies a realistic multi-screen navigation flow end-to-end.
    #[test]
    fn full_navigation_flow_multi_screen() {
        let nav = Navigator::new(AppScreen::Splash);

        // Boot sequence: replace splash with login (no history depth added).
        nav.replace(AppScreen::Login);
        assert_eq!(nav.current(), Some(AppScreen::Login));
        assert_eq!(nav.depth(), 1);
        assert!(!nav.can_go_back());

        // After login: reset to home so login is not in back-stack.
        nav.reset_to(AppScreen::Home);
        assert_eq!(nav.current(), Some(AppScreen::Home));
        assert_eq!(nav.depth(), 1);

        // Navigate deeper.
        assert!(nav.push(AppScreen::Profile { user_id: 7 }));
        assert_eq!(nav.depth(), 2);
        assert!(nav.can_go_back());

        assert!(nav.push(AppScreen::Settings));
        assert_eq!(nav.depth(), 3);
        assert_eq!(nav.current(), Some(AppScreen::Settings));

        // Back from Settings → Profile.
        assert!(nav.pop());
        assert_eq!(nav.current(), Some(AppScreen::Profile { user_id: 7 }));

        // Back from Profile → Home.
        assert!(nav.pop());
        assert_eq!(nav.current(), Some(AppScreen::Home));
        assert!(!nav.can_go_back());

        // Cannot pop past root.
        assert!(!nav.pop());
        assert_eq!(nav.current(), Some(AppScreen::Home));
    }

    /// Guard blocks navigation and allows it based on runtime condition.
    #[test]
    fn navigation_guard_conditional_block() {
        use std::sync::{Arc, Mutex};

        let blocked = Arc::new(Mutex::new(true));
        let blocked_clone = Arc::clone(&blocked);

        let nav = Navigator::new(AppScreen::Home)
            .with_guard(BlockWhenGuard::new(move || *blocked_clone.lock().unwrap()));

        // Blocked initially.
        assert!(!nav.push(AppScreen::Settings));
        assert_eq!(nav.current(), Some(AppScreen::Home));

        // Unblock and retry.
        *blocked.lock().unwrap() = false;
        assert!(nav.push(AppScreen::Settings));
        assert_eq!(nav.current(), Some(AppScreen::Settings));
    }

    /// Cloned navigators share the same underlying stack.
    #[test]
    fn cloned_navigators_share_state() {
        let nav1 = Navigator::new(AppScreen::Home);
        let nav2 = nav1.clone();

        nav1.push(AppScreen::Settings);
        assert_eq!(nav2.current(), Some(AppScreen::Settings));
        assert_eq!(nav2.depth(), nav1.depth());
    }

    /// NavigationDecision variants are distinct and comparable.
    #[test]
    fn navigation_decision_variants_are_distinct() {
        assert_ne!(NavigationDecision::Allow, NavigationDecision::Block);
        assert_ne!(
            NavigationDecision::Allow,
            NavigationDecision::RedirectTo("/other".to_string())
        );
        assert_eq!(
            NavigationDecision::RedirectTo("/x".to_string()),
            NavigationDecision::RedirectTo("/x".to_string())
        );
    }

    /// Stack snapshot reflects full push history.
    #[test]
    fn stack_snapshot_reflects_push_history() {
        let nav = Navigator::new(AppScreen::Home);
        nav.push(AppScreen::Login);
        nav.push(AppScreen::Settings);
        let snap = nav.stack();
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0], AppScreen::Home);
        assert_eq!(snap[1], AppScreen::Login);
        assert_eq!(snap[2], AppScreen::Settings);
    }
}
