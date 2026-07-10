use rosace_render::{Color, FontCache, SkiaCanvas};
use rosace_core::types::{Point, Rect};

/// Lightweight headless render environment for widget tests.
///
/// Wraps a `SkiaCanvas` and an optional `FontCache`. Use `pixel_at` to read
/// individual pixels after rendering and `encode_png` to dump the buffer.
pub struct WidgetEnv {
    canvas: SkiaCanvas,
    font: Option<FontCache>,
}

impl WidgetEnv {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            canvas: SkiaCanvas::new(width, height),
            font: FontCache::system_mono(),
        }
    }

    /// Width of the canvas in pixels.
    pub fn width(&self) -> u32 { self.canvas.width() }

    /// Height of the canvas in pixels.
    pub fn height(&self) -> u32 { self.canvas.height() }

    /// Fill the canvas with `color`.
    pub fn clear(&mut self, color: Color) {
        self.canvas.clear(color);
    }

    /// Fill a rectangle.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.canvas.fill_rect(rect, color);
    }

    /// Render text at `(x, y)` with the given pixel size.
    /// Falls back to `draw_text_placeholder` when no system font is available.
    pub fn render_text(&mut self, text: &str, x: f32, y: f32, size: f32) {
        let origin = Point { x, y };
        if let Some(ref font) = self.font {
            self.canvas.draw_text(text, origin, Color::BLACK, font, size);
        } else {
            self.canvas.draw_text_placeholder(text, origin, Color::BLACK);
        }
    }

    /// Render text with an explicit color.
    pub fn render_text_colored(&mut self, text: &str, x: f32, y: f32, size: f32, color: Color) {
        let origin = Point { x, y };
        if let Some(ref font) = self.font {
            self.canvas.draw_text(text, origin, color, font, size);
        } else {
            self.canvas.draw_text_placeholder(text, origin, color);
        }
    }

    /// Encode the canvas as PNG bytes. Returns empty vec on failure.
    pub fn encode_png(&self) -> Vec<u8> {
        self.canvas.encode_png().unwrap_or_default()
    }

    /// Read the RGBA color of a single pixel.
    /// Returns `Color::TRANSPARENT` when `(x, y)` is out of bounds.
    pub fn pixel_at(&self, x: u32, y: u32) -> Color {
        let w = self.canvas.width();
        let h = self.canvas.height();
        if x >= w || y >= h {
            return Color::TRANSPARENT;
        }
        let pixels = self.canvas.pixels();
        // tiny-skia stores pixels as premultiplied RGBA, 4 bytes per pixel
        let idx = ((y * w + x) * 4) as usize;
        if idx + 3 >= pixels.len() {
            return Color::TRANSPARENT;
        }
        Color::rgba(pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3])
    }

    /// Expose the underlying `SkiaCanvas` for advanced drawing.
    pub fn canvas_mut(&mut self) -> &mut SkiaCanvas { &mut self.canvas }
    pub fn canvas(&self) -> &SkiaCanvas { &self.canvas }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_new_dimensions() {
        let env = WidgetEnv::new(100, 80);
        assert_eq!(env.width(), 100);
        assert_eq!(env.height(), 80);
    }

    #[test]
    fn env_clear_sets_pixel() {
        let mut env = WidgetEnv::new(10, 10);
        env.clear(Color::rgb(255, 0, 0));
        let px = env.pixel_at(5, 5);
        assert_eq!(px.r, 255);
        assert_eq!(px.g, 0);
        assert_eq!(px.b, 0);
    }

    #[test]
    fn env_pixel_at_out_of_bounds() {
        let env = WidgetEnv::new(10, 10);
        let px = env.pixel_at(100, 100);
        assert_eq!(px.a, 0); // TRANSPARENT
    }

    #[test]
    fn env_encode_png_non_empty() {
        let mut env = WidgetEnv::new(4, 4);
        env.clear(Color::WHITE);
        let png = env.encode_png();
        assert!(!png.is_empty());
        // PNG magic bytes: \x89PNG
        assert_eq!(&png[0..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn env_fill_rect_sets_pixel() {
        use rosace_core::types::{Point, Rect, Size};
        let mut env = WidgetEnv::new(20, 20);
        env.clear(Color::WHITE);
        let rect = Rect { origin: Point { x: 2.0, y: 2.0 }, size: Size { width: 10.0, height: 10.0 } };
        env.fill_rect(rect, Color::rgb(0, 0, 255));
        let px = env.pixel_at(5, 5);
        assert_eq!(px.b, 255);
    }

    #[test]
    fn env_render_text_does_not_panic() {
        let mut env = WidgetEnv::new(200, 50);
        env.clear(Color::WHITE);
        env.render_text("Hello", 10.0, 30.0, 16.0); // must not panic
    }
}
