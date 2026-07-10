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
