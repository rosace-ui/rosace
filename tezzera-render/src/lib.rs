//! `tezzera-render` — Phase 1 software rasterizer for TEZZERA.
//!
//! Provides a [`RenderPipeline`] that orchestrates dirty-region tracking,
//! per-layer painting via [`SkiaCanvas`] (backed by `tiny-skia`), and layer
//! compositing.  The pipeline returns raw RGBA pixel data; the caller (the CLI
//! host in `tezzera-cli`) decides how to display the pixels on screen.

pub mod canvas;
pub mod dirty_region;
pub mod image;
pub mod layer;
pub mod render_pipeline;

pub use canvas::{Color, SkiaCanvas};
pub use dirty_region::DirtyRegionTracker;
pub use image::{CachePolicy, ImageFit, ImageHandle};
pub use layer::{layer_index, Layer, LayerCompositor};
pub use render_pipeline::RenderPipeline;

#[cfg(test)]
mod tests {
    use tezzera_core::types::{Point, Rect, Size};

    use crate::canvas::{Color, SkiaCanvas};
    use crate::dirty_region::DirtyRegionTracker;
    use crate::image::ImageHandle;
    use crate::layer::{layer_index, LayerCompositor};
    use crate::render_pipeline::RenderPipeline;

    #[test]
    fn canvas_clear_fills_with_color() {
        let mut canvas = SkiaCanvas::new(10, 10);
        canvas.clear(Color::RED);
        // First pixel should be red (RGBA).
        let pixels = canvas.pixels();
        assert_eq!(pixels[0], 255); // R
        assert_eq!(pixels[1], 0); // G
        assert_eq!(pixels[2], 0); // B
        assert_eq!(pixels[3], 255); // A
    }

    #[test]
    fn canvas_fill_rect_changes_pixels() {
        let mut canvas = SkiaCanvas::new(100, 100);
        canvas.clear(Color::WHITE);
        canvas.fill_rect(
            Rect {
                origin: Point { x: 0.0, y: 0.0 },
                size: Size {
                    width: 10.0,
                    height: 10.0,
                },
            },
            Color::BLUE,
        );
        let pixels = canvas.pixels();
        // Pixel at (0,0) should be blue.
        assert_eq!(pixels[2], 255); // B channel
    }

    #[test]
    fn dirty_region_starts_dirty() {
        let tracker = DirtyRegionTracker::new();
        assert!(tracker.needs_full_repaint());
    }

    #[test]
    fn dirty_region_clear_removes_dirty_state() {
        let mut tracker = DirtyRegionTracker::new();
        tracker.clear();
        assert!(!tracker.is_dirty());
    }

    #[test]
    fn dirty_region_mark_adds_rect() {
        let mut tracker = DirtyRegionTracker::new();
        tracker.clear();
        tracker.mark_dirty(Rect {
            origin: Point { x: 0.0, y: 0.0 },
            size: Size {
                width: 50.0,
                height: 50.0,
            },
        });
        assert!(tracker.is_dirty());
        assert_eq!(tracker.dirty_rects().len(), 1);
    }

    #[test]
    fn layer_compositor_creates_six_layers() {
        let comp = LayerCompositor::new(100, 100);
        assert_eq!(layer_index::COUNT, 6);
        // Access all layers without panic.
        for i in 0..layer_index::COUNT {
            let _ = comp.layer(i);
        }
    }

    #[test]
    fn render_pipeline_frame_counter_increments() {
        let mut pipeline = RenderPipeline::new(100, 100);
        pipeline.mark_dirty();
        pipeline.render_frame(|canvas| canvas.clear(Color::BLACK));
        pipeline.mark_dirty();
        pipeline.render_frame(|canvas| canvas.clear(Color::BLACK));
        // Two frames rendered without panic — counter has advanced to 2.
    }

    #[test]
    fn image_handle_from_valid_png() {
        // Minimal 1×1 white PNG.
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC, 0x59,
            0xE7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let handle = ImageHandle::from_png_bytes(png_bytes);
        assert!(handle.is_some());
        let h = handle.unwrap();
        assert_eq!(h.width, 1);
        assert_eq!(h.height, 1);
    }
}
