//! Native-host FFI glue (D106 Phase 24) — exports the ABI
//! `ios/App/EngineViewController.swift` and `android/.../MainActivity.kt`
//! call into. iOS uses the plain C ABI in `rosace-ffi`'s
//! `include/rsc_engine.h` (pattern: `rosace-ffi/examples/ios_stub.rs`).
//! Android uses JNI instead — Kotlin's `external fun` resolves to a symbol
//! literally named `Java_<package>_<Class>_<method>` (JNI's mangling: `.` ->
//! `_`, a literal `_` -> `_1` — see `jni_class_prefix` in
//! `rosace-cli/src/commands/new.rs`, which computed the exact prefix below
//! from this app's bundle id at `rsc new` time). Pattern:
//! `rosace-ffi/examples/android_stub.rs`.

use std::os::raw::c_void;
#[cfg(target_os = "ios")]
use std::ptr::NonNull;

#[cfg(any(target_os = "ios", target_os = "android"))]
use rosace::prelude::*;
use rosace_ffi::{Engine, RscInputEventFfi};
#[cfg(target_os = "ios")]
use rosace_ffi::RawSurface;
#[cfg(target_os = "android")]
use rosace_ffi::AndroidSurfaceHandle;

#[cfg(any(target_os = "ios", target_os = "android"))]
use crate::app::AppRoot;

// -- iOS: plain C ABI --------------------------------------------------------

/// # Safety
/// `surface_handle` must be a valid, non-null `CAMetalLayer`-backed
/// `UIView*` for the engine's lifetime.
#[cfg(target_os = "ios")]
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_init(
    surface_handle: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut Engine {
    let Some(handle) = NonNull::new(surface_handle) else { return std::ptr::null_mut() };
    let surface = unsafe { RawSurface::from_ca_metal_layer(handle, None, width, height, scale) };
    let theme = light_theme();
    match Engine::init(Box::new(AppRoot), theme, surface) {
        Some(engine) => Box::into_raw(engine),
        None => std::ptr::null_mut(),
    }
}

#[cfg(not(target_os = "ios"))]
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
    let safe_area = rosace::core::SafeArea { top: safe_top, right: safe_right, bottom: safe_bottom, left: safe_left };
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

// -- Android: JNI -------------------------------------------------------------
// Symbol names are burned in at codegen time (JNI resolves by exact name,
// no runtime registration) — see the module doc above for why this can't be
// the same plain-C functions iOS uses. `AndroidEngine` keeps the `Engine`
// and the `AndroidSurfaceHandle` (whose `Drop` releases the `ANativeWindow`
// reference) alive together, torn down as a unit in nativeShutdown — same
// reasoning as `rosace-ffi/examples/android_stub.rs`'s `AndroidEngine`.

#[cfg(target_os = "android")]
struct AndroidEngine {
    engine: Box<Engine>,
    #[allow(dead_code)]
    surface: AndroidSurfaceHandle,
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_demo_1app_MainActivity_nativeInit(
    env: jni::JNIEnv,
    _class: jni::objects::JObject,
    surface: jni::objects::JObject,
    width: jni::sys::jint,
    height: jni::sys::jint,
    scale: jni::sys::jfloat,
) -> jni::sys::jlong {
    let raw_env = env.get_raw();
    let Some(handle) = (unsafe { AndroidSurfaceHandle::from_jni(raw_env, &surface) }) else {
        return 0;
    };
    let raw_surface = unsafe { handle.raw_surface(width as u32, height as u32, scale) };
    let theme = light_theme();
    match Engine::init(Box::new(AppRoot), theme, raw_surface) {
        Some(engine) => Box::into_raw(Box::new(AndroidEngine { engine, surface: handle })) as jni::sys::jlong,
        None => 0,
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_demo_1app_MainActivity_nativeResize(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
    width: jni::sys::jint,
    height: jni::sys::jint,
    scale: jni::sys::jfloat,
    safe_top: jni::sys::jfloat,
    safe_right: jni::sys::jfloat,
    safe_bottom: jni::sys::jfloat,
    safe_left: jni::sys::jfloat,
) {
    if handle == 0 { return; }
    let ptr = handle as *mut AndroidEngine;
    let safe_area = rosace::core::SafeArea { top: safe_top, right: safe_right, bottom: safe_bottom, left: safe_left };
    unsafe { (*ptr).engine.resize(width as u32, height as u32, scale, safe_area) };
}

/// One touch/pointer event per call — `kind` is `0` = move, `1` = down,
/// `2` = up (matching `rosace_ffi`'s `RSC_EVENT_MOUSE_*` constants); a
/// touch is always reported as the left button, mirroring how the existing
/// winit `Touch` handling already treats touch input (see `rosace-ffi`'s
/// `event.rs` module doc).
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_demo_1app_MainActivity_nativeTouch(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
    kind: jni::sys::jint,
    x: jni::sys::jfloat,
    y: jni::sys::jfloat,
) {
    if handle == 0 { return; }
    let ptr = handle as *mut AndroidEngine;
    let event = RscInputEventFfi {
        kind: kind as u32, x, y, button: 0, key: 0, character: 0,
        width: 0, height: 0, delta_x: 0.0, delta_y: 0.0,
    };
    unsafe { (*ptr).engine.input(&[event]) };
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_demo_1app_MainActivity_nativeFrame(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
) {
    if handle == 0 { return; }
    let ptr = handle as *mut AndroidEngine;
    unsafe { (*ptr).engine.frame() };
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_demo_1app_MainActivity_nativeShutdown(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
) {
    if handle == 0 { return; }
    drop(unsafe { Box::from_raw(handle as *mut AndroidEngine) });
}
