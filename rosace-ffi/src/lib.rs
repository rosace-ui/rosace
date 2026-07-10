//! Native-host FFI boundary (D106 Phase 24 Step 1).
//!
//! ROSACE's iOS/Android story today has winit generate its own implicit
//! `AppDelegate`, which structurally blocks push notifications, deep links,
//! background tasks, and Xcode-editable capabilities/signing — see
//! `.steering/PHASE_24.md`. The fix: a native host (Swift `AppDelegate` on
//! iOS, Kotlin `Activity` on Android) owns the app lifecycle and drives the
//! Rust engine through this small C-compatible boundary instead.
//!
//! This crate provides the safe Rust machinery (`Engine`, `RawSurface`,
//! `TzrInputEventFfi`); it does NOT itself export `#[no_mangle] extern "C"`
//! functions, because only a concrete app knows its root `Component`. Each
//! app's own thin glue (see `examples/ios_stub.rs`) supplies that and
//! exports the actual `tzr_engine_init`/`_resize`/`_input`/`_frame`/
//! `_shutdown` symbols described in `include/tzr_engine.h` — the same
//! pattern `rsc new`'s generated `lib.rs` already uses for
//! `#[wasm_bindgen(start)]` on web.

#[cfg(target_os = "android")]
mod android;
mod capability;
mod engine;
mod event;
mod surface;

#[cfg(target_os = "android")]
pub use android::AndroidSurfaceHandle;
pub use capability::{report_camera_result, request_camera, take_camera_request, CAMERA_PERMISSION};
pub use engine::Engine;
pub use event::TzrInputEventFfi;
pub use surface::RawSurface;
