use crate::class::{bidi_class, BidiClass};

/// Detect the paragraph embedding level (TR#9 §3.3 P2/P3).
///
/// Scans for the first strongly-directional character:
///
/// - L → level 0 (LTR)
/// - R or AL → level 1 (RTL)
///
/// Returns 0 if no strong character is found.
pub fn paragraph_level(text: &str) -> u8 {
    for ch in text.chars() {
        match bidi_class(ch) {
            BidiClass::L => return 0,
            BidiClass::R | BidiClass::AL => return 1,
            _ => {}
        }
    }
    0
}

/// Assign per-character embedding levels (simplified TR#9 X/W rules).
///
/// This is a simplified implementation: strong LTR chars get the base
/// level, strong RTL chars get level 1, others inherit from context.
/// Returns a Vec<u8> parallel to the input's `chars()`.
pub fn resolve_levels(text: &str) -> Vec<u8> {
    let base = paragraph_level(text);
    let mut levels = Vec::new();

    for ch in text.chars() {
        let class = bidi_class(ch);
        let level = match class {
            BidiClass::L                  => 0,
            BidiClass::R | BidiClass::AL  => 1,
            BidiClass::AN                 => if base == 1 { 1 } else { 2 },
            BidiClass::EN                 => if base == 1 { 2 } else { 0 },
            BidiClass::NSM                => *levels.last().unwrap_or(&base),
            _                             => base,
        };
        levels.push(level);
    }

    levels
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_level_ltr_empty() {
        assert_eq!(paragraph_level(""), 0);
    }

    #[test]
    fn paragraph_level_ltr_latin() {
        assert_eq!(paragraph_level("Hello"), 0);
    }

    #[test]
    fn paragraph_level_rtl_arabic() {
        // U+0645 U+0631 U+062D U+0628 U+0627 — مرحبا
        assert_eq!(paragraph_level("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}"), 1);
    }

    #[test]
    fn paragraph_level_rtl_hebrew() {
        // U+05E9 U+05DC U+05D5 U+05DD — שלום
        assert_eq!(paragraph_level("\u{05E9}\u{05DC}\u{05D5}\u{05DD}"), 1);
    }

    #[test]
    fn paragraph_level_ltr_digit_first() {
        // Digits are EN (not strong directional), so scan continues to 'H'
        assert_eq!(paragraph_level("123 Hello"), 0);
    }

    #[test]
    fn paragraph_level_rtl_mixed_starts_rtl() {
        // Arabic first → level 1
        assert_eq!(paragraph_level("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} Hello"), 1);
    }

    #[test]
    fn resolve_levels_all_ltr() {
        let levels = resolve_levels("Hello");
        assert!(levels.iter().all(|&l| l == 0));
    }

    #[test]
    fn resolve_levels_all_rtl() {
        let text = "\u{05E9}\u{05DC}\u{05D5}\u{05DD}"; // שלום
        let levels = resolve_levels(text);
        assert!(levels.iter().all(|&l| l == 1));
    }

    #[test]
    fn resolve_levels_length_matches_chars() {
        let text = "Hello \u{0645}\u{0631}\u{062D}\u{0628}\u{0627}";
        let levels = resolve_levels(text);
        assert_eq!(levels.len(), text.chars().count());
    }

    #[test]
    fn resolve_levels_nsm_inherits() {
        // U+0301 is a combining acute accent (NSM) — should inherit from previous char.
        // "A\u{0301}" — A is L (level 0), NSM should inherit 0.
        let levels = resolve_levels("A\u{0301}");
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0], 0); // 'A' → L
        assert_eq!(levels[1], 0); // NSM inherits from 'A'
    }
}
