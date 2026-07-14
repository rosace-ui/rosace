//! Native surface handle (D106 Phase 24 Step 1).
//!
//! `GpuPresenter::new<W>` (`rosace-compositor`) is already generic over
//! `W: wgpu::rwh::HasWindowHandle + HasDisplayHandle + Send + Sync +
//! 'static` — nothing winit-specific. `RawSurface` implements those two
//! traits directly from a raw pointer the native host hands over (a
//! `CAMetalLayer*` on iOS, an `ANativeWindow*` on Android), so
//! `GpuPresenter` needs no changes to run outside winit.
//!
//! Implemented against `wgpu::rwh` (rather than depending on
//! `raw-window-handle` directly) so the version always matches whatever
//! `rosace-compositor` was built against.

use std::ptr::NonNull;

use wgpu::rwh::{
    AndroidDisplayHandle, AndroidNdkWindowHandle, DisplayHandle, HandleError, HasDisplayHandle,
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, UiKitDisplayHandle, UiKitWindowHandle,
    WindowHandle,
};

/// Which native surface kind `RawSurface` wraps.
#[derive(Debug, Clone, Copy)]
enum Kind {
    /// iOS: a `CAMetalLayer*` (optionally the owning `UIViewController*`).
    CaMetalLayer { ui_view: NonNull<std::ffi::c_void>, ui_view_controller: Option<NonNull<std::ffi::c_void>> },
    /// Android: an `ANativeWindow*` obtained from `ANativeWindow_fromSurface`.
    NativeWindow { a_native_window: NonNull<std::ffi::c_void> },
}

/// A native rendering surface handed over the FFI boundary by the host.
///
/// # Safety contract (upheld by the native host, not checked here)
/// The wrapped pointer must stay valid for the lifetime of the `Engine`
/// built from this surface — the host must call `rsc_engine_shutdown`
/// (dropping the `Engine`) before releasing the view/window it points to.
pub struct RawSurface {
    kind: Kind,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

impl RawSurface {
    /// Wraps an iOS `UIView`'s backing `CAMetalLayer`. `ui_view` must point
    /// at a live `UIView` (or `CALayer`) whose `layerClass` is
    /// `CAMetalLayer`; `ui_view_controller` is optional context some hosts
    /// provide for diagnostics.
    ///
    /// # Safety
    /// `ui_view` must be a valid, non-dangling pointer to a live `UIView`
    /// for as long as the returned `RawSurface` (and any `Engine` built from
    /// it) is in use.
    pub unsafe fn from_ca_metal_layer(
        ui_view: NonNull<std::ffi::c_void>,
        ui_view_controller: Option<NonNull<std::ffi::c_void>>,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Self {
        Self { kind: Kind::CaMetalLayer { ui_view, ui_view_controller }, width, height, scale }
    }

    /// Wraps an Android `ANativeWindow*` (typically from
    /// `ANativeWindow_fromSurface` on the JNI `Surface` backing a
    /// `SurfaceView`).
    ///
    /// # Safety
    /// `a_native_window` must be a valid, non-dangling `ANativeWindow*` for
    /// as long as the returned `RawSurface` (and any `Engine` built from it)
    /// is in use.
    pub unsafe fn from_native_window(
        a_native_window: NonNull<std::ffi::c_void>,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Self {
        Self { kind: Kind::NativeWindow { a_native_window }, width, height, scale }
    }
}

impl HasWindowHandle for RawSurface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = match self.kind {
            Kind::CaMetalLayer { ui_view, ui_view_controller } => {
                let mut h = UiKitWindowHandle::new(ui_view);
                h.ui_view_controller = ui_view_controller;
                RawWindowHandle::UiKit(h)
            }
            Kind::NativeWindow { a_native_window } => {
                RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(a_native_window))
            }
        };
        // SAFETY: the pointer inside `raw` is upheld valid by the caller's
        // contract on `from_ca_metal_layer`/`from_native_window`, for at
        // least as long as `self` (and this borrow) lives.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for RawSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = match self.kind {
            Kind::CaMetalLayer { .. } => RawDisplayHandle::UiKit(UiKitDisplayHandle::new()),
            Kind::NativeWindow { .. } => RawDisplayHandle::Android(AndroidDisplayHandle::new()),
        };
        // SAFETY: both display handle variants carry no borrowed data.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

// SAFETY: `RawSurface` only carries a raw pointer + plain data; the pointer
// itself is never dereferenced by ROSACE code (only handed to wgpu, which
// treats it as an opaque platform handle). The native host is responsible
// for only calling FFI functions that touch the underlying view/window from
// the correct thread (UIKit/Android UI thread), per the safety contract on
// the constructors above.
unsafe impl Send for RawSurface {}
unsafe impl Sync for RawSurface {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ca_metal_layer_surface_reports_ui_kit_handle() {
        let mut dummy: u8 = 0;
        let ptr = NonNull::new(&mut dummy as *mut u8 as *mut std::ffi::c_void).unwrap();
        let surface = unsafe { RawSurface::from_ca_metal_layer(ptr, None, 390, 844, 3.0) };
        match surface.window_handle().unwrap().as_raw() {
            RawWindowHandle::UiKit(h) => assert_eq!(h.ui_view, ptr),
            other => panic!("expected UiKit handle, got {other:?}"),
        }
        assert!(matches!(surface.display_handle().unwrap().as_raw(), RawDisplayHandle::UiKit(_)));
    }

    #[test]
    fn native_window_surface_reports_android_ndk_handle() {
        let mut dummy: u8 = 0;
        let ptr = NonNull::new(&mut dummy as *mut u8 as *mut std::ffi::c_void).unwrap();
        let surface = unsafe { RawSurface::from_native_window(ptr, 1080, 2340, 2.75) };
        match surface.window_handle().unwrap().as_raw() {
            RawWindowHandle::AndroidNdk(h) => assert_eq!(h.a_native_window, ptr),
            other => panic!("expected AndroidNdk handle, got {other:?}"),
        }
        assert!(matches!(surface.display_handle().unwrap().as_raw(), RawDisplayHandle::Android(_)));
    }
}
