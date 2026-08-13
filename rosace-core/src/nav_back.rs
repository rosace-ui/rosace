//! The system back intent, and who gets to answer it.
//!
//! Android's back button and back gesture, and iOS's left-edge swipe, are
//! *requests to go back* — not key presses. The host asks the app "can you
//! handle this?", and what happens if the answer is no is the platform's
//! business: Android finishes the activity, iOS does nothing.
//!
//! ## Why a registry
//!
//! The engine owns overlays, so it can close a dialog itself. It cannot pop a
//! navigation stack: [`ScreenNav<R>`] is generic over the app's route type
//! and owned by app code, and the engine has no idea R exists. So `ScreenNav`
//! registers a handler here when it is created, and the engine calls it.
//!
//! ## Resolution order
//!
//! Matching Android's own convention, and what every user already expects:
//!
//! 1. **An open, dismissible overlay closes.** A dialog on top of a screen
//!    takes the back press — closing the dialog AND the screen underneath in
//!    one gesture is the classic bug this ordering prevents.
//! 2. **Otherwise the nav stack pops**, if it has anywhere to go.
//! 3. **Otherwise the app declines**, and the platform does its default —
//!    which on Android means leaving the app. Declining is the correct answer
//!    at the root: swallowing it silently traps the user in an app they
//!    cannot exit with the control the OS gave them.

use std::cell::RefCell;
use std::sync::Arc;

/// Returns `true` if it consumed the back intent.
pub type BackHandler = Arc<dyn Fn() -> bool + Send + Sync>;

thread_local! {
    /// The active handler, if any.
    ///
    /// Thread-local for the same reason the dirty set is: this belongs to
    /// the engine on the UI thread, and a process-global would let one
    /// engine's navigator answer another engine's back press. It is also
    /// what keeps parallel tests from stealing each other's handler.
    ///
    /// A single slot, last registration wins. Nested navigators are a real
    /// pattern (a tab bar with a stack per tab), and resolving *which* one
    /// should answer needs a focus/visibility notion the framework does not
    /// have yet. Last-wins is predictable and documented rather than
    /// half-guessed; see the module docs for the ordering it sits inside.
    static HANDLER: RefCell<Option<BackHandler>> = const { RefCell::new(None) };
}

/// Register the handler the engine consults on a back intent.
///
/// Called by `ScreenNav` on construction. Registering again replaces the
/// previous handler.
pub fn set_back_handler(f: BackHandler) {
    HANDLER.with(|h| *h.borrow_mut() = Some(f));
}

/// Drop the current handler (an app tearing down its navigator).
pub fn clear_back_handler() {
    HANDLER.with(|h| *h.borrow_mut() = None);
}

/// Offer the back intent to the registered handler.
///
/// `false` means nothing consumed it and the platform should do its default.
pub fn dispatch_back() -> bool {
    // Cloned out of the RefCell BEFORE calling: the handler pops a nav stack,
    // which sets an atom, which can rebuild a component, which may register a
    // new handler — re-entering this thread_local while the borrow is live
    // would panic.
    let handler = HANDLER.with(|h| h.borrow().clone());
    handler.map(|f| f()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn with_no_handler_the_app_declines() {
        clear_back_handler();
        assert!(!dispatch_back(), "no navigator means the platform should act");
    }

    #[test]
    fn the_handler_decides_whether_the_intent_was_consumed() {
        clear_back_handler();
        set_back_handler(Arc::new(|| true));
        assert!(dispatch_back());

        // A navigator at its root returns false, so back still exits the app.
        set_back_handler(Arc::new(|| false));
        assert!(!dispatch_back(), "a root navigator must not swallow the intent");
        clear_back_handler();
    }

    #[test]
    fn registering_again_replaces_rather_than_stacks() {
        clear_back_handler();
        let calls = Arc::new(AtomicUsize::new(0));
        let first = calls.clone();
        set_back_handler(Arc::new(move || { first.fetch_add(1, Ordering::SeqCst); true }));
        set_back_handler(Arc::new(|| true));
        dispatch_back();
        assert_eq!(calls.load(Ordering::SeqCst), 0, "the replaced handler must not run");
        clear_back_handler();
    }

    /// A handler that navigates can cause a rebuild that registers a NEW
    /// handler. Dispatch must not be holding the borrow when that happens.
    #[test]
    fn a_handler_may_register_a_new_handler_while_running() {
        clear_back_handler();
        set_back_handler(Arc::new(|| {
            set_back_handler(Arc::new(|| true)); // re-entrant registration
            true
        }));
        assert!(dispatch_back(), "must not panic on a re-entrant borrow");
        clear_back_handler();
    }
}
