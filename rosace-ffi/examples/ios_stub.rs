//! Throwaway stub proving the FFI ABI end-to-end (D106 Phase 24 Step 1).
//!
//! This is NOT shipped code — it exists to prove that `rosace-ffi`'s safe
//! Rust API can be wrapped in exactly the ~15 lines of `#[no_mangle] extern
//! "C"` glue a real app needs, that the result compiles to a staticlib for
//! `aarch64-apple-ios-sim`, and that a hand-written Swift host can link and
//! call it. Phase 24 Step 2's `rsc new` codegen will generate the real
//! per-app equivalent of this file (with the app's actual root `Component`
//! in place of `StubRoot`) as `src/ffi.rs`.
//!
//! Build: `cargo build -p rosace-ffi --example ios_stub --target
//! aarch64-apple-ios-sim --release` — produces `libios_stub.a`, matching
//! `include/rsc_engine.h`.

use std::os::raw::c_void;
#[cfg(any(target_os = "ios", target_os = "android"))]
use std::ptr::NonNull;

use rosace_core::{Component, Context, Element};
use rosace_ffi::{Engine, RscInputEventFfi};
#[cfg(any(target_os = "ios", target_os = "android"))]
use rosace_ffi::RawSurface;

/// A trivial root component — this stub only needs to prove the ABI links
/// and returns a handle, not that a real UI paints correctly (that's
/// exercised by the existing desktop examples via `App::launch`, which now
/// shares this same `FrameEngine` internally).
// Constructed only inside the ios/android cfg branch of `rsc_engine_init`.
#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
struct StubRoot;

impl Component for StubRoot {
    fn build(&self, _ctx: &mut Context) -> Element {
        Element::Empty
    }
}

/// # Safety
/// `surface_handle` must be a valid, non-null `CAMetalLayer`-backed
/// `UIView*` (iOS) or `ANativeWindow*` (Android) for the engine's lifetime.
#[cfg(any(target_os = "ios", target_os = "android"))]
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_init(
    surface_handle: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut Engine {
    let Some(handle) = NonNull::new(surface_handle) else { return std::ptr::null_mut() };

    #[cfg(target_os = "ios")]
    let surface = unsafe { RawSurface::from_ca_metal_layer(handle, None, width, height, scale) };
    #[cfg(target_os = "android")]
    let surface = unsafe { RawSurface::from_native_window(handle, width, height, scale) };

    let theme = rosace_theme::built_in::light_theme();
    match Engine::init(Box::new(StubRoot), theme, surface) {
        Some(engine) => Box::into_raw(engine),
        None => std::ptr::null_mut(),
    }
}

/// This stub only has a native surface kind on iOS/Android; on other host
/// targets (used only to typecheck this file while iterating) `init` always
/// fails closed rather than pretending to construct a surface.
///
/// # Safety
/// No requirements — this fallback never dereferences `_surface_handle`
/// and always returns null. `unsafe` only to keep the ABI signature
/// identical to the real mobile variant above.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_init(
    _surface_handle: *mut c_void,
    _width: u32,
    _height: u32,
    _scale: f32,
) -> *mut Engine {
    std::ptr::null_mut()
}

/// # Safety
/// `engine` must be a live pointer previously returned by `rsc_engine_init`
/// (or null, which is a no-op).
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_resize(
    engine: *mut Engine,
    width: u32,
    height: u32,
    scale: f32,
    safe_top: f32,
    safe_right: f32,
    safe_bottom: f32,
    safe_left: f32,
) {
    if engine.is_null() { return; }
    let safe_area = rosace_core::SafeArea { top: safe_top, right: safe_right, bottom: safe_bottom, left: safe_left };
    unsafe { (*engine).resize(width, height, scale, safe_area) };
}

/// # Safety
/// `engine` must be a live pointer from `rsc_engine_init`; `events` must
/// point to at least `count` valid `RscInputEvent`s.
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_input(
    engine: *mut Engine,
    events: *const RscInputEventFfi,
    count: usize,
) {
    if engine.is_null() || events.is_null() { return; }
    let slice = unsafe { std::slice::from_raw_parts(events, count) };
    unsafe { (*engine).input(slice) };
}

/// # Safety
/// `engine` must be a live pointer from `rsc_engine_init` (or null).
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_frame(engine: *mut Engine) {
    if engine.is_null() { return; }
    unsafe { (*engine).frame() };
}

/// # Safety
/// `engine` must be a pointer previously returned by `rsc_engine_init` and
/// not yet passed to this function; it must not be used again afterward.
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_shutdown(engine: *mut Engine) {
    if engine.is_null() { return; }
    drop(unsafe { Box::from_raw(engine) });
}

// -- Platform capabilities (D106 Phase 24 Step 5) ----------------------------
// Engine-independent by design — see rsc_engine.h's doc comment. Request
// DISCOVERY goes through the generic Platform Channel poll below (D127),
// not a dedicated take_request per capability — see rosace_ffi::capability's
// module doc. Result-reporting stays a plain setter (no call_id correlation
// needed for a singleton capability).

#[no_mangle]
pub extern "C" fn rsc_camera_permission_report_result(granted: u8) {
    rosace_ffi::report_camera_result(granted != 0);
}

// -- Push notifications (D110 Phase 29 Step 2) — same shape as camera, plus
// a token report and a foreground-delivery report (both C strings).

#[no_mangle]
pub extern "C" fn rsc_push_permission_report_result(granted: u8) {
    rosace_ffi::report_push_result(granted != 0);
}

// -- Platform Channel (D127) --------------------------------------------------
// The generic bidirectional method-call bridge — see rsc-cli's `ffi_rs`
// generator (rosace-cli/src/commands/new.rs) for the canonical, fully
// commented version this mirrors; kept brief here since this file is a
// reference stub, not the actual per-app generated glue.

#[no_mangle]
pub extern "C" fn rsc_platform_channel_take_outgoing() -> *mut std::os::raw::c_char {
    let calls: Vec<serde_json::Value> = rosace_ffi::take_outgoing_calls()
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "call_id": c.call_id,
                "channel": c.channel,
                "method": c.method,
                "args": serde_json::from_str::<serde_json::Value>(&c.args_json)
                    .unwrap_or(serde_json::Value::Null),
            })
        })
        .collect();
    let text = serde_json::Value::Array(calls).to_string();
    std::ffi::CString::new(text).unwrap_or_default().into_raw()
}

/// # Safety
/// `ptr` must be either null (a no-op) or a pointer this crate returned
/// across the FFI boundary, not yet freed.
#[no_mangle]
pub unsafe extern "C" fn rsc_string_free(ptr: *mut std::os::raw::c_char) {
    if ptr.is_null() { return; }
    drop(unsafe { std::ffi::CString::from_raw(ptr) });
}

/// # Safety
/// `result_json` must be a valid NUL-terminated C string or null (a no-op).
#[no_mangle]
pub unsafe extern "C" fn rsc_platform_channel_report_result(
    call_id: u64,
    result_json: *const std::os::raw::c_char,
) {
    if result_json.is_null() { return; }
    let json = unsafe { std::ffi::CStr::from_ptr(result_json) }.to_string_lossy();
    rosace_ffi::report_call_result(call_id, &json);
}

/// # Safety
/// `message` must be a valid NUL-terminated C string or null (a no-op).
#[no_mangle]
pub unsafe extern "C" fn rsc_platform_channel_report_error(
    call_id: u64,
    message: *const std::os::raw::c_char,
) {
    if message.is_null() { return; }
    let msg = unsafe { std::ffi::CStr::from_ptr(message) }.to_string_lossy().into_owned();
    rosace_ffi::report_call_error(call_id, msg);
}

/// # Safety
/// Each argument must be a valid NUL-terminated C string or null. The
/// returned pointer is owned — pass it to `rsc_string_free` when done.
#[no_mangle]
pub unsafe extern "C" fn rsc_platform_channel_dispatch(
    channel: *const std::os::raw::c_char,
    method: *const std::os::raw::c_char,
    args_json: *const std::os::raw::c_char,
) -> *mut std::os::raw::c_char {
    let read = |p: *const std::os::raw::c_char| -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    let result = rosace_ffi::dispatch_call(&read(channel), &read(method), &read(args_json));
    std::ffi::CString::new(result).unwrap_or_default().into_raw()
}

/// # Safety
/// `token` must be a valid NUL-terminated C string (UTF-8 expected; other
/// bytes are replaced, never UB) or null (a no-op).
#[no_mangle]
pub unsafe extern "C" fn rsc_push_report_token(token: *const std::os::raw::c_char) {
    if token.is_null() { return; }
    let token = unsafe { std::ffi::CStr::from_ptr(token) }.to_string_lossy().into_owned();
    rosace_ffi::report_push_token(token);
}

/// # Safety
/// Each argument must be a valid NUL-terminated C string or null (null
/// reads as the empty string; the call still delivers).
#[no_mangle]
pub unsafe extern "C" fn rsc_push_report_notification(
    title: *const std::os::raw::c_char,
    body: *const std::os::raw::c_char,
    payload_json: *const std::os::raw::c_char,
) {
    let read = |p: *const std::os::raw::c_char| -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    rosace_ffi::report_push_notification(read(title), read(body), read(payload_json));
}
