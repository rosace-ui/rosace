/// Unicode script category (simplified subset for shaping engine selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Script {
    #[default]
    Latin,
    Arabic,
    Hebrew,
    Devanagari,
    Han,
    Greek,
    Cyrillic,
    Unknown,
}

impl Script {
    /// Detect script from the first alphabetic character in `text`.
    pub fn detect(text: &str) -> Self {
        for ch in text.chars() {
            let cp = ch as u32;
            match cp {
                0x0041..=0x007A | 0x00C0..=0x024F => return Script::Latin,
                0x0370..=0x03FF                    => return Script::Greek,
                0x0400..=0x04FF                    => return Script::Cyrillic,
                0x0590..=0x05FF | 0xFB1D..=0xFB4F  => return Script::Hebrew,
                0x0600..=0x06FF | 0x0750..=0x077F
                | 0x08A0..=0x08FF | 0xFB50..=0xFDFF
                | 0xFE70..=0xFEFF                  => return Script::Arabic,
                0x0900..=0x097F                    => return Script::Devanagari,
                0x4E00..=0x9FFF | 0x3400..=0x4DBF  => return Script::Han,
                _                                  => {}
            }
        }
        Script::Unknown
    }

    pub fn is_rtl(&self) -> bool {
        matches!(self, Script::Arabic | Script::Hebrew)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Script::Latin      => "Latin",
            Script::Arabic     => "Arabic",
            Script::Hebrew     => "Hebrew",
            Script::Devanagari => "Devanagari",
            Script::Han        => "Han (CJK)",
            Script::Greek      => "Greek",
            Script::Cyrillic   => "Cyrillic",
            Script::Unknown    => "Unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_detect_latin() {
        assert_eq!(Script::detect("Hello"), Script::Latin);
    }

    #[test]
    fn script_detect_arabic() {
        // U+0627 ARABIC LETTER ALEF
        assert_eq!(Script::detect("\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}"), Script::Arabic);
    }

    #[test]
    fn script_detect_hebrew() {
        // U+05E9 HEBREW LETTER SHIN
        assert_eq!(Script::detect("\u{05E9}\u{05DC}\u{05D5}\u{05DD}"), Script::Hebrew);
    }

    #[test]
    fn script_detect_greek() {
        // U+03B1 GREEK SMALL LETTER ALPHA
        assert_eq!(Script::detect("\u{03B1}\u{03B2}\u{03B3}"), Script::Greek);
    }

    #[test]
    fn script_detect_cyrillic() {
        // U+0410 CYRILLIC CAPITAL LETTER A
        assert_eq!(Script::detect("\u{0410}\u{0411}"), Script::Cyrillic);
    }

    #[test]
    fn script_detect_unknown_for_digits() {
        assert_eq!(Script::detect("12345"), Script::Unknown);
    }

    #[test]
    fn script_is_rtl_arabic() {
        assert!(Script::Arabic.is_rtl());
        assert!(Script::Hebrew.is_rtl());
        assert!(!Script::Latin.is_rtl());
    }

    #[test]
    fn script_name() {
        assert_eq!(Script::Latin.name(), "Latin");
        assert_eq!(Script::Arabic.name(), "Arabic");
        assert_eq!(Script::Unknown.name(), "Unknown");
        assert_eq!(Script::Han.name(), "Han (CJK)");
    }
}
