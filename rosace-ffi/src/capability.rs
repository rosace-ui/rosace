//! Platform-capability request/result plumbing (D106 Phase 24 Step 5;
//! request discovery unified onto Platform Channel, D127).
//!
//! Proves the native-host model actually reaches things Info.plist-only
//! (winit-owned) apps structurally couldn't: a real permission prompt, with
//! the result flowing back into Rust app code and driving a UI re-render.
//!
//! Flow: app code (e.g. a button's `on_press`) calls [`request_camera`],
//! which queues a Platform Channel call on the well-known `"rosace/camera"`
//! channel (see `platform_channel::outgoing`). The native host's ONE generic
//! per-frame poll (`take_outgoing_calls`, alongside `rsc_engine_frame`) sees
//! it, recognizes the channel, and triggers the real permission API
//! (`AVCaptureDevice.requestAccess` on iOS). When that resolves, the host
//! calls [`report_camera_result`], which writes [`CAMERA_PERMISSION`] — app
//! code reads it via `CAMERA_PERMISSION.get()`, and `GlobalAtom::set`
//! notifies subscribers, so a widget reading it re-renders automatically.
//!
//! Camera/push deliberately do NOT correlate by `call_id` the way an
//! arbitrary Platform Channel caller would (`outgoing::invoke_method`'s
//! returned `Atom` is queued and then dropped here) — each is a singleton
//! capability (never more than one camera permission in flight at once), so
//! a plain boolean result-setter is simpler and sufficient. An app-defined
//! channel that might have several concurrent calls in flight should use
//! `invoke_method` directly and keep its returned `Atom` instead.
//!
//! These are plain functions, not `#[no_mangle] extern "C"` themselves —
//! same reasoning as `Engine`: the FFI symbols crossing the boundary are
//! per-app generated (`rsc new`'s `ffi_rs`) so an app that never asks for
//! camera access doesn't get an unused `NSCameraUsageDescription` baked
//! into its Info.plist as a side effect of the framework existing (the
//! generated template's default tick loop does NOT special-case
//! `"rosace/camera"` for exactly this reason — only `"rosace/push"` is
//! handled unconditionally, since every app already gets push-permission
//! polling; camera wiring is opt-in native code an app adds itself,
//! recognizing the channel via the same generic `take_outgoing_calls`
//! every custom Platform Channel channel uses).

use std::sync::Mutex;

use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;
use serde_json::Value;

use crate::platform_channel::invoke_method;

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

/// Read [`CAMERA_PERMISSION`] from a component's `build()`, subscribing the
/// component so it re-renders when the permission resolves — the explicit
/// `subscribe` is required, `GlobalAtom`s aren't auto-subscribed by
/// `ctx.state`'s hook machinery (same convention as
/// `rosace_core::use_app_lifecycle`). Prefer this over
/// `CAMERA_PERMISSION.get()` directly inside a widget; use the bare `.get()`
/// only outside the component tree (e.g. engine/host code), where there's
/// no component to subscribe.
pub fn use_camera_permission(ctx: &rosace_core::Context) -> Option<bool> {
    CAMERA_PERMISSION.get_or_init().subscribe(ctx.component_id());
    CAMERA_PERMISSION.get()
}

/// Whether a request has been queued but not yet resolved. A `bool`, not a
/// counter — duplicate requests (e.g. impatient double-taps before the
/// first prompt resolves) collapse into one, matching how a real permission
/// prompt can't be shown twice at once anyway.
static CAMERA_REQUEST_PENDING: Mutex<bool> = Mutex::new(false);

/// Called by app code (e.g. a button's `on_press`) to ask the native host
/// to show the camera permission prompt. Does nothing if a request is
/// already queued or the permission was already resolved either way —
/// callers don't need to guard against re-requesting themselves.
pub fn request_camera() {
    if CAMERA_PERMISSION.get().is_some() {
        return; // already resolved, nothing to re-request
    }
    let mut pending = CAMERA_REQUEST_PENDING.lock().unwrap_or_else(|e| e.into_inner());
    if *pending {
        return; // already queued, don't double up
    }
    *pending = true;
    // The returned Atom is intentionally dropped — see the module doc on
    // why camera/push report results via a plain setter instead of
    // correlating by call_id.
    invoke_method("rosace/camera", "requestPermission", Value::Null);
}

/// Called by the native host once its permission API resolves (e.g.
/// `AVCaptureDevice.requestAccess`'s completion handler). Writes
/// [`CAMERA_PERMISSION`], which notifies subscribers — any widget reading
/// it re-renders with the real answer.
pub fn report_camera_result(granted: bool) {
    CAMERA_PERMISSION.set(Some(granted));
    *CAMERA_REQUEST_PENDING.lock().unwrap_or_else(|e| e.into_inner()) = false;
}

// ── Push notifications (D110 Phase 29 Step 2) ────────────────────────────
//
// The second capability over the bridge — the exact shape the camera proof
// above established (request queue + result/state atoms + host-side native
// call), NOT a new architecture; see this module's doc. Two extra pieces
// camera didn't need: a device TOKEN (APNs/FCM registration outcome) and a
// foreground-delivery channel (the host reports a received notification; a
// widget reading it re-renders). Both of those are native pushing
// unprompted data INTO Rust, not a request/result round-trip, so they stay
// plain setters — there's nothing to "queue" or "discover" about them.

/// Push permission state — same `Option<bool>` semantics as
/// [`CAMERA_PERMISSION`] (`None` = never requested or still pending).
/// `0xFFF8` continues the reserved-high-id ladder (see the doc on
/// [`CAMERA_PERMISSION`]; `0xFFF9` is `rosace_core`'s app-lifecycle atom).
pub static PUSH_PERMISSION: GlobalAtom<Option<bool>> =
    GlobalAtom::new(AtomId(0xFFF8), || None);

/// The device push token (APNs hex token on iOS, FCM registration token on
/// Android), reported by the host after a successful registration. `None`
/// until then — registration is asynchronous and can legitimately fail
/// (no entitlement, no network), in which case this simply stays `None`.
pub static PUSH_TOKEN: GlobalAtom<Option<String>> =
    GlobalAtom::new(AtomId(0xFFF7), || None);

/// One push notification delivered while the app was FOREGROUNDED
/// (background/silent push and notification actions are explicitly out of
/// Phase 29's scope — see `.steering/PHASE_29.md`).
#[derive(Debug, Clone, PartialEq)]
pub struct PushMessage {
    /// 1-based receipt order — lets a widget tell two identical payloads
    /// apart (the atom only holds the LATEST message).
    pub seq: u64,
    pub title: String,
    pub body: String,
    /// The notification's full payload as JSON text (`aps` + custom keys)
    /// — the app parses whatever it needs; the framework doesn't impose a
    /// schema on custom keys.
    pub payload_json: String,
}

/// The most recent foreground-delivered push notification. Latest-wins by
/// design: this is a re-render signal plus payload, not a queue — an app
/// that must not miss messages should persist them from its own watcher
/// (`seq` makes gaps detectable).
pub static PUSH_MESSAGE: GlobalAtom<Option<PushMessage>> =
    GlobalAtom::new(AtomId(0xFFF6), || None);

/// Read [`PUSH_PERMISSION`] from a component's `build()`, subscribing the
/// component — see [`use_camera_permission`]'s doc for why the explicit
/// subscribe is required.
pub fn use_push_permission(ctx: &rosace_core::Context) -> Option<bool> {
    PUSH_PERMISSION.get_or_init().subscribe(ctx.component_id());
    PUSH_PERMISSION.get()
}

/// Read [`PUSH_TOKEN`] from a component's `build()`, subscribing the
/// component — see [`use_camera_permission`]'s doc for why the explicit
/// subscribe is required.
pub fn use_push_token(ctx: &rosace_core::Context) -> Option<String> {
    PUSH_TOKEN.get_or_init().subscribe(ctx.component_id());
    PUSH_TOKEN.get()
}

/// Read [`PUSH_MESSAGE`] from a component's `build()`, subscribing the
/// component — see [`use_camera_permission`]'s doc for why the explicit
/// subscribe is required.
pub fn use_push_message(ctx: &rosace_core::Context) -> Option<PushMessage> {
    PUSH_MESSAGE.get_or_init().subscribe(ctx.component_id());
    PUSH_MESSAGE.get()
}

static PUSH_REQUEST_PENDING: Mutex<bool> = Mutex::new(false);

/// Monotonic receipt counter for [`PushMessage::seq`].
static PUSH_SEQ: Mutex<u64> = Mutex::new(0);

/// Called by app code to ask the native host to request push permission
/// (and, on grant, register for a device token). Same collapse-duplicates
/// semantics as [`request_camera`]; queues on the well-known `"rosace/push"`
/// Platform Channel — the generated tick loop recognizes this one
/// unconditionally (unlike camera, every app already carries push-permission
/// polling, so there's no new per-app cost to special-casing it by default).
pub fn request_push_permission() {
    if PUSH_PERMISSION.get().is_some() {
        return; // already resolved, nothing to re-request
    }
    let mut pending = PUSH_REQUEST_PENDING.lock().unwrap_or_else(|e| e.into_inner());
    if *pending {
        return;
    }
    *pending = true;
    invoke_method("rosace/push", "requestPermission", Value::Null);
}

/// Called by the host when its permission API resolves
/// (`UNUserNotificationCenter.requestAuthorization` on iOS,
/// `POST_NOTIFICATIONS` on Android 13+).
pub fn report_push_result(granted: bool) {
    PUSH_PERMISSION.set(Some(granted));
    *PUSH_REQUEST_PENDING.lock().unwrap_or_else(|e| e.into_inner()) = false;
}

/// Called by the host once registration yields a device token
/// (`didRegisterForRemoteNotificationsWithDeviceToken` on iOS, FCM's
/// `onNewToken` on Android). May be called again later — tokens rotate.
pub fn report_push_token(token: impl Into<String>) {
    PUSH_TOKEN.set(Some(token.into()));
}

/// Called by the host when a notification arrives while the app is
/// foregrounded (`UNUserNotificationCenterDelegate.willPresent` on iOS,
/// `FirebaseMessagingService.onMessageReceived` on Android). Stamps the
/// receipt `seq` and notifies subscribers.
pub fn report_push_notification(
    title: impl Into<String>,
    body: impl Into<String>,
    payload_json: impl Into<String>,
) {
    let seq = {
        let mut guard = PUSH_SEQ.lock().unwrap_or_else(|e| e.into_inner());
        *guard += 1;
        *guard
    };
    PUSH_MESSAGE.set(Some(PushMessage {
        seq,
        title: title.into(),
        body: body.into(),
        payload_json: payload_json.into(),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_channel::take_outgoing_calls;
    use std::sync::Mutex as StdMutex;

    // `CAMERA_PERMISSION`/`CAMERA_REQUEST_PENDING` (and push's equivalents)
    // are process-global statics — tests touching them must be serialized
    // against each other, same reasoning as `rosace-cli`'s `CWD_LOCK`
    // (`test_support.rs`). Also serialized against `platform_channel`'s own
    // tests, since both share the one process-global outgoing-call queue.
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn request_camera_queues_a_platform_channel_call() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        CAMERA_PERMISSION.set(None);
        *CAMERA_REQUEST_PENDING.lock().unwrap() = false;
        take_outgoing_calls(); // drain anything left over from another test

        request_camera();
        let calls = take_outgoing_calls();
        assert!(
            calls.iter().any(|c| c.channel == "rosace/camera" && c.method == "requestPermission"),
            "request_camera must queue a rosace/camera Platform Channel call"
        );

        // A second request before resolution must not queue a duplicate.
        request_camera();
        let calls = take_outgoing_calls();
        assert!(
            !calls.iter().any(|c| c.channel == "rosace/camera"),
            "a pending camera request must not be queued twice"
        );

        CAMERA_PERMISSION.set(None);
        *CAMERA_REQUEST_PENDING.lock().unwrap() = false;
    }

    #[test]
    fn report_result_updates_the_atom_and_clears_pending() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        CAMERA_PERMISSION.set(None);
        *CAMERA_REQUEST_PENDING.lock().unwrap() = true;

        report_camera_result(true);
        assert_eq!(CAMERA_PERMISSION.get(), Some(true));
        assert!(!*CAMERA_REQUEST_PENDING.lock().unwrap(), "reporting a result must clear the pending flag");

        report_camera_result(false);
        assert_eq!(CAMERA_PERMISSION.get(), Some(false));

        CAMERA_PERMISSION.set(None);
    }

    #[test]
    fn request_is_a_noop_once_already_resolved() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *CAMERA_REQUEST_PENDING.lock().unwrap() = false;
        CAMERA_PERMISSION.set(Some(true));
        take_outgoing_calls();

        request_camera();
        let calls = take_outgoing_calls();
        assert!(
            !calls.iter().any(|c| c.channel == "rosace/camera"),
            "already-resolved permission shouldn't re-queue a request"
        );

        CAMERA_PERMISSION.set(None); // reset for other tests
    }

    // ── Push (D110 Phase 29 Step 2) — mirrors the camera tests above ─────

    #[test]
    fn push_request_queues_a_platform_channel_call() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        PUSH_PERMISSION.set(None);
        *PUSH_REQUEST_PENDING.lock().unwrap() = false;
        take_outgoing_calls();

        request_push_permission();
        let calls = take_outgoing_calls();
        assert!(calls.iter().any(|c| c.channel == "rosace/push" && c.method == "requestPermission"));

        PUSH_PERMISSION.set(None);
        *PUSH_REQUEST_PENDING.lock().unwrap() = false;
    }

    #[test]
    fn push_request_is_a_noop_once_already_resolved() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *PUSH_REQUEST_PENDING.lock().unwrap() = false;
        PUSH_PERMISSION.set(Some(false));
        take_outgoing_calls();

        request_push_permission();
        let calls = take_outgoing_calls();
        assert!(!calls.iter().any(|c| c.channel == "rosace/push"), "already-resolved permission shouldn't re-queue a request");

        PUSH_PERMISSION.set(None); // reset for other tests
    }

    #[test]
    fn push_result_and_token_update_their_atoms() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        PUSH_PERMISSION.set(None);
        PUSH_TOKEN.set(None);

        report_push_result(true);
        assert_eq!(PUSH_PERMISSION.get(), Some(true));

        report_push_token("ab12cd34");
        assert_eq!(PUSH_TOKEN.get(), Some("ab12cd34".to_string()));

        // Tokens rotate — a later report must win.
        report_push_token("ef56");
        assert_eq!(PUSH_TOKEN.get(), Some("ef56".to_string()));

        PUSH_PERMISSION.set(None);
        PUSH_TOKEN.set(None);
    }

    #[test]
    fn push_notifications_carry_increasing_seq_so_identical_payloads_differ() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        PUSH_MESSAGE.set(None);

        report_push_notification("Hi", "First", "{}");
        let first = PUSH_MESSAGE.get().expect("message must land");
        assert_eq!((first.title.as_str(), first.body.as_str()), ("Hi", "First"));

        // The exact same payload again must still be a NEW value (seq
        // differs), so subscribed widgets re-render.
        report_push_notification("Hi", "First", "{}");
        let second = PUSH_MESSAGE.get().expect("second message must land");
        assert!(second.seq > first.seq, "seq must strictly increase");
        assert_ne!(first, second, "identical payloads must not compare equal across receipts");

        PUSH_MESSAGE.set(None);
    }
}
