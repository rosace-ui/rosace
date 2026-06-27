//! [`Text`] — a plain-text leaf widget.

use tezzera_core::{element::Element, types::{Point, Size}};
use tezzera_render::canvas::{Color, SkiaCanvas};
use tezzera_render::FontCache;

/// A widget that displays a string of text.
///
/// Supports both the Phase 1 placeholder rendering (`paint`) and the Phase 2
/// real glyph rendering (`render` with a [`FontCache`]).
pub struct Text {
    pub content: String,
    pub color: Color,
    /// Font size in pixels (used by [`Text::render`]).
    pub size: f32,
    /// Optional maximum width for wrapping or clamping.
    pub max_width: Option<f32>,
}

impl Text {
    /// Creates a new [`Text`] widget with the given content.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            color: Color::rgb(240, 242, 255),
            size: 14.0,
            max_width: None,
        }
    }

    /// Sets the color used to render the text.
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }

    /// Sets the font size in pixels.
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }

    /// Clamps the rendered text to the given maximum width.
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = Some(w);
        self
    }

    /// Returns the approximate natural (Phase 1) size: 8 px per character wide, 16 px tall.
    ///
    /// Kept for backward compatibility with Phase 1 tests.
    pub fn natural_size(&self) -> Size {
        Size { width: self.content.len() as f32 * 8.0, height: 16.0 }
    }

    /// Returns the preferred (width, height) using Phase 2 font-size estimates.
    ///
    /// Approximates monospace at 0.6 × size per character.
    pub fn preferred_size(&self) -> (f32, f32) {
        let w = self.content.len() as f32 * self.size * 0.6;
        let clamped = self.max_width.map(|mw| mw.min(w)).unwrap_or(w);
        (clamped, self.size * 1.4)
    }

    /// Phase 1: paints a colored rectangle placeholder at `origin`.
    pub fn paint(&self, canvas: &mut SkiaCanvas, origin: Point) {
        canvas.draw_text_placeholder(&self.content, origin, self.color);
    }

    /// Phase 2: renders real glyphs at `(x, y)` using `font`.
    pub fn render(&self, canvas: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
        canvas.draw_text(&self.content, Point { x, y }, self.color, font, self.size);
    }
}

impl From<Text> for Element {
    fn from(t: Text) -> Element {
        Element::text(t.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_new_sets_content() {
        let t = Text::new("hello");
        assert_eq!(t.content, "hello");
    }

    #[test]
    fn text_preferred_size_width_positive() {
        let t = Text::new("hello");
        let (w, _h) = t.preferred_size();
        assert!(w > 0.0);
    }

    #[test]
    fn text_preferred_size_respects_max_width() {
        let t = Text::new("hello world this is long").max_width(50.0);
        let (w, _) = t.preferred_size();
        assert!(w <= 50.0);
    }

    #[test]
    fn text_natural_size_scales_with_content() {
        let short = Text::new("Hi").natural_size();
        let long = Text::new("Hello World").natural_size();
        assert!(long.width > short.width);
    }
}
