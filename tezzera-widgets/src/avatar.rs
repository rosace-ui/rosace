//! [`Avatar`] — a circular avatar showing initials.

use tezzera_core::types::Point;
use tezzera_render::canvas::SkiaCanvas;
use tezzera_render::FontCache;
use tezzera_theme::ThemeData;
use crate::theme_color_to_render as tc;

/// A circular avatar that displays up to two initials.
pub struct Avatar {
    /// Initials to display (only the first two characters are rendered).
    pub initials: String,
    /// Diameter of the avatar circle in pixels.
    pub size: f32,
    /// Optional background color override; falls back to the theme primary color.
    pub color: Option<tezzera_theme::Color>,
}

impl Avatar {
    /// Creates a new [`Avatar`] with the given initials at default size (40 px).
    pub fn new(initials: impl Into<String>) -> Self {
        Self {
            initials: initials.into(),
            size: 40.0,
            color: None,
        }
    }

    /// Sets the diameter of the avatar circle in pixels.
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }

    /// Overrides the background fill color.
    pub fn color(mut self, c: tezzera_theme::Color) -> Self {
        self.color = Some(c);
        self
    }

    /// Paints the avatar onto `canvas` at `(x, y)` (top-left corner).
    pub fn render(
        &self,
        canvas: &mut SkiaCanvas,
        font: &FontCache,
        x: f32,
        y: f32,
        theme: &ThemeData,
    ) {
        let r = self.size / 2.0;
        let cx = x + r;
        let cy = y + r;

        let bg = self.color.map(tc).unwrap_or_else(|| tc(theme.colors.primary));
        canvas.fill_circle(Point { x: cx, y: cy }, r, bg);

        // Render at most 2 characters
        let char_count = self.initials.chars().count().min(2);
        let text: String = self.initials.chars().take(char_count).collect();
        let tw = text.len() as f32 * self.size * 0.28;
        canvas.draw_text(
            &text,
            Point { x: cx - tw / 2.0, y: cy - self.size * 0.18 },
            tc(theme.colors.on_primary),
            font,
            self.size * 0.38,
        );
    }

    /// Returns the pixel width of the avatar.
    pub fn width(&self) -> f32 {
        self.size
    }

    /// Returns the pixel height of the avatar.
    pub fn height(&self) -> f32 {
        self.size
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_initials_stored() {
        let a = Avatar::new("AB");
        assert_eq!(a.initials, "AB");
    }

    #[test]
    fn avatar_initials_truncate_at_2() {
        let a = Avatar::new("ABCDE");
        // The render takes .chars().take(2), verify via width calculation
        let char_count = a.initials.chars().count().min(2);
        assert_eq!(char_count, 2);
    }

    #[test]
    fn avatar_single_initial_is_fine() {
        let a = Avatar::new("J");
        let char_count = a.initials.chars().count().min(2);
        assert_eq!(char_count, 1);
    }

    #[test]
    fn avatar_size_setter() {
        let a = Avatar::new("AB").size(60.0);
        assert_eq!(a.size, 60.0);
        assert_eq!(a.width(), 60.0);
        assert_eq!(a.height(), 60.0);
    }

    #[test]
    fn avatar_default_size_is_40() {
        let a = Avatar::new("AB");
        assert_eq!(a.size, 40.0);
    }

    #[test]
    fn avatar_color_override() {
        let color = tezzera_theme::Color::from_hex(0xFF5733);
        let a = Avatar::new("AB").color(color);
        assert!(a.color.is_some());
        let c = a.color.unwrap();
        assert!((c.r - color.r).abs() < 1e-6);
    }

    #[test]
    fn avatar_no_color_defaults_to_none() {
        let a = Avatar::new("AB");
        assert!(a.color.is_none());
    }

    #[test]
    fn avatar_width_equals_height_equals_size() {
        let a = Avatar::new("XY").size(50.0);
        assert_eq!(a.width(), a.height());
        assert_eq!(a.width(), a.size);
    }
}
