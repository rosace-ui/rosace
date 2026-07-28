//! The current device pixel scale (window `scale_factor`), published by the
//! engine each frame so the widget layer can reason in physical pixels.
//!
//! Widgets lay out in logical px, but some decisions depend on the physical
//! size — notably the GPU-composited scroll layer, whose offscreen texture is
//! `logical × scale` and is capped at a hardware/self-imposed dimension. A
//! widget that must stay within that cap has to know the scale to convert.
//!
//! Main-thread only (set during present, read during paint), so a plain
//! thread-local cell suffices — no atomics needed.

use std::cell::Cell;

thread_local! {
    static RENDER_SCALE: Cell<f32> = const { Cell::new(1.0) };
}

/// The latest device pixel scale (e.g. 2.0 on a Retina display). Defaults to
/// 1.0 until the engine publishes a real value.
pub fn render_scale() -> f32 {
    RENDER_SCALE.with(|s| s.get())
}

/// Publish the current device pixel scale (called by the engine each frame).
pub fn set_render_scale(scale: f32) {
    if scale.is_finite() && scale > 0.0 {
        RENDER_SCALE.with(|s| s.set(scale));
    }
}
