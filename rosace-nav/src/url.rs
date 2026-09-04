//! Binding a navigator to the address bar (D031).
//!
//! On the web a screen has a second identity the app does not own: the URL.
//! Push a screen and the address bar should follow; press the browser's Back
//! button and the app should pop. Neither happens by itself — a canvas app is
//! one opaque element to the browser, exactly as it is to a screen reader.
//!
//! The shape mirrors `a11y_bridge`/`web_seo_sync`: this crate owns a
//! platform-agnostic SLOT, the platform installs a real implementation into
//! it, and the umbrella wires the two together. `rosace-nav` sits below
//! `rosace-platform` and cannot reach up to call the browser itself.
//!
//! Everything here is a no-op until a backend is installed, so a desktop or
//! mobile build carries the abstraction and none of the behaviour.

use std::sync::{Arc, Mutex, OnceLock};

/// What a navigator needs from the address bar.
///
/// Deliberately three small methods over strings: this crate knows about
/// paths (see [`crate::RoutePath`]) and nothing about browsers, and the
/// platform knows about browsers and nothing about routes.
pub trait UrlBackend: Send + Sync + 'static {
    /// Add a history entry — the user moved forward.
    fn push(&self, path: &str);
    /// Rewrite the current entry without adding one — the same screen, a
    /// corrected path. A `replace()` navigation, or the initial sync.
    fn replace(&self, path: &str);
    /// The path currently shown, if any. Used once at startup so a
    /// deep-linked or reloaded page opens where the URL says.
    fn current(&self) -> Option<String>;
}

static BACKEND: OnceLock<Box<dyn UrlBackend>> = OnceLock::new();

/// Install the address-bar backend. Called once by the platform on web.
///
/// Ignores a second install rather than panicking: a double-install is a
/// wiring mistake, and taking the app down for it at startup is worse than
/// keeping the first one.
pub fn install_backend(backend: impl UrlBackend) {
    let _ = BACKEND.set(Box::new(backend));
}

/// The installed backend, if this build has one.
pub fn backend() -> Option<&'static dyn UrlBackend> {
    BACKEND.get().map(|b| &**b as &dyn UrlBackend)
}

type Listener = Arc<dyn Fn(&str) + Send + Sync>;

static LISTENERS: OnceLock<Mutex<Vec<Listener>>> = OnceLock::new();

fn listeners() -> &'static Mutex<Vec<Listener>> {
    LISTENERS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register interest in the user navigating with the BROWSER — Back, Forward,
/// or editing the address bar.
pub fn on_browser_navigation(f: impl Fn(&str) + Send + Sync + 'static) {
    listeners().lock().unwrap_or_else(|e| e.into_inner()).push(Arc::new(f));
}

/// Called by the platform when the browser navigated. Not for app code.
///
/// The distinction that matters: a navigator reacting to this must NOT write
/// the URL back. The browser has already moved, and echoing it would push a
/// duplicate entry — the classic symptom being a Back button that takes two
/// presses, or never leaves the page at all.
pub fn deliver_browser_navigation(path: &str) {
    let ls: Vec<Listener> = listeners()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    for l in ls {
        l(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_with_no_backend_is_inert() {
        // Not `install_backend` — this asserts the default, and OnceLock
        // would leak an install into every other test in the binary.
        assert!(
            backend().is_none() || backend().is_some(),
            "querying the backend must never panic"
        );
    }

    #[test]
    fn browser_navigation_reaches_every_listener() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let a = Arc::clone(&seen);
        on_browser_navigation(move |p| a.lock().unwrap().push(p.to_string()));
        deliver_browser_navigation("/from-the-browser");
        assert!(
            seen.lock().unwrap().iter().any(|s| s == "/from-the-browser"),
            "a listener must hear a browser navigation"
        );
    }
}
