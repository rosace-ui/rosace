/// Unicode Bidirectional Character Type (TR#9 §3.3, abbreviated set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidiClass {
    /// Left-to-Right (e.g. Latin letters)
    L,
    /// Right-to-Left (e.g. Hebrew letters)
    R,
    /// Arabic Letter
    AL,
    /// European Number
    EN,
    /// Arabic Number
    AN,
    /// Paragraph Separator
    B,
    /// Segment Separator
    S,
    /// Whitespace
    WS,
    /// Other Neutral
    ON,
    /// Non-Spacing Mark
    NSM,
}

/// Classify a Unicode character into its bidi class.
///
/// Covers the most common ranges. Falls back to `BidiClass::ON` (Other Neutral)
/// for anything not explicitly classified.
pub fn bidi_class(ch: char) -> BidiClass {
    let cp = ch as u32;
    match cp {
        // Strong Left-to-Right: ASCII letters and common Latin extended
        0x0041..=0x005A => BidiClass::L,  // A-Z
        0x0061..=0x007A => BidiClass::L,  // a-z
        0x00C0..=0x00D6 => BidiClass::L,  // Latin Extended-A
        0x00D8..=0x00F6 => BidiClass::L,
        0x00F8..=0x02B8 => BidiClass::L,
        0x0370..=0x0482 => BidiClass::L,  // Greek
        0x048A..=0x058F => BidiClass::L,  // Cyrillic, Armenian
        0x0900..=0x0939 => BidiClass::L,  // Devanagari
        0x0966..=0x096F => BidiClass::EN, // Devanagari digits
        0x0E00..=0x0E7F => BidiClass::L,  // Thai
        0x1E00..=0x1EFF => BidiClass::L,  // Latin Extended Additional
        0x2C00..=0x2C5F => BidiClass::L,  // Glagolitic

        // Hebrew (Right-to-Left)
        0x05BE           => BidiClass::R,
        0x05C0           => BidiClass::R,
        0x05C3           => BidiClass::R,
        0x05C6           => BidiClass::R,
        0x05D0..=0x05EA  => BidiClass::R,
        0x05F0..=0x05F4  => BidiClass::R,
        0x07C0..=0x07EA  => BidiClass::R,  // NKo
        0x07F4..=0x07F5  => BidiClass::R,
        0x07FA           => BidiClass::R,
        0xFB1D           => BidiClass::R,
        0xFB1F..=0xFB28  => BidiClass::R,
        0xFB2A..=0xFB36  => BidiClass::R,
        0xFB38..=0xFB3C  => BidiClass::R,
        0xFB40..=0xFB41  => BidiClass::R,
        0xFB43..=0xFB44  => BidiClass::R,
        0xFB46..=0xFB4F  => BidiClass::R,

        // Arabic (Arabic Letter — RTL, different from Hebrew R)
        0x0600..=0x060B  => BidiClass::AL,
        0x060D..=0x061A  => BidiClass::AL,
        0x061C           => BidiClass::AL,
        0x061E..=0x064A  => BidiClass::AL,
        0x0660..=0x0669  => BidiClass::AN, // Arabic-Indic digits
        0x066B..=0x066C  => BidiClass::AN,
        0x066D..=0x066F  => BidiClass::AL,
        0x0671..=0x06D3  => BidiClass::AL,
        0x06D5           => BidiClass::AL,
        0x06E5..=0x06E6  => BidiClass::AL,
        0x06EE..=0x06EF  => BidiClass::AL,
        0x06FA..=0x070D  => BidiClass::AL,
        0x0750..=0x077F  => BidiClass::AL,
        0x08A0..=0x08FF  => BidiClass::AL,
        0xFB50..=0xFDCF  => BidiClass::AL,
        0xFDF0..=0xFDFD  => BidiClass::AL,
        0xFE70..=0xFEFF  => BidiClass::AL,

        // European Numbers (ASCII digits)
        0x0030..=0x0039  => BidiClass::EN,
        0x00B2..=0x00B3  => BidiClass::EN,
        0x00B9           => BidiClass::EN,
        0x06F0..=0x06F9  => BidiClass::EN,

        // Whitespace / Separators
        0x0009           => BidiClass::S,   // Tab (Segment Separator)
        0x000A           => BidiClass::B,   // LF (Paragraph Separator)
        0x000D           => BidiClass::B,   // CR
        0x0020           => BidiClass::WS,  // Space
        0x00A0           => BidiClass::WS,  // NBSP — map to WS for our subset
        0x2000..=0x200A  => BidiClass::WS,
        0x2028           => BidiClass::B,
        0x2029           => BidiClass::B,
        0x205F           => BidiClass::WS,
        0x3000           => BidiClass::WS,

        // Non-Spacing Marks
        0x0300..=0x036F  => BidiClass::NSM,
        0x0483..=0x0489  => BidiClass::NSM,
        0x064B..=0x065F  => BidiClass::NSM,
        0x0670           => BidiClass::NSM,
        0x06D6..=0x06DC  => BidiClass::NSM,

        // Everything else is Other Neutral
        _ => BidiClass::ON,
    }
}

impl BidiClass {
    pub fn is_strong_ltr(&self) -> bool { *self == BidiClass::L }
    pub fn is_strong_rtl(&self) -> bool { matches!(self, BidiClass::R | BidiClass::AL) }
    pub fn is_rtl(&self) -> bool { matches!(self, BidiClass::R | BidiClass::AL | BidiClass::AN) }
    pub fn is_neutral(&self) -> bool { matches!(self, BidiClass::WS | BidiClass::ON | BidiClass::S) }
    pub fn is_number(&self) -> bool { matches!(self, BidiClass::EN | BidiClass::AN) }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bidi_class_latin_upper() {
        assert_eq!(bidi_class('A'), BidiClass::L);
    }

    #[test]
    fn bidi_class_latin_lower() {
        assert_eq!(bidi_class('z'), BidiClass::L);
    }

    #[test]
    fn bidi_class_digit() {
        assert_eq!(bidi_class('5'), BidiClass::EN);
    }

    #[test]
    fn bidi_class_arabic_letter() {
        // U+0639 ARABIC LETTER AIN
        assert_eq!(bidi_class('\u{0639}'), BidiClass::AL);
    }

    #[test]
    fn bidi_class_hebrew_letter() {
        // U+05D0 HEBREW LETTER ALEF
        assert_eq!(bidi_class('\u{05D0}'), BidiClass::R);
    }

    #[test]
    fn bidi_class_space() {
        assert_eq!(bidi_class(' '), BidiClass::WS);
    }

    #[test]
    fn bidi_class_newline() {
        assert_eq!(bidi_class('\n'), BidiClass::B);
    }

    #[test]
    fn bidi_class_tab() {
        assert_eq!(bidi_class('\t'), BidiClass::S);
    }

    #[test]
    fn bidi_class_is_strong_ltr() {
        assert!(bidi_class('A').is_strong_ltr());
        assert!(!bidi_class('\u{05D0}').is_strong_ltr());
    }

    #[test]
    fn bidi_class_is_strong_rtl_r() {
        assert!(bidi_class('\u{05D0}').is_strong_rtl()); // Hebrew R
        assert!(!bidi_class('A').is_strong_rtl());
    }

    #[test]
    fn bidi_class_is_strong_rtl_al() {
        assert!(bidi_class('\u{0639}').is_strong_rtl()); // Arabic AL
    }

    #[test]
    fn bidi_class_is_neutral() {
        assert!(bidi_class(' ').is_neutral());  // WS
        assert!(bidi_class('\t').is_neutral()); // S
        assert!(!bidi_class('A').is_neutral());
    }

    #[test]
    fn bidi_class_is_number_en() {
        assert!(bidi_class('5').is_number());
    }

    #[test]
    fn bidi_class_is_number_an() {
        // U+0660 ARABIC-INDIC DIGIT ZERO
        assert!(bidi_class('\u{0660}').is_number());
    }
}
