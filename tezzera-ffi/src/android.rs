//! Android JNI surface acquisition (D106 Phase 24 Step 3).
//!
//! Android's native host calls Rust via JNI, not a plain C ABI — a Kotlin
//! `external fun` resolves to a symbol named `Java_<package>_<Class>_<method>`
//! taking `JNIEnv`/`JObject` parameters, unlike iOS's plain
//! `tzr_engine_init(*mut c_void, ...)`. Since the package name varies per
//! app, those `Java_*`-named functions are generated per-app by `tzr new`
//! (mirroring how `tzr_engine_*` itself is per-app glue — see
//! `examples/ios_stub.rs`'s module doc). What's reusable, and lives here, is
//! turning the JNI `android.view.Surface` object the generated glue
//! receives into the same kind of raw pointer `RawSurface::from_native_window`
//! already expects.
//!
//! The NDK's `ANativeWindow_fromSurface` acquires a reference that must be
//! released with a matching `ANativeWindow_release` — `AndroidSurfaceHandle`
//! does this via `Drop`, so a generated app just keeps the handle alongside
//! its `Engine` (both torn down together on `tzr_engine_shutdown`) instead of
//! managing the raw pointer by hand.

use std::ptr::NonNull;

use jni::objects::JObject;
use jni::sys::JNIEnv as RawJNIEnv;

use crate::surface::RawSurface;

/// An acquired `ANativeWindow*` reference, released on drop.
///
/// # Safety contract
/// Must be dropped only after the `Engine` built from its `RawSurface` has
/// been dropped — same ordering `RawSurface::from_native_window`'s own
/// safety contract already requires. The simplest way to guarantee this is
/// to store the `Engine` and this handle together and drop them as a unit
/// (see `tzr new`'s generated `src/ffi.rs` android section).
pub struct AndroidSurfaceHandle {
    window: NonNull<std::ffi::c_void>,
}

impl AndroidSurfaceHandle {
    /// Acquires a native window from a JNI `Surface` object (as handed to a
    /// `SurfaceHolder.Callback.surfaceCreated`/`surfaceChanged` override,
    /// passed down to Rust through a generated `external fun`).
    ///
    /// # Safety
    /// `env` must be a valid, currently-attached `JNIEnv*` for the calling
    /// thread; `surface` must be a valid, non-null `android.view.Surface`
    /// JNI reference for the duration of this call.
    pub unsafe fn from_jni(env: *mut RawJNIEnv, surface: &JObject) -> Option<Self> {
        let raw = unsafe {
            ndk_sys::ANativeWindow_fromSurface(
                env.cast(),
                surface.as_raw().cast(),
            )
        };
        let window = NonNull::new(raw.cast::<std::ffi::c_void>())?;
        Some(Self { window })
    }

    /// Wraps this handle's window as a `RawSurface` for `Engine::init`.
    ///
    /// # Safety
    /// Same contract as `RawSurface::from_native_window`: the returned
    /// `RawSurface` (and any `Engine` built from it) must not outlive
    /// `self`.
    pub unsafe fn raw_surface(&self, width: u32, height: u32, scale: f32) -> RawSurface {
        unsafe { RawSurface::from_native_window(self.window, width, height, scale) }
    }
}

impl Drop for AndroidSurfaceHandle {
    fn drop(&mut self) {
        // SAFETY: `self.window` was returned by a successful
        // `ANativeWindow_fromSurface` call (that's the only way to construct
        // `Self`), so it holds one reference this release call balances.
        unsafe { ndk_sys::ANativeWindow_release(self.window.as_ptr().cast()) };
    }
}
