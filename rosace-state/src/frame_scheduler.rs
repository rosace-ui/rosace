use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static FRAME_REQUESTED: AtomicBool = AtomicBool::new(false);
static WAKEUP_FN: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Register the platform wakeup function.
///
/// Called once at app startup by `rosace-platform` with a closure that
/// sends a `FrameRequest` user event to the winit event loop, waking it
/// from `ControlFlow::Wait`.
pub fn register_wakeup(f: impl Fn() + Send + Sync + 'static) {
    let _ = WAKEUP_FN.set(Box::new(f));
}

/// Signal that a frame should be rendered on the next VSync.
///
/// Sets an atomic flag and calls the registered wakeup function if one
/// is installed. Safe to call from any thread. Idempotent — multiple
/// calls before the platform polls collapse into one redraw.
pub fn request_frame() {
    FRAME_REQUESTED.store(true, Ordering::Release);
    if let Some(f) = WAKEUP_FN.get() {
        f();
    }
}

/// Atomically read-and-clear the frame-requested flag.
///
/// Returns `true` if a frame was requested since the last call.
/// Called by the platform's `about_to_wait` handler.
pub fn take_frame_requested() -> bool {
    FRAME_REQUESTED.swap(false, Ordering::AcqRel)
}

/// Run `f` once, `ms` milliseconds from now — the web-safe timer primitive.
///
/// Native spawns a short-lived timer thread. Web uses `setTimeout`, because
/// `wasm32-unknown-unknown` has NO threads and `std::thread::spawn` aborts the
/// whole module there (this is what used to crash the app the instant a text
/// field was focused, and would crash on any toast/snackbar). Callbacks must
/// only touch `Atom`s / atomics / mutexes and call [`request_frame`] — never
/// engine internals — so the single-threaded web path stays sound.
#[cfg(not(target_arch = "wasm32"))]
pub fn fire_after_ms(ms: u64, f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        f();
    });
}

/// See the native docs above — web variant, `setTimeout`-backed, no `Send`.
#[cfg(target_arch = "wasm32")]
pub fn fire_after_ms(ms: u64, f: impl FnOnce() + 'static) {
    use wasm_bindgen::JsCast;
    let cb = wasm_bindgen::closure::Closure::once_into_js(f);
    if let Some(w) = web_sys::window() {
        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
            cb.as_ref().unchecked_ref(),
            ms as i32,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_not_requested_initially() {
        // Clear any state left by other tests.
        take_frame_requested();
        assert!(!take_frame_requested());
    }

    #[test]
    fn request_frame_sets_flag() {
        take_frame_requested();
        request_frame();
        assert!(take_frame_requested());
    }

    #[test]
    fn take_clears_flag() {
        request_frame();
        assert!(take_frame_requested());
        assert!(!take_frame_requested());
    }

    #[test]
    fn multiple_requests_collapse_to_one() {
        take_frame_requested();
        request_frame();
        request_frame();
        request_frame();
        assert!(take_frame_requested());
        assert!(!take_frame_requested());
    }
}
