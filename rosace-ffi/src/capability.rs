//! Platform-capability request/result plumbing (D106 Phase 24 Step 5).
//!
//! Proves the native-host model actually reaches things Info.plist-only
//! (winit-owned) apps structurally couldn't: a real permission prompt, with
//! the result flowing back into Rust app code and driving a UI re-render.
//! Deliberately ONE capability (camera), not a general `Permission`/
//! `Haptics`/`Biometrics`/`use_sensor()` surface — see `.steering/
//! PHASE_24.md`'s Step 5 scope note: "one proof is enough to validate the
//! model; a fuller capabilities surface is later work once real apps show
//! which capabilities are actually needed first." A second capability
//! would follow this exact same three-piece shape (request queue + result
//! atom + host-side native call), not a new architecture.
//!
//! Flow: app code (e.g. a button's `on_press`) calls [`request_camera`],
//! which queues the request. The native host polls
//! [`take_camera_request`] once per frame tick (same polling shape
//! `Engine::frame` already uses for input events — see `engine.rs`), and if
//! `true`, triggers its own native permission API (`AVCaptureDevice.
//! requestAccess` on iOS). When that resolves, the host calls
//! [`report_camera_result`], which writes [`CAMERA_PERMISSION`] — app code
//! reads it via `CAMERA_PERMISSION.get()`, and `GlobalAtom::set` notifies
//! subscribers, so a widget reading it re-renders automatically.
//!
//! These are plain functions, not `#[no_mangle] extern "C"` themselves —
//! same reasoning as `Engine`: the FFI symbols crossing the boundary are
//! per-app generated (`rsc new`'s `ffi_rs`) so an app that never asks for
//! camera access doesn't get an unused `NSCameraUsageDescription` baked
//! into its Info.plist as a side effect of the framework existing.

use std::sync::Mutex;

use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;

/// Whether the camera permission has been requested, and if resolved, the
/// native host's answer. `None` = never requested (or still pending: the
/// native permission dialog is asynchronous, so there's a real window
/// between [`request_camera`] and [`report_camera_result`] where a widget
/// reading this atom should show "asking…", not treat `None` as "denied").
///
/// `0xFFFC` follows the existing reserved-high-id convention for
/// framework-global atoms (see `rosace_core::platform::PLATFORM_ATOM_ID`
/// `0xFFFD`, `SAFE_AREA_ATOM_ID` `0xFFFE`, `rosace_theme`'s `THEME_ATOM_ID`
/// `0xFFFF`) — well clear of the auto-incrementing per-component atom ids
/// (`rosace_state::atom_id_gen`, starts at 1).
const CAMERA_PERMISSION_ATOM_ID: AtomId = AtomId(0xFFFC);

pub static CAMERA_PERMISSION: GlobalAtom<Option<bool>> =
    GlobalAtom::new(CAMERA_PERMISSION_ATOM_ID, || None);

/// Whether a request is queued but not yet delivered to the host via
/// [`take_camera_request`]. A `bool`, not a counter — duplicate requests
/// (e.g. impatient double-taps before the first prompt resolves) collapse
/// into one, matching how a real permission prompt can't be shown twice at
/// once anyway.
static CAMERA_REQUEST_PENDING: Mutex<bool> = Mutex::new(false);

/// Called by app code (e.g. a button's `on_press`) to ask the native host
/// to show the camera permission prompt. Does nothing if a request is
/// already queued or the permission was already resolved either way —
/// callers don't need to guard against re-requesting themselves.
pub fn request_camera() {
    if CAMERA_PERMISSION.get().is_some() {
        return; // already resolved, nothing to re-request
    }
    *CAMERA_REQUEST_PENDING.lock().unwrap_or_else(|e| e.into_inner()) = true;
}

/// Polled by the native host once per frame tick (alongside
/// `tzr_engine_frame`). Returns `true` at most once per [`request_camera`]
/// call — the host is expected to act on it immediately (trigger the real
/// permission API), not to hold a `true` result and ask again later.
pub fn take_camera_request() -> bool {
    let mut pending = CAMERA_REQUEST_PENDING.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *pending)
}

/// Called by the native host once its permission API resolves (e.g.
/// `AVCaptureDevice.requestAccess`'s completion handler). Writes
/// [`CAMERA_PERMISSION`], which notifies subscribers — any widget reading
/// it re-renders with the real answer.
pub fn report_camera_result(granted: bool) {
    CAMERA_PERMISSION.set(Some(granted));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    // `CAMERA_PERMISSION`/`CAMERA_REQUEST_PENDING` are process-global
    // statics — tests touching them must be serialized against each other,
    // same reasoning as `rosace-cli`'s `CWD_LOCK` (`test_support.rs`).
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn request_then_take_returns_true_once() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        CAMERA_PERMISSION.set(None);
        *CAMERA_REQUEST_PENDING.lock().unwrap() = false;

        request_camera();
        assert!(take_camera_request(), "first poll should see the request");
        assert!(!take_camera_request(), "second poll should not see it again");
    }

    #[test]
    fn report_result_updates_the_atom() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        CAMERA_PERMISSION.set(None);

        report_camera_result(true);
        assert_eq!(CAMERA_PERMISSION.get(), Some(true));

        report_camera_result(false);
        assert_eq!(CAMERA_PERMISSION.get(), Some(false));
    }

    #[test]
    fn request_is_a_noop_once_already_resolved() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *CAMERA_REQUEST_PENDING.lock().unwrap() = false;
        CAMERA_PERMISSION.set(Some(true));

        request_camera();
        assert!(!take_camera_request(), "already-resolved permission shouldn't re-queue a request");

        CAMERA_PERMISSION.set(None); // reset for other tests
    }
}
