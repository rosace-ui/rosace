/* tzr_engine.h — ROSACE native-host FFI boundary (D106 Phase 24 Step 1).
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
 * calling `tzr_engine_shutdown`.
 */

#ifndef TZR_ENGINE_H
#define TZR_ENGINE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle — never dereferenced by the host. */
typedef struct TzrEngine TzrEngine;

/* One input event. Unused fields for a given `kind` are ignored.
 * `character` holds a Unicode scalar value for TZR_EVENT_TEXT / TZR_KEY_CHAR.
 * Layout must match `rosace_ffi::event::TzrInputEventFfi` exactly. */
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
} TzrInputEvent;

/* `kind` values */
#define TZR_EVENT_MOUSE_MOVE     0
#define TZR_EVENT_MOUSE_DOWN     1
#define TZR_EVENT_MOUSE_UP       2
#define TZR_EVENT_KEY_DOWN       3
#define TZR_EVENT_KEY_UP         4
#define TZR_EVENT_TEXT           5
#define TZR_EVENT_WINDOW_RESIZED 6
#define TZR_EVENT_SCROLL         7

/* `button` values (TZR_EVENT_MOUSE_DOWN / TZR_EVENT_MOUSE_UP) */
#define TZR_BUTTON_LEFT   0
#define TZR_BUTTON_RIGHT  1
#define TZR_BUTTON_MIDDLE 2

/* `key` values (TZR_EVENT_KEY_DOWN / TZR_EVENT_KEY_UP); TZR_KEY_CHAR reads `character`. */
#define TZR_KEY_ENTER       0
#define TZR_KEY_ESCAPE      1
#define TZR_KEY_SPACE       2
#define TZR_KEY_BACKSPACE   3
#define TZR_KEY_TAB         4
#define TZR_KEY_ARROW_UP    5
#define TZR_KEY_ARROW_DOWN  6
#define TZR_KEY_ARROW_LEFT  7
#define TZR_KEY_ARROW_RIGHT 8
#define TZR_KEY_SHIFT       9
#define TZR_KEY_CONTROL     10
#define TZR_KEY_ALT         11
#define TZR_KEY_META        12
#define TZR_KEY_CHAR        13

/* Creates the engine against `surface_handle`. Returns NULL on failure
 * (e.g. no suitable GPU adapter) — mirrors `GpuPresenter::new`. */
TzrEngine *tzr_engine_init(void *surface_handle, uint32_t width, uint32_t height, float scale);

/* Resizes the surface + canvases (e.g. on rotation / layout change), and
 * updates the safe-area insets (logical points) — e.g. from a real
 * `UIView.safeAreaInsets` on iOS. Resize and safe-area change together in
 * practice (rotation, layout), so they share one call rather than needing a
 * separate function. Pass zeros if the host has no safe-area concept. */
void tzr_engine_resize(TzrEngine *engine, uint32_t width, uint32_t height, float scale,
                        float safe_top, float safe_right, float safe_bottom, float safe_left);

/* Queues `count` events, applied on the next `tzr_engine_frame` call. */
void tzr_engine_input(TzrEngine *engine, const TzrInputEvent *events, size_t count);

/* Builds (if dirty), paints, and presents one frame. */
void tzr_engine_frame(TzrEngine *engine);

/* Releases the engine. `engine` must not be used again after this call. */
void tzr_engine_shutdown(TzrEngine *engine);

/* -- Platform capabilities (D106 Phase 24 Step 5) --------------------------
 * Proves the native-host model reaches things a winit-owned app structurally
 * couldn't: a real permission prompt, with the result flowing back into Rust
 * app code. Deliberately ONE capability (camera) — see
 * rosace-ffi/src/capability.rs's module doc and .steering/PHASE_24.md's
 * Step 5 scope note. Engine-independent (not `TzrEngine*`-scoped) — there's
 * only one engine per app process, matching how `rosace_platform`'s own
 * scroll-layer registry is also a bare global, not window-scoped.
 *
 * Host usage (see ios_stub.rs / a real app's EngineViewController.swift):
 *   - Poll `tzr_camera_permission_take_request()` once per frame tick
 *     (alongside `tzr_engine_frame`). If it returns true, trigger the real
 *     platform permission API (e.g. `AVCaptureDevice.requestAccess`).
 *   - When that resolves, call `tzr_camera_permission_report_result`.
 */

/* True at most once per Rust-side `rosace_ffi::request_camera()` call —
 * act on it immediately, don't cache a `true` result. */
uint8_t tzr_camera_permission_take_request(void);

/* Reports the native permission API's result back into Rust — updates the
 * `rosace_ffi::CAMERA_PERMISSION` atom, which notifies subscribed widgets. */
void tzr_camera_permission_report_result(uint8_t granted);

#ifdef __cplusplus
}
#endif

#endif /* TZR_ENGINE_H */
