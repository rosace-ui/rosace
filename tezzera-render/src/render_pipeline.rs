use std::time::Instant;

use tezzera_trace::{
    event::{TezzeraTrace},
    trace,
};

use crate::canvas::SkiaCanvas;
use crate::dirty_region::DirtyRegionTracker;
use crate::layer::{layer_index, LayerCompositor};

/// Orchestrates the full per-frame render cycle:
/// dirty check → paint → composite → present.
///
/// Callers supply a `paint_fn` closure that receives the content-layer canvas.
/// The pipeline calls the closure only when the content layer is dirty,
/// preventing redundant redraws.
pub struct RenderPipeline {
    compositor: LayerCompositor,
    dirty: DirtyRegionTracker,
    frame_counter: u64,
    /// Per-frame time budget for the 60 fps target (≈ 16.67 ms).
    frame_budget_ms: f64,
}

impl RenderPipeline {
    /// Create a render pipeline for a surface with the given pixel dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            compositor: LayerCompositor::new(width, height),
            dirty: DirtyRegionTracker::new(),
            frame_counter: 0,
            frame_budget_ms: 16.667,
        }
    }

    /// Resize the pipeline to a new surface size.
    ///
    /// Triggers a full repaint on the next frame.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.compositor.resize(width, height);
        self.dirty.mark_full_repaint();
    }

    /// Mark the content layer dirty so it will be repainted on the next frame.
    pub fn mark_dirty(&mut self) {
        self.compositor
            .layer_mut(layer_index::CONTENT)
            .mark_dirty();
        self.dirty.mark_full_repaint();
    }

    /// Run a full frame: paint dirty layers, composite, return pixel data.
    ///
    /// `paint_fn` receives the content-layer canvas and should draw the current
    /// UI.  It is called only when the content layer is dirty.
    ///
    /// Returns a byte slice of raw RGBA pixel data for the rendered frame.
    pub fn render_frame<F>(&mut self, paint_fn: F) -> &[u8]
    where
        F: FnOnce(&mut SkiaCanvas),
    {
        let frame = self.frame_counter;
        self.frame_counter += 1;

        let start = Instant::now();
        trace!(TezzeraTrace::FrameStart {
            frame,
            timestamp: start,
        });

        // Paint the content layer only when it is dirty.
        let content = self.compositor.layer_mut(layer_index::CONTENT);
        if content.is_dirty() {
            paint_fn(content.canvas_mut());
            content.mark_clean();
        }

        // Composite all layers into a temporary output canvas.
        let width = self.compositor.width;
        let height = self.compositor.height;
        let mut output = SkiaCanvas::new(width, height);
        self.compositor.composite(&mut output);

        let duration = start.elapsed();
        let dropped = duration.as_secs_f64() * 1000.0 > self.frame_budget_ms;

        trace!(TezzeraTrace::FrameEnd {
            frame,
            duration,
            dropped,
        });

        if dropped {
            #[cfg(debug_assertions)]
            eprintln!(
                "[TEZZERA] frame #{} dropped ({:.2}ms > {:.2}ms budget)",
                frame,
                duration.as_secs_f64() * 1000.0,
                self.frame_budget_ms,
            );
        }

        self.dirty.clear();

        // Phase 1: return pixels from the content layer.
        // Real composited output (all six layers blended) will be returned in
        // Phase 2 once per-layer alpha blending is implemented.
        self.compositor
            .layer(layer_index::CONTENT)
            .canvas()
            .pixels()
    }
}
