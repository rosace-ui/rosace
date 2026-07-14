//! Throwaway stub proving the Android JNI ABI end-to-end (D106 Phase 24
//! Step 3) — the Android counterpart to `ios_stub.rs`.
//!
//! Unlike iOS's plain C ABI, Kotlin's `external fun` resolves to a symbol
//! literally named `Java_<package>_<Class>_<method>` (JNI's mangling rules —
//! see `rosace-cli/src/commands/new.rs`'s `jni_mangle`, which computes this
//! exact name for a real app's bundle id at `rsc new` codegen time). This
//! stub hardcodes a throwaway package (`dev.rosace.stub`, class
//! `MainActivity`) purely to prove the shape compiles, links, and resolves
//! by name — `rsc new`'s real codegen generates the equivalent per-app.
//!
//! Build: `cargo build -p rosace-ffi --example android_stub --target
//! aarch64-linux-android --release` — produces `libandroid_stub.so`.
//! Verify exported symbols with `nm -D` (or the NDK's `llvm-nm`) against a
//! `Java_dev_rosace_stub_MainActivity_native*` name.

use rosace_core::{Component, Context, Element};
use rosace_ffi::{Engine, RscInputEventFfi};

#[cfg(target_os = "android")]
use jni::objects::JObject;
#[cfg(target_os = "android")]
use jni::sys::{jfloat, jint, jlong};
#[cfg(target_os = "android")]
use jni::JNIEnv;
#[cfg(target_os = "android")]
use rosace_ffi::AndroidSurfaceHandle;

/// Trivial root — same reasoning as `ios_stub.rs`'s `StubRoot`: this stub
/// only proves the ABI links and returns a handle, not visual correctness.
// Constructed only inside the android cfg branch of the JNI init fn.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct StubRoot;

impl Component for StubRoot {
    fn build(&self, _ctx: &mut Context) -> Element {
        Element::Empty
    }
}

/// The `Engine` plus the `AndroidSurfaceHandle` that must outlive it (its
/// `ANativeWindow` reference is what the engine's `GpuPresenter` renders
/// into) — boxed and torn down together in `nativeShutdown`.
#[cfg(target_os = "android")]
struct AndroidEngine {
    engine: Box<Engine>,
    #[allow(dead_code)] // kept alive for its Drop (ANativeWindow_release), never read
    surface: AndroidSurfaceHandle,
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_stub_MainActivity_nativeInit(
    env: JNIEnv,
    _class: JObject,
    surface: JObject,
    width: jint,
    height: jint,
    scale: jfloat,
) -> jlong {
    let raw_env = env.get_raw();
    let Some(handle) = (unsafe { AndroidSurfaceHandle::from_jni(raw_env, &surface) }) else {
        return 0;
    };
    let raw_surface = unsafe { handle.raw_surface(width as u32, height as u32, scale) };
    let theme = rosace_theme::built_in::light_theme();
    match Engine::init(Box::new(StubRoot), theme, raw_surface) {
        Some(engine) => Box::into_raw(Box::new(AndroidEngine { engine, surface: handle })) as jlong,
        None => 0,
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_stub_MainActivity_nativeResize(
    _env: JNIEnv,
    _class: JObject,
    handle: jlong,
    width: jint,
    height: jint,
    scale: jfloat,
    safe_top: jfloat,
    safe_right: jfloat,
    safe_bottom: jfloat,
    safe_left: jfloat,
) {
    if handle == 0 { return; }
    let ptr = handle as *mut AndroidEngine;
    let safe_area = rosace_core::SafeArea {
        top: safe_top, right: safe_right, bottom: safe_bottom, left: safe_left,
    };
    unsafe { (*ptr).engine.resize(width as u32, height as u32, scale, safe_area) };
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_stub_MainActivity_nativeInput(
    _env: JNIEnv,
    _class: JObject,
    handle: jlong,
    events: *const RscInputEventFfi,
    count: jint,
) {
    if handle == 0 || events.is_null() { return; }
    let ptr = handle as *mut AndroidEngine;
    let slice = unsafe { std::slice::from_raw_parts(events, count as usize) };
    unsafe { (*ptr).engine.input(slice) };
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_stub_MainActivity_nativeFrame(
    _env: JNIEnv,
    _class: JObject,
    handle: jlong,
) {
    if handle == 0 { return; }
    let ptr = handle as *mut AndroidEngine;
    unsafe { (*ptr).engine.frame() };
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_dev_rosace_stub_MainActivity_nativeShutdown(
    _env: JNIEnv,
    _class: JObject,
    handle: jlong,
) {
    if handle == 0 { return; }
    drop(unsafe { Box::from_raw(handle as *mut AndroidEngine) });
}

/// Keeps this file typecheckable while iterating on non-Android hosts (the
/// same reasoning `ios_stub.rs` uses for its non-ios/android `init` stub).
#[cfg(not(target_os = "android"))]
fn _typecheck_only() {
    let _ = StubRoot;
    let _: Option<Box<Engine>> = None;
    let _: Option<&[RscInputEventFfi]> = None;
}
