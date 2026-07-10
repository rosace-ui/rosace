/// Text flow direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDirection {
    /// Left-to-right (Latin, default).
    Ltr,
    /// Right-to-left (Arabic, Hebrew).
    Rtl,
    /// Detect from first strong bidi character.
    #[default]
    Auto,
}

/// Detect text direction by scanning for the first strongly-directional Unicode character.
///
/// Returns `Rtl` if any character falls in Arabic (U+0600–U+06FF),
/// Hebrew (U+0590–U+05FF), or Arabic Extended-A (U+08A0–U+08FF) ranges.
/// Returns `Ltr` otherwise (including empty strings).
pub fn detect_direction(text: &str) -> TextDirection {
    for ch in text.chars() {
        let cp = ch as u32;
        if (0x0590..=0x05FF).contains(&cp)   // Hebrew
        || (0x0600..=0x06FF).contains(&cp)   // Arabic
        || (0x0750..=0x077F).contains(&cp)   // Arabic Supplement
        || (0x08A0..=0x08FF).contains(&cp)   // Arabic Extended-A
        || (0xFB1D..=0xFB4F).contains(&cp)   // Hebrew Presentation Forms
        || (0xFB50..=0xFDFF).contains(&cp)   // Arabic Presentation Forms-A
        || (0xFE70..=0xFEFF).contains(&cp)   // Arabic Presentation Forms-B
        {
            return TextDirection::Rtl;
        }
    }
    TextDirection::Ltr
}

/// Reverse word order for RTL display (visual reordering stub).
pub fn reverse_words(text: &str) -> String {
    text.split_whitespace()
        .rev()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::TextStyle;
    use crate::direction::TextDirection as SpanDir;

    #[test]
    fn direction_ltr_for_latin() {
        assert_eq!(detect_direction("Hello world"), TextDirection::Ltr);
    }

    #[test]
    fn direction_ltr_for_empty() {
        assert_eq!(detect_direction(""), TextDirection::Ltr);
    }

    #[test]
    fn direction_rtl_for_arabic() {
        // U+0627 ARABIC LETTER ALEF
        assert_eq!(detect_direction("\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}"), TextDirection::Rtl);
    }

    #[test]
    fn direction_rtl_for_hebrew() {
        // U+05E9 HEBREW LETTER SHIN
        assert_eq!(detect_direction("\u{05E9}\u{05DC}\u{05D5}\u{05DD}"), TextDirection::Rtl);
    }

    #[test]
    fn direction_auto_default() {
        let d: TextDirection = TextDirection::default();
        assert_eq!(d, TextDirection::Auto);
    }

    #[test]
    fn direction_detects_mixed_as_rtl_when_first_strong_is_rtl() {
        // Arabic character appears before Latin
        let text = "\u{0627}hello";
        assert_eq!(detect_direction(text), TextDirection::Rtl);
    }

    #[test]
    fn direction_ltr_for_numbers() {
        assert_eq!(detect_direction("12345"), TextDirection::Ltr);
    }

    #[test]
    fn reverse_words_two_words() {
        assert_eq!(reverse_words("hello world"), "world hello");
    }

    #[test]
    fn reverse_words_single_word() {
        assert_eq!(reverse_words("hello"), "hello");
    }

    #[test]
    fn reverse_words_empty() {
        assert_eq!(reverse_words(""), "");
    }

    #[test]
    fn reverse_words_three_words() {
        assert_eq!(reverse_words("one two three"), "three two one");
    }

    #[test]
    fn text_style_has_direction_field() {
        use rosace_theme::Color;
        let style = TextStyle::new(14.0, Color::WHITE);
        assert_eq!(style.direction, SpanDir::Auto);
    }
}
