/* rsc_engine.h — ROSACE native-host FFI boundary (D106 Phase 24 Step 1).
 *
 * Hand-written (not cbindgen-generated — the surface is small and stable
 * enough not to warrant the extra build dependency). Keep this in sync with
 * the per-app `#[no_mangle] extern "C"` shims that implement it (see
 * `rosace-ffi/examples/ios_stub.rs` for the reference implementation every
 * generated app's own `src/ffi.rs` follows, per Phase 24 Step 2).
 *
 * `surface_handle`:
 *   - iOS:     a `CAMetalLayer`-backed `UIView*` (the view itself, not the
 *              layer — matches `RawSurface::from_ca_metal_layer`).
 *   - Android: an `ANativeWindow*` obtained via `ANativeWindow_fromSurface`.
 * It must stay valid for the engine's lifetime — release it only after
 * calling `rsc_engine_shutdown`.
 */

#ifndef RSC_ENGINE_H
#define RSC_ENGINE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle — never dereferenced by the host. */
typedef struct RscEngine RscEngine;

/* One input event. Unused fields for a given `kind` are ignored.
 * `character` holds a Unicode scalar value for RSC_EVENT_TEXT / RSC_KEY_CHAR.
 * Layout must match `rosace_ffi::event::RscInputEventFfi` exactly. */
typedef struct {
    uint32_t kind;
    float x;
    float y;
    uint32_t button;
    uint32_t key;
    uint32_t character;
    uint32_t width;
    uint32_t height;
    float delta_x;
    float delta_y;
} RscInputEvent;

/* `kind` values */
#define RSC_EVENT_MOUSE_MOVE     0
#define RSC_EVENT_MOUSE_DOWN     1
#define RSC_EVENT_MOUSE_UP       2
#define RSC_EVENT_KEY_DOWN       3
#define RSC_EVENT_KEY_UP         4
#define RSC_EVENT_TEXT           5
#define RSC_EVENT_WINDOW_RESIZED 6
#define RSC_EVENT_SCROLL         7
/* App-lifecycle transitions (D042/D110, Phase 29 Step 1) — send from the
 * host's real lifecycle callbacks (iOS UIApplication notifications /
 * Android onResume/onPause/onStop). All other fields are ignored. */
#define RSC_EVENT_LIFECYCLE_ACTIVE     8
#define RSC_EVENT_LIFECYCLE_INACTIVE   9
#define RSC_EVENT_LIFECYCLE_BACKGROUND 10
#define RSC_EVENT_LIFECYCLE_SUSPENDED  11

/* `button` values (RSC_EVENT_MOUSE_DOWN / RSC_EVENT_MOUSE_UP) */
#define RSC_BUTTON_LEFT   0
#define RSC_BUTTON_RIGHT  1
#define RSC_BUTTON_MIDDLE 2

/* `key` values (RSC_EVENT_KEY_DOWN / RSC_EVENT_KEY_UP); RSC_KEY_CHAR reads `character`. */
#define RSC_KEY_ENTER       0
#define RSC_KEY_ESCAPE      1
#define RSC_KEY_SPACE       2
#define RSC_KEY_BACKSPACE   3
#define RSC_KEY_TAB         4
#define RSC_KEY_ARROW_UP    5
#define RSC_KEY_ARROW_DOWN  6
#define RSC_KEY_ARROW_LEFT  7
#define RSC_KEY_ARROW_RIGHT 8
#define RSC_KEY_SHIFT       9
#define RSC_KEY_CONTROL     10
#define RSC_KEY_ALT         11
#define RSC_KEY_META        12
#define RSC_KEY_CHAR        13
/* Added D116 Phase 28 Step 6 (Known Issue #15) — see event.rs's comment
 * on the Rust-side constants for why these were missing until now. */
#define RSC_KEY_DELETE       14
#define RSC_KEY_HOME         15
#define RSC_KEY_END          16

/* Keyboard-type hint values (D116 Phase 28 Step 6) — poll
 * rsc_focused_keyboard_type() (per-app codegen wraps
 * rosace_ffi::focused_keyboard_type()) once per frame to know which
 * software keyboard layout to show for the currently-focused field. */
#define RSC_KEYBOARD_DEFAULT 0
#define RSC_KEYBOARD_EMAIL   1
#define RSC_KEYBOARD_NUMERIC 2
#define RSC_KEYBOARD_URL     3
#define RSC_KEYBOARD_PHONE   4

/* Creates the engine against `surface_handle`. Returns NULL on failure
 * (e.g. no suitable GPU adapter) — mirrors `GpuPresenter::new`. */
RscEngine *rsc_engine_init(void *surface_handle, uint32_t width, uint32_t height, float scale);

/* Resizes the surface + canvases (e.g. on rotation / layout change), and
 * updates the safe-area insets (logical points) — e.g. from a real
 * `UIView.safeAreaInsets` on iOS. Resize and safe-area change together in
 * practice (rotation, layout), so they share one call rather than needing a
 * separate function. Pass zeros if the host has no safe-area concept. */
void rsc_engine_resize(RscEngine *engine, uint32_t width, uint32_t height, float scale,
                        float safe_top, float safe_right, float safe_bottom, float safe_left);

/* Queues `count` events, applied on the next `rsc_engine_frame` call. */
void rsc_engine_input(RscEngine *engine, const RscInputEvent *events, size_t count);

/* Builds (if dirty), paints, and presents one frame. */
void rsc_engine_frame(RscEngine *engine);

/* Releases the engine. `engine` must not be used again after this call. */
void rsc_engine_shutdown(RscEngine *engine);

/* -- Platform capabilities (D106 Phase 24 Step 5) --------------------------
 * Proves the native-host model reaches things a winit-owned app structurally
 * couldn't: a real permission prompt, with the result flowing back into Rust
 * app code. Deliberately ONE capability (camera) — see
 * rosace-ffi/src/capability.rs's module doc and .steering/PHASE_24.md's
 * Step 5 scope note. Engine-independent (not `RscEngine*`-scoped) — there's
 * only one engine per app process, matching how `rosace_platform`'s own
 * scroll-layer registry is also a bare global, not window-scoped.
 *
 * Host usage (see ios_stub.rs / a real app's EngineViewController.swift):
 *   - Poll `rsc_camera_permission_take_request()` once per frame tick
 *     (alongside `rsc_engine_frame`). If it returns true, trigger the real
 *     platform permission API (e.g. `AVCaptureDevice.requestAccess`).
 *   - When that resolves, call `rsc_camera_permission_report_result`.
 */

/* True at most once per Rust-side `rosace_ffi::request_camera()` call —
 * act on it immediately, don't cache a `true` result. */
uint8_t rsc_camera_permission_take_request(void);

/* Reports the native permission API's result back into Rust — updates the
 * `rosace_ffi::CAMERA_PERMISSION` atom, which notifies subscribed widgets. */
void rsc_camera_permission_report_result(uint8_t granted);

/* -- Push notifications (D110 Phase 29 Step 2) ------------------------------
 * Same three-piece shape as camera, plus a device-token report and a
 * foreground-delivery report. Host usage (see a generated app's
 * EngineViewController.swift):
 *   - Poll `rsc_push_permission_take_request()` once per frame tick. If
 *     true: UNUserNotificationCenter.requestAuthorization (iOS) /
 *     POST_NOTIFICATIONS (Android 13+), then report the result; on grant,
 *     register for remote notifications.
 *   - Registration success → `rsc_push_report_token` (APNs hex / FCM token).
 *     Tokens rotate — report every time the OS hands you one.
 *   - A notification arriving while FOREGROUNDED (willPresent /
 *     onMessageReceived) → `rsc_push_report_notification`. Background and
 *     silent push are out of Phase 29's scope.
 */

/* True at most once per Rust-side `request_push_permission()` call. */
uint8_t rsc_push_permission_take_request(void);

/* Updates the `rosace_ffi::PUSH_PERMISSION` atom. */
void rsc_push_permission_report_result(uint8_t granted);

/* NUL-terminated UTF-8; updates the `rosace_ffi::PUSH_TOKEN` atom. */
void rsc_push_report_token(const char *token);

/* All three NUL-terminated UTF-8 (null reads as empty); updates the
 * `rosace_ffi::PUSH_MESSAGE` atom with a receipt-ordered `seq`. */
void rsc_push_report_notification(const char *title, const char *body,
                                  const char *payload_json);

#ifdef __cplusplus
}
#endif

#endif /* RSC_ENGINE_H */
