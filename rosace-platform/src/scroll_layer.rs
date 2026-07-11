//! Scroll-layer handoff registry (D090).
//!
//! The frame loop renders each scrolling region's content into its own
//! content-sized RGBA buffer and `publish`es the set for the current frame.
//! The platform present path `take`s them and composites each as a placed GPU
//! layer (`CompositorLayer::placed`) at its viewport, sampling the content
//! texture at the scroll offset — so scrolling is a UV shift, not a base-canvas
//! re-rasterization (foundation for zero-repaint scroll).
//!
//! `take` returns `Some` only on frames where the frame loop published (i.e.
//! it repainted). On clean/skipped frames it returns `None`, and the platform
//! reuses the retained set — so the layers persist across frame-skips and a
//! surface-resize-forced present doesn't drop them.

use std::cell::RefCell;

/// One scrolling region handed from the frame loop to the compositor.
#[derive(Clone)]
pub struct ScrollLayer {
    /// Render-tree node id — keys the non-reactive scroll-offset channel
    /// (`rosace_state::scroll_offset`) so a wheel tick shifts this layer's UV
    /// without a repaint.
    pub id:     u64,
    /// Content texture, RGBA8, `width * height * 4` bytes (physical pixels).
    /// Empty when `items` is used instead (GPU-shapes mode, D109 C2).
    pub pixels: Vec<u8>,
    pub width:  u32,
    pub height: u32,
    /// Viewport placement on screen in physical pixels: `(x, y, w, h)`.
    pub dest:   (f32, f32, f32, f32),
    /// GPU-shapes mode (D109 C2): the content as ordered frame items
    /// (shape quads + CPU segments) instead of a pre-rasterized pixel
    /// buffer — the compositor renders them into an offscreen texture on
    /// publish and samples it at the live scroll offset every frame.
    /// Empty ⇒ use `pixels` (CPU path).
    pub items:  Vec<rosace_render::canvas::CanvasFrameItem>,
}

thread_local! {
    /// `Some` after the frame loop publishes for a repaint frame; `None` on
    /// clean frames (nothing published → reuse the platform's retained set).
    static SCROLL_LAYERS: RefCell<Option<Vec<ScrollLayer>>> = const { RefCell::new(None) };
}

/// Frame loop: publish this repaint frame's scroll layers (may be empty).
pub fn publish_scroll_layers(layers: Vec<ScrollLayer>) {
    SCROLL_LAYERS.with(|s| *s.borrow_mut() = Some(layers));
}

/// Platform present: take the published set, if any. `None` = reuse retained.
pub fn take_scroll_layers() -> Option<Vec<ScrollLayer>> {
    SCROLL_LAYERS.with(|s| s.borrow_mut().take())
}
