use tezzera_core::types::{Point, Rect};
use tezzera_render::{Color, FontCache, SkiaCanvas};
use crate::backend::RendererBackend;
use crate::renderer::Renderer;

/// `Renderer` implementation backed by `SkiaCanvas` (tiny-skia).
///
/// This is the current production renderer. To swap in v1.0, add a
/// `SkiaSafeRenderer` that also implements `Renderer`, and update
/// `SkiaRenderer::backend()` to return `RendererBackend::SkiaSafe`.
pub struct SkiaRenderer {
    canvas: SkiaCanvas,
}

impl SkiaRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            canvas: SkiaCanvas::new(width, height),
        }
    }

    /// The active backend for this renderer.
    pub fn backend(&self) -> RendererBackend {
        RendererBackend::TinySkia
    }

    /// Borrow the inner canvas (e.g. for pixel-level operations).
    pub fn canvas(&self) -> &SkiaCanvas {
        &self.canvas
    }

    /// Mutably borrow the inner canvas.
    pub fn canvas_mut(&mut self) -> &mut SkiaCanvas {
        &mut self.canvas
    }
}

impl Renderer for SkiaRenderer {
    fn clear(&mut self, color: Color) {
        self.canvas.clear(color);
    }

    fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.canvas.fill_rect(rect, color);
    }

    fn stroke_rect(&mut self, rect: Rect, color: Color, width: f32) {
        self.canvas.stroke_rect(rect, color, width);
    }

    fn fill_circle(&mut self, center: Point, radius: f32, color: Color) {
        self.canvas.fill_circle(center, radius, color);
    }

    fn draw_text(&mut self, text: &str, pos: Point, color: Color, font: &FontCache, size: f32) {
        self.canvas.draw_text(text, pos, color, font, size);
    }

    fn encode_png(&self) -> Vec<u8> {
        self.canvas.encode_png().unwrap_or_default()
    }

    fn width(&self) -> u32 {
        self.canvas.width()
    }

    fn height(&self) -> u32 {
        self.canvas.height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tezzera_core::types::{Point, Rect, Size};
    use tezzera_render::Color;
    use crate::renderer::Renderer;

    #[test]
    fn skia_renderer_new() {
        let r = SkiaRenderer::new(100, 200);
        assert_eq!(r.width(), 100);
        assert_eq!(r.height(), 200);
    }

    #[test]
    fn skia_renderer_backend_is_tiny_skia() {
        let r = SkiaRenderer::new(10, 10);
        assert_eq!(r.backend(), RendererBackend::TinySkia);
    }

    #[test]
    fn skia_renderer_width() {
        let r = SkiaRenderer::new(640, 480);
        assert_eq!(r.width(), 640);
    }

    #[test]
    fn skia_renderer_height() {
        let r = SkiaRenderer::new(640, 480);
        assert_eq!(r.height(), 480);
    }

    #[test]
    fn skia_renderer_clear_no_panic() {
        let mut r = SkiaRenderer::new(50, 50);
        r.clear(Color::BLUE);
        let pixels = r.canvas().pixels();
        assert_eq!(pixels[2], 255); // B channel
    }

    #[test]
    fn skia_renderer_fill_rect_no_panic() {
        let mut r = SkiaRenderer::new(100, 100);
        r.clear(Color::WHITE);
        r.fill_rect(
            Rect {
                origin: Point { x: 10.0, y: 10.0 },
                size: Size { width: 30.0, height: 30.0 },
            },
            Color::RED,
        );
        // No panic is the requirement.
    }

    #[test]
    fn skia_renderer_stroke_rect_no_panic() {
        let mut r = SkiaRenderer::new(100, 100);
        r.clear(Color::WHITE);
        r.stroke_rect(
            Rect {
                origin: Point { x: 5.0, y: 5.0 },
                size: Size { width: 40.0, height: 40.0 },
            },
            Color::BLACK,
            2.0,
        );
        // No panic is the requirement.
    }

    #[test]
    fn skia_renderer_fill_circle_no_panic() {
        let mut r = SkiaRenderer::new(100, 100);
        r.clear(Color::WHITE);
        r.fill_circle(Point { x: 50.0, y: 50.0 }, 25.0, Color::GREEN);
        // No panic is the requirement.
    }

    #[test]
    fn skia_renderer_draw_text_no_panic() {
        let mut r = SkiaRenderer::new(200, 100);
        r.clear(Color::WHITE);
        if let Some(font) = FontCache::system_mono() {
            r.draw_text("Test", Point { x: 10.0, y: 50.0 }, Color::BLACK, &font, 14.0);
        }
        // No panic regardless of font availability.
    }

    #[test]
    fn skia_renderer_encode_png_non_empty() {
        let mut r = SkiaRenderer::new(10, 10);
        r.clear(Color::WHITE);
        let png = r.encode_png();
        assert!(!png.is_empty());
        // Verify PNG magic bytes.
        assert_eq!(&png[0..4], b"\x89PNG");
    }

    #[test]
    fn skia_renderer_canvas_borrow() {
        let mut r = SkiaRenderer::new(10, 10);
        r.clear(Color::RED);
        let pixels = r.canvas().pixels();
        assert_eq!(pixels[0], 255); // R
        assert_eq!(pixels[1], 0);   // G
        assert_eq!(pixels[2], 0);   // B

        r.canvas_mut().clear(Color::BLUE);
        let pixels2 = r.canvas().pixels();
        assert_eq!(pixels2[2], 255); // B channel
    }

    #[test]
    fn skia_renderer_via_trait_object() {
        let mut r: Box<dyn Renderer> = Box::new(SkiaRenderer::new(50, 50));
        r.clear(Color::GREEN);
        assert_eq!(r.width(), 50);
        assert_eq!(r.height(), 50);
        let png = r.encode_png();
        assert!(!png.is_empty());
    }
}
