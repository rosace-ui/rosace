//! `tezzera-render` — software rasterizer and display-list recording for TEZZERA.
//!
//! Provides [`SkiaCanvas`] (backed by `tiny-skia`), [`PictureRecorder`] for
//! recording draw commands during the paint pass, and [`Picture`] for replaying
//! them. The [`FontCache`] handles glyph rasterization.

pub mod canvas;
pub mod draw_command;
pub mod font;
pub mod image;
pub mod picture;

pub use canvas::{Color, SkiaCanvas};
pub use draw_command::DrawCommand;
pub use font::{FontCache, FontWeight};
pub use image::{CachePolicy, ImageFit, ImageHandle};
pub use picture::{Picture, PictureRecorder};

#[cfg(test)]
mod tests {
    use tezzera_core::types::{Point, Rect, Size};

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
