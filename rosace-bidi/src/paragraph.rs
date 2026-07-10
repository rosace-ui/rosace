use crate::level::{paragraph_level, resolve_levels};
use crate::reorder::reorder_line;

/// A fully processed bidirectional paragraph.
#[derive(Debug, Clone)]
pub struct BidiParagraph {
    pub text: String,
    pub levels: Vec<u8>,
    pub base_level: u8,
    /// Visually reordered text.
    pub visual: String,
}

impl BidiParagraph {
    /// Run the full bidi pipeline: classify → resolve → reorder.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let base_level = paragraph_level(&text);
        let levels = resolve_levels(&text);
        let visual = reorder_line(&text, &levels);
        Self { text, levels, base_level, visual }
    }

    pub fn is_rtl(&self) -> bool { self.base_level == 1 }
    pub fn is_ltr(&self) -> bool { self.base_level == 0 }

    /// Embedding level of the character at char index `i`.
    pub fn char_level(&self, i: usize) -> Option<u8> {
        self.levels.get(i).copied()
    }

    /// Count of characters at RTL level (level >= 1).
    pub fn rtl_char_count(&self) -> usize {
        self.levels.iter().filter(|&&l| l >= 1).count()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bidi_paragraph_new_ltr() {
        let p = BidiParagraph::new("Hello");
        assert_eq!(p.base_level, 0);
    }

    #[test]
    fn bidi_paragraph_new_rtl() {
        // U+05E9 U+05DC U+05D5 U+05DD — שלום
        let p = BidiParagraph::new("\u{05E9}\u{05DC}\u{05D5}\u{05DD}");
        assert_eq!(p.base_level, 1);
    }

    #[test]
    fn bidi_paragraph_is_rtl() {
        let p = BidiParagraph::new("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}"); // مرحبا
        assert!(p.is_rtl());
        assert!(!p.is_ltr());
    }

    #[test]
    fn bidi_paragraph_is_ltr() {
        let p = BidiParagraph::new("Hello World");
        assert!(p.is_ltr());
        assert!(!p.is_rtl());
    }

    #[test]
    fn bidi_paragraph_char_level() {
        let p = BidiParagraph::new("Hello");
        // All Latin → level 0
        assert_eq!(p.char_level(0), Some(0));
        assert_eq!(p.char_level(4), Some(0));
        assert_eq!(p.char_level(10), None);
    }

    #[test]
    fn bidi_paragraph_rtl_char_count() {
        // Pure RTL
        let text = "\u{05E9}\u{05DC}\u{05D5}\u{05DD}"; // שלום (4 chars)
        let p = BidiParagraph::new(text);
        assert_eq!(p.rtl_char_count(), 4);
    }

    #[test]
    fn bidi_paragraph_levels_length() {
        let text = "Hello \u{0645}\u{0631}\u{062D}";
        let p = BidiParagraph::new(text);
        assert_eq!(p.levels.len(), text.chars().count());
    }

    #[test]
    fn bidi_paragraph_visual_not_empty() {
        let p = BidiParagraph::new("Hello");
        assert!(!p.visual.is_empty());
    }
}
