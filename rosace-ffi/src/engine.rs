//! `Engine` — the safe Rust API a native host drives (D106 Phase 24 Step 1).
//!
//! Wraps `rosace::FrameEngine` (build/paint/input) + `GpuPresenter`
//! (`rosace-compositor`) + the base/overlay `SkiaCanvas`es, replaying the
//! same per-frame sequence `rosace-platform`'s winit loop runs (see
//! `rosace-platform/src/app.rs`'s `RedrawRequested` handler) but driven by
//! explicit `resize`/`input`/`frame` calls instead of a winit event loop.
//!
//! This struct is intentionally NOT exposed as `#[no_mangle] extern "C"`
//! here — only a concrete app crate knows its root `Component`, so the
//! actual `tzr_engine_*` C functions are generated per-app (see
//! `examples/ios_stub.rs` for the pattern). `Engine` is what that ~15 lines
//! of per-app glue calls into.

use rosace_compositor::{CompositorLayer, GpuPresenter, LayerRect};
use rosace_core::Component;
use rosace_render::SkiaCanvas;
use rosace_theme::ThemeData;

use crate::event::TzrInputEventFfi;
use crate::surface::RawSurface;

pub struct Engine {
    frame_engine: rosace::FrameEngine,
    presenter: GpuPresenter,
    canvas: SkiaCanvas,
    overlay_canvas: SkiaCanvas,
    scroll_layers: Vec<rosace_platform::ScrollLayer>,
    pending_events: Vec<rosace_platform::InputEvent>,
    width: u32,
    height: u32,
    scale: f32,
}

impl Engine {
    /// Builds an engine for `root`, targeting `surface`. Returns `None` if
    /// the host GPU/surface setup fails (mirrors `GpuPresenter::new`).
    /// Sets `theme` as the active theme once, like `App::launch` does.
    pub fn init(root: Box<dyn Component>, theme: ThemeData, surface: RawSurface) -> Option<Box<Engine>> {
        let width = surface.width;
        let height = surface.height;
        let scale = surface.scale;

        rosace_theme::set_theme(theme);

        let presenter = GpuPresenter::new(surface, width, height)?;

        let font = rosace_render::FontCache::system_ui()
            .or_else(rosace_render::FontCache::system_mono)
            .unwrap_or_else(rosace_render::FontCache::embedded);

        Some(Box::new(Engine {
            frame_engine: rosace::FrameEngine::new(root, font),
            presenter,
            canvas: SkiaCanvas::new_hidpi(width, height, scale),
            overlay_canvas: SkiaCanvas::new_hidpi(width, height, scale),
            scroll_layers: Vec::new(),
            pending_events: Vec::new(),
            width,
            height,
            scale,
        }))
    }

    /// Resizes the surface, presenter, and canvases (e.g. on device rotation
    /// or `viewWillLayoutSubviews`), and updates the safe-area insets (e.g.
    /// from a real `UIView.safeAreaInsets` on iOS — Phase 24 Step 2). The
    /// safe-area update always applies, even when the size/scale portion is
    /// a no-op, since insets can change independently (status bar changes,
    /// keyboard) without a size change.
    pub fn resize(
        &mut self,
        width: u32,
        height: u32,
        scale: f32,
        safe_area: rosace_core::SafeArea,
    ) {
        rosace_core::set_safe_area(safe_area);

        if width == 0 || height == 0 { return; }
        if self.width == width && self.height == height && (self.scale - scale).abs() < 0.01 {
            return;
        }
        self.width = width;
        self.height = height;
        self.scale = scale;
        self.presenter.resize(width, height);
        self.canvas = SkiaCanvas::new_hidpi(width, height, scale);
        self.overlay_canvas = SkiaCanvas::new_hidpi(width, height, scale);
    }

    /// Queues input events for the next `frame()` call — mirrors how the
    /// winit path batches `WindowEvent`s between `RedrawRequested`s.
    pub fn input(&mut self, events: &[TzrInputEventFfi]) {
        self.pending_events.extend(events.iter().map(|&e| e.into()));
    }

    /// Runs one frame: build/paint/dispatch (via `FrameEngine`), then
    /// composite + present (via `GpuPresenter`) — the same two-step sequence
    /// `rosace-platform/src/app.rs`'s `RedrawRequested` handler runs.
    pub fn frame(&mut self) {
        self.overlay_canvas.clear_transparent();
        let events = std::mem::take(&mut self.pending_events);
        self.frame_engine.paint(&mut self.canvas, &mut self.overlay_canvas, &events);

        let base_dirty = self.canvas.take_frame_dirty();

        let refreshed = rosace_platform::take_scroll_layers();
        let scroll_dirty = refreshed.is_some();
        if let Some(layers) = refreshed {
            self.scroll_layers = layers;
        }

        let mut layers = vec![
            CompositorLayer::tracked(self.canvas.pixels(), self.width, self.height, base_dirty),
        ];
        for sl in &self.scroll_layers {
            let off = rosace_state::scroll_offset(sl.id);
            layers.push(CompositorLayer::placed(
                &sl.pixels, sl.width, sl.height,
                LayerRect { x: sl.dest.0, y: sl.dest.1, w: sl.dest.2, h: sl.dest.3 },
                (off[0] * self.scale, off[1] * self.scale),
                scroll_dirty,
            ));
        }
        if self.overlay_canvas.has_drawn() {
            layers.push(CompositorLayer::tracked(self.overlay_canvas.pixels(), self.width, self.height, true));
        }
        self.presenter.present_layers(&layers);
    }
}
