//! `rosace-render` — software rasterizer and display-list recording for ROSACE.
//!
//! Provides [`SkiaCanvas`] (backed by `tiny-skia`), [`PictureRecorder`] for
//! recording draw commands during the paint pass, and [`Picture`] for replaying
//! them. The [`FontCache`] handles glyph rasterization.

pub mod canvas;
pub mod draw_command;
pub mod font;
pub mod image;
pub mod picture;

pub use canvas::{Color, ShaderQuadCmd, SkiaCanvas};
pub use draw_command::DrawCommand;
pub use font::{FontCache, FontWeight};
pub use image::{CachePolicy, ImageFit, ImageHandle};
pub use picture::{Picture, PictureRecorder};

#[cfg(test)]
mod tests {
    use rosace_core::types::{Point, Rect, Size};

    use crate::canvas::{Color, SkiaCanvas};
    use crate::image::ImageHandle;

    #[test]
    fn canvas_clear_fills_with_color() {
        let mut canvas = SkiaCanvas::new(10, 10);
        canvas.clear(Color::RED);
        let pixels = canvas.pixels();
        assert_eq!(pixels[0], 255); // R
        assert_eq!(pixels[1], 0);   // G
        assert_eq!(pixels[2], 0);   // B
        assert_eq!(pixels[3], 255); // A
    }

    #[test]
    fn canvas_fill_rect_changes_pixels() {
        let mut canvas = SkiaCanvas::new(100, 100);
        canvas.clear(Color::WHITE);
        canvas.fill_rect(
            Rect {
                origin: Point { x: 0.0, y: 0.0 },
                size: Size { width: 10.0, height: 10.0 },
            },
            Color::BLUE,
        );
        let pixels = canvas.pixels();
        assert_eq!(pixels[2], 255); // B channel
    }

    #[test]
    fn blit_rgba_at_full_opacity_replaces_the_background() {
        let mut canvas = SkiaCanvas::new(4, 4);
        canvas.clear(Color::WHITE);
        // A single fully-opaque red pixel, blitted 1:1.
        let red_pixel = vec![255u8, 0, 0, 255];
        canvas.blit_rgba(&red_pixel, 1, 1, Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 1.0, height: 1.0 } }, 1.0);
        let px = canvas.pixels();
        assert_eq!(&px[0..4], &[255, 0, 0, 255], "opacity=1.0 must fully replace the background");
    }

    #[test]
    fn blit_rgba_at_zero_opacity_leaves_the_background_untouched() {
        let mut canvas = SkiaCanvas::new(4, 4);
        canvas.clear(Color::WHITE);
        let red_pixel = vec![255u8, 0, 0, 255];
        canvas.blit_rgba(&red_pixel, 1, 1, Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 1.0, height: 1.0 } }, 0.0);
        let px = canvas.pixels();
        assert_eq!(&px[0..4], &[255, 255, 255, 255], "opacity=0.0 must leave the white background untouched — this is the D108/Phase 26 Step 4 image fade-in's very first frame");
    }

    #[test]
    fn blit_rgba_at_half_opacity_blends_partway_between_background_and_source() {
        let mut canvas = SkiaCanvas::new(4, 4);
        canvas.clear(Color::WHITE);
        let red_pixel = vec![255u8, 0, 0, 255];
        canvas.blit_rgba(&red_pixel, 1, 1, Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 1.0, height: 1.0 } }, 0.5);
        let px = canvas.pixels();
        // Halfway from white (255,255,255) toward red (255,0,0): R stays
        // 255, G/B roughly halve. Allow rounding slack.
        assert_eq!(px[0], 255, "R channel");
        assert!((100..156).contains(&px[1]), "G channel should be roughly halved, got {}", px[1]);
        assert!((100..156).contains(&px[2]), "B channel should be roughly halved, got {}", px[2]);
    }

    // ── ShaderFill collection (D109/Phase 27 Step 2) ─────────────────────

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
    }

    #[test]
    fn shader_fill_is_collected_scaled_to_physical_px_not_rasterized() {
        use crate::draw_command::DrawCommand;
        use crate::font::FontCache;
        use crate::picture::PictureRecorder;

        // HiDPI canvas: logical coords must scale ×2 into the quad.
        let mut canvas = SkiaCanvas::new_hidpi(200, 200, 2.0);
        canvas.clear(Color::WHITE);
        let before = canvas.pixels().to_vec();

        let mut rec = PictureRecorder::new();
        rec.push(DrawCommand::ShaderFill {
            pipeline_id: 0x200,
            rect: rect(10.0, 20.0, 30.0, 40.0),
            uniforms: vec![1, 2, 3, 4],
        });
        canvas.play_picture(&rec.finish(), &FontCache::embedded());

        let quads = canvas.take_shader_quads();
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].pipeline_id, 0x200);
        assert_eq!(quads[0].rect, (20.0, 40.0, 60.0, 80.0), "must be physical px (×2)");
        assert_eq!(quads[0].uniforms, vec![1, 2, 3, 4]);
        assert_eq!(quads[0].clip, None);
        // No CPU pixel was touched — ShaderFill has no raster path.
        assert_eq!(canvas.pixels(), &before[..], "ShaderFill must not rasterize");
        assert!(canvas.take_shader_quads().is_empty(), "take must drain");
    }

    #[test]
    fn shader_fill_captures_widget_clip_but_not_damage_clip() {
        use crate::draw_command::DrawCommand;
        use crate::font::FontCache;
        use crate::picture::PictureRecorder;

        let mut canvas = SkiaCanvas::new(100, 100);
        // Damage clip active (partial repaint) — must NOT leak into quads.
        canvas.set_logical_clip(Some(rect(0.0, 0.0, 5.0, 5.0)));

        let mut rec = PictureRecorder::new();
        rec.push(DrawCommand::PushClip { rect: rect(10.0, 10.0, 50.0, 50.0) });
        rec.push(DrawCommand::PushClip { rect: rect(30.0, 30.0, 50.0, 50.0) });
        rec.push(DrawCommand::ShaderFill {
            pipeline_id: 0x300,
            rect: rect(0.0, 0.0, 100.0, 100.0),
            uniforms: vec![],
        });
        rec.push(DrawCommand::PopClip);
        rec.push(DrawCommand::PopClip);
        rec.push(DrawCommand::ShaderFill {
            pipeline_id: 0x301,
            rect: rect(0.0, 0.0, 10.0, 10.0),
            uniforms: vec![],
        });
        canvas.play_picture(&rec.finish(), &FontCache::embedded());
        canvas.set_logical_clip(None);

        let quads = canvas.take_shader_quads();
        assert_eq!(quads.len(), 2);
        // Nested clips intersect: (10..60) ∩ (30..80) = (30, 30, 30, 30).
        assert_eq!(quads[0].clip, Some((30.0, 30.0, 30.0, 30.0)));
        // Outside all PushClips: no widget clip, damage clip ignored.
        assert_eq!(quads[1].clip, None);
    }

    #[test]
    fn image_handle_from_valid_png() {
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
