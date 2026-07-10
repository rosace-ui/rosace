use rosace_theme::Color;
use crate::direction::TextDirection;

/// Style flags for a text span.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub direction: TextDirection,
}

impl TextStyle {
    pub fn new(font_size: f32, color: Color) -> Self {
        Self { font_size, color, bold: false, italic: false, underline: false, direction: TextDirection::Auto }
    }
    pub fn bold(mut self) -> Self { self.bold = true; self }
    pub fn italic(mut self) -> Self { self.italic = true; self }
    pub fn underline(mut self) -> Self { self.underline = true; self }
}

impl Default for TextStyle {
    fn default() -> Self { Self::new(14.0, Color::WHITE) }
}

/// A styled run of text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub style: TextStyle,
}

impl TextSpan {
    pub fn new(text: impl Into<String>, style: TextStyle) -> Self {
        Self { text: text.into(), style }
    }

    /// Estimated pixel width using monospace approximation (font_size * 0.55 per char).
    pub fn estimated_width(&self) -> f32 {
        self.text.len() as f32 * self.style.font_size * 0.55
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_style_new_defaults() {
        let style = TextStyle::new(16.0, Color::WHITE);
        assert_eq!(style.font_size, 16.0);
        assert!(!style.bold);
        assert!(!style.italic);
        assert!(!style.underline);
    }

    #[test]
    fn text_style_bold() {
        let style = TextStyle::new(14.0, Color::WHITE).bold();
        assert!(style.bold);
        assert!(!style.italic);
        assert!(!style.underline);
    }

    #[test]
    fn text_style_underline() {
        let style = TextStyle::new(14.0, Color::WHITE).underline();
        assert!(style.underline);
        assert!(!style.bold);
    }

    #[test]
    fn text_span_new() {
        let style = TextStyle::default();
        let span = TextSpan::new("hello", style.clone());
        assert_eq!(span.text, "hello");
        assert_eq!(span.style, style);
    }

    #[test]
    fn text_span_estimated_width_scales_with_size() {
        let small = TextSpan::new("A", TextStyle::new(10.0, Color::WHITE));
        let large = TextSpan::new("A", TextStyle::new(20.0, Color::WHITE));
        assert!((large.estimated_width() - 2.0 * small.estimated_width()).abs() < 1e-5);
    }

    #[test]
    fn text_span_estimated_width_scales_with_length() {
        let one = TextSpan::new("A", TextStyle::new(14.0, Color::WHITE));
        let three = TextSpan::new("ABC", TextStyle::new(14.0, Color::WHITE));
        assert!((three.estimated_width() - 3.0 * one.estimated_width()).abs() < 1e-5);
    }
}
