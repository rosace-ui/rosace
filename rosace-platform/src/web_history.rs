//! The browser half of D031 — `history.pushState` and `popstate`.
//!
//! Raw browser primitives ONLY — this crate knows nothing about routes or
//! navigators, and deliberately does not depend on `rosace-nav`. Nav sits
//! ABOVE platform; having platform reach up to it would invert the layering
//! for one feature. The umbrella depends on both and does the adapting, the
//! same division `a11y_bridge` uses: platform owns the mechanism, the layer
//! that owns the engine connects it.
//!
//! Web only. On every other target [`install`] is a no-op and the navigator's
//! `sync_url` finds no backend, so nothing in this file costs a desktop or
//! mobile build anything.

#[cfg(target_arch = "wasm32")]
mod imp {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    /// Add a history entry.
    pub fn push_state(path: &str) {
            if let Some(h) = history() {
                // `pushState` cannot throw for a same-origin path, but it CAN
                // for a cross-origin one — a route whose path someone made
                // absolute. Ignore rather than trap: a wrong URL is a cosmetic
                // failure, and taking the app down over the address bar is not.
                let _ = h.push_state_with_url(&JsValue::NULL, "", Some(path));
            }
    }

    /// Rewrite the current entry without adding one.
    pub fn replace_state(path: &str) {
        if let Some(h) = history() {
            let _ = h.replace_state_with_url(&JsValue::NULL, "", Some(path));
        }
    }

    /// The path (and query) currently shown.
    pub fn current_path() -> Option<String> {
            let loc = web_sys::window()?.location();
            let path = loc.pathname().ok()?;
            // The query is deliberately kept: `RoutePath::from_path` strips it
            // before matching, and a caller that wants the parameters needs
            // them to still be here.
        match loc.search().ok().filter(|s| !s.is_empty()) {
            Some(q) => Some(format!("{path}{q}")),
            None => Some(path),
        }
    }

    fn history() -> Option<web_sys::History> {
        web_sys::window()?.history().ok()
    }

    /// Call `f` with the new path whenever the BROWSER navigates.
    pub fn on_popstate(f: impl Fn(String) + 'static) {
        // The browser -> app direction. `popstate` fires for Back, Forward,
        // and a same-document address-bar edit; it does NOT fire for our own
        // `pushState`, which is exactly the asymmetry that keeps this from
        // looping.
        let Some(win) = web_sys::window() else { return };
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            if let Some(w) = web_sys::window() {
                let loc = w.location();
                if let Ok(path) = loc.pathname() {
                    let full = match loc.search().ok().filter(|s| !s.is_empty()) {
                        Some(q) => format!("{path}{q}"),
                        None => path,
                    };
                    f(full);
                }
            }
        });
        let _ = win.add_event_listener_with_callback("popstate", cb.as_ref().unchecked_ref());
        // Deliberately leaked: this listener lives as long as the page does,
        // and dropping the closure would detach it. The alternative is
        // storing it in a thread-local that is never read, which is the same
        // leak with more ceremony.
        cb.forget();
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    /// No address bar off the web — these are inert so callers need no `cfg`.
    pub fn push_state(_path: &str) {}
    pub fn replace_state(_path: &str) {}
    pub fn current_path() -> Option<String> { None }
    pub fn on_popstate(_f: impl Fn(String) + 'static) {}
}

pub use imp::{current_path, on_popstate, push_state, replace_state};
