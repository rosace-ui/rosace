use tezzera_core::types::{Point, Rect};
use tezzera_render::{Color, FontCache};

/// Backend-agnostic drawing surface.
///
/// This trait is the v1.0 swap point (D032): widgets call `Renderer` methods,
/// so switching from tiny-skia to skia-safe only requires a new impl of this
/// trait — no widget code changes.
pub trait Renderer {
    /// Clear the entire surface with `color`.
    fn clear(&mut self, color: Color);

    /// Fill a rectangle.
    fn fill_rect(&mut self, rect: Rect, color: Color);

    /// Stroke a rectangle outline.
    fn stroke_rect(&mut self, rect: Rect, color: Color, width: f32);

    /// Fill a circle at `center` with `radius`.
    fn fill_circle(&mut self, center: Point, radius: f32, color: Color);

    /// Draw text at `pos`. `font` provides glyph rasterization.
    fn draw_text(&mut self, text: &str, pos: Point, color: Color, font: &FontCache, size: f32);

    /// Encode the current surface as a PNG byte vector.
    fn encode_png(&self) -> Vec<u8>;

    /// Surface width in pixels.
    fn width(&self) -> u32;

    /// Surface height in pixels.
    fn height(&self) -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tezzera_core::types::{Point, Rect, Size};
    use tezzera_render::Color;
    use crate::skia::SkiaRenderer;

    fn make_renderer(w: u32, h: u32) -> SkiaRenderer {
        SkiaRenderer::new(w, h)
    }

    #[test]
    fn renderer_clear_changes_pixel() {
        let mut r = make_renderer(10, 10);
        r.clear(Color::RED);
        let pixels = r.canvas().pixels();
        assert_eq!(pixels[0], 255); // R
        assert_eq!(pixels[1], 0);   // G
        assert_eq!(pixels[2], 0);   // B
        assert_eq!(pixels[3], 255); // A
    }

    #[test]
    fn renderer_fill_rect_fills_pixels() {
        let mut r = make_renderer(100, 100);
        r.clear(Color::WHITE);
        r.fill_rect(
            Rect {
                origin: Point { x: 0.0, y: 0.0 },
                size: Size { width: 10.0, height: 10.0 },
            },
            Color::BLUE,
        );
        let pixels = r.canvas().pixels();
        assert_eq!(pixels[2], 255); // B channel of first pixel
    }

    #[test]
    fn renderer_width_matches_constructor() {
        let r = make_renderer(320, 240);
        assert_eq!(r.width(), 320);
    }

    #[test]
    fn renderer_height_matches_constructor() {
        let r = make_renderer(320, 240);
        assert_eq!(r.height(), 240);
    }

    #[test]
    fn renderer_encode_png_is_valid_png_header() {
        let mut r = make_renderer(10, 10);
        r.clear(Color::BLACK);
        let png = r.encode_png();
        assert!(png.len() >= 4);
        assert_eq!(&png[0..4], b"\x89PNG");
    }

    #[test]
    fn renderer_trait_object_fill_rect() {
        let mut r: Box<dyn Renderer> = Box::new(make_renderer(100, 100));
        r.clear(Color::WHITE);
        r.fill_rect(
            Rect {
                origin: Point { x: 5.0, y: 5.0 },
                size: Size { width: 20.0, height: 20.0 },
            },
            Color::GREEN,
        );
        // No panic is sufficient for trait object usage.
    }

    #[test]
    fn renderer_trait_object_fill_circle() {
        let mut r: Box<dyn Renderer> = Box::new(make_renderer(100, 100));
        r.clear(Color::WHITE);
        r.fill_circle(Point { x: 50.0, y: 50.0 }, 20.0, Color::RED);
        // No panic is sufficient.
    }

    #[test]
    fn renderer_trait_object_draw_text() {
        let mut r: Box<dyn Renderer> = Box::new(make_renderer(200, 100));
        r.clear(Color::WHITE);
        // Use system font if available; fall back gracefully.
        if let Some(font) = tezzera_render::FontCache::system_mono() {
            r.draw_text("Hello", Point { x: 10.0, y: 40.0 }, Color::BLACK, &font, 16.0);
        }
        // No panic regardless of font availability.
    }

    #[test]
    fn renderer_trait_object_encode_png() {
        let mut r: Box<dyn Renderer> = Box::new(make_renderer(10, 10));
        r.clear(Color::BLACK);
        let png = r.encode_png();
        assert!(!png.is_empty());
        assert_eq!(&png[0..4], b"\x89PNG");
    }

    #[test]
    fn renderer_backend_round_trip() {
        use crate::backend::RendererBackend;
        let r = make_renderer(10, 10);
        let b = r.backend();
        assert_eq!(b, RendererBackend::TinySkia);
        assert!(b.is_tiny_skia());
        assert!(!b.is_skia_safe());
    }
}
