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
//! `include/tzr_engine.h`.

use std::os::raw::c_void;
#[cfg(any(target_os = "ios", target_os = "android"))]
use std::ptr::NonNull;

use rosace_core::{Component, Context, Element};
use rosace_ffi::{Engine, TzrInputEventFfi};
#[cfg(any(target_os = "ios", target_os = "android"))]
use rosace_ffi::RawSurface;

/// A trivial root component — this stub only needs to prove the ABI links
/// and returns a handle, not that a real UI paints correctly (that's
/// exercised by the existing desktop examples via `App::launch`, which now
/// shares this same `FrameEngine` internally).
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
pub unsafe extern "C" fn tzr_engine_init(
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_init(
    _surface_handle: *mut c_void,
    _width: u32,
    _height: u32,
    _scale: f32,
) -> *mut Engine {
    std::ptr::null_mut()
}

/// # Safety
/// `engine` must be a live pointer previously returned by `tzr_engine_init`
/// (or null, which is a no-op).
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_resize(
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
/// `engine` must be a live pointer from `tzr_engine_init`; `events` must
/// point to at least `count` valid `TzrInputEvent`s.
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_input(
    engine: *mut Engine,
    events: *const TzrInputEventFfi,
    count: usize,
) {
    if engine.is_null() || events.is_null() { return; }
    let slice = unsafe { std::slice::from_raw_parts(events, count) };
    unsafe { (*engine).input(slice) };
}

/// # Safety
/// `engine` must be a live pointer from `tzr_engine_init` (or null).
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_frame(engine: *mut Engine) {
    if engine.is_null() { return; }
    unsafe { (*engine).frame() };
}

/// # Safety
/// `engine` must be a pointer previously returned by `tzr_engine_init` and
/// not yet passed to this function; it must not be used again afterward.
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_shutdown(engine: *mut Engine) {
    if engine.is_null() { return; }
    drop(unsafe { Box::from_raw(engine) });
}

// -- Platform capabilities (D106 Phase 24 Step 5) ----------------------------
// Engine-independent by design — see tzr_engine.h's doc comment on these two.

#[no_mangle]
pub extern "C" fn tzr_camera_permission_take_request() -> u8 {
    rosace_ffi::take_camera_request() as u8
}

#[no_mangle]
pub extern "C" fn tzr_camera_permission_report_result(granted: u8) {
    rosace_ffi::report_camera_result(granted != 0);
}
