use crate::span::{TextSpan, TextStyle};
use rosace_theme::Color;

/// A paragraph of styled text spans.
#[derive(Debug, Clone, Default)]
pub struct RichText {
    pub spans: Vec<TextSpan>,
}

impl RichText {
    pub fn new() -> Self { Self { spans: Vec::new() } }

    pub fn push(mut self, text: impl Into<String>, style: TextStyle) -> Self {
        self.spans.push(TextSpan::new(text, style));
        self
    }

    pub fn text(mut self, text: impl Into<String>, size: f32, color: Color) -> Self {
        self.spans.push(TextSpan::new(text, TextStyle::new(size, color)));
        self
    }

    pub fn bold(mut self, text: impl Into<String>, size: f32, color: Color) -> Self {
        self.spans.push(TextSpan::new(text, TextStyle::new(size, color).bold()));
        self
    }

    /// Concatenated plain text (no styles).
    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Total estimated pixel width (sum of all spans).
    pub fn total_width(&self) -> f32 {
        self.spans.iter().map(|s| s.estimated_width()).sum()
    }

    pub fn is_empty(&self) -> bool { self.spans.is_empty() }
    pub fn len(&self) -> usize { self.spans.len() }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_text_new_empty() {
        let rt = RichText::new();
        assert!(rt.is_empty());
        assert_eq!(rt.len(), 0);
    }

    #[test]
    fn rich_text_push_span() {
        let style = TextStyle::new(14.0, Color::WHITE);
        let rt = RichText::new().push("hello", style);
        assert_eq!(rt.len(), 1);
        assert!(!rt.is_empty());
    }

    #[test]
    fn rich_text_text_builder() {
        let rt = RichText::new().text("Hello", 16.0, Color::WHITE);
        assert_eq!(rt.len(), 1);
        assert_eq!(rt.spans[0].text, "Hello");
        assert_eq!(rt.spans[0].style.font_size, 16.0);
    }

    #[test]
    fn rich_text_bold_builder() {
        let rt = RichText::new().bold("World", 16.0, Color::WHITE);
        assert_eq!(rt.len(), 1);
        assert!(rt.spans[0].style.bold);
    }

    #[test]
    fn rich_text_plain_text() {
        let rt = RichText::new()
            .text("Hello ", 14.0, Color::WHITE)
            .text("world", 14.0, Color::WHITE);
        assert_eq!(rt.plain_text(), "Hello world");
    }

    #[test]
    fn rich_text_total_width() {
        let rt = RichText::new()
            .text("A", 10.0, Color::WHITE)  // 1 * 10 * 0.55 = 5.5
            .text("B", 10.0, Color::WHITE);  // 1 * 10 * 0.55 = 5.5
        let expected = 2.0 * 10.0 * 0.55;
        assert!((rt.total_width() - expected).abs() < 1e-5);
    }
}
