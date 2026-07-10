//! Typography tokens: font families, weights, and the type scale.

/// The font family to use for a text style.
#[derive(Debug, Clone, PartialEq)]
pub enum FontFamily {
    /// The operating system's default UI font.
    System,
    /// The operating system's default monospaced font.
    Monospace,
    /// A custom font loaded by name.
    Custom(String),
}

/// Numeric font weight following the CSS/OpenType convention.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontWeight {
    Thin = 100,
    Light = 300,
    Regular = 400,
    Medium = 500,
    SemiBold = 600,
    Bold = 700,
    ExtraBold = 800,
    Black = 900,
}

/// A complete description of how a piece of text should be rendered.
#[derive(Debug, Clone)]
pub struct TextStyle {
    pub family: FontFamily,
    /// Font size in logical pixels.
    pub size: f32,
    pub weight: FontWeight,
    /// Line height as a multiplier of `size`. Defaults to 1.4.
    pub line_height: f32,
    /// Additional letter spacing in logical pixels. Defaults to 0.0.
    pub letter_spacing: f32,
}

impl TextStyle {
    /// Builds a `TextStyle` with system font, regular weight, and default
    /// line-height / letter-spacing.
    pub fn new(size: f32, weight: FontWeight) -> Self {
        Self {
            family: FontFamily::System,
            size,
            weight,
            line_height: 1.4,
            letter_spacing: 0.0,
        }
    }
}

/// The complete Material Design 3–inspired type scale.
#[derive(Debug, Clone)]
pub struct Typography {
    pub display_large: TextStyle,   // 57 px
    pub display_medium: TextStyle,  // 45 px
    pub display_small: TextStyle,   // 36 px
    pub headline_large: TextStyle,  // 32 px
    pub headline_medium: TextStyle, // 28 px
    pub headline_small: TextStyle,  // 24 px
    pub title_large: TextStyle,     // 22 px
    pub title_medium: TextStyle,    // 16 px, medium weight
    pub title_small: TextStyle,     // 14 px, medium weight
    pub body_large: TextStyle,      // 16 px
    pub body_medium: TextStyle,     // 14 px
    pub body_small: TextStyle,      // 12 px
    pub label_large: TextStyle,     // 14 px, medium
    pub label_medium: TextStyle,    // 12 px, medium
    pub label_small: TextStyle,     // 11 px, medium
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            display_large:   TextStyle::new(57.0, FontWeight::Regular),
            display_medium:  TextStyle::new(45.0, FontWeight::Regular),
            display_small:   TextStyle::new(36.0, FontWeight::Regular),
            headline_large:  TextStyle::new(32.0, FontWeight::Regular),
            headline_medium: TextStyle::new(28.0, FontWeight::Regular),
            headline_small:  TextStyle::new(24.0, FontWeight::Regular),
            title_large:     TextStyle::new(22.0, FontWeight::Regular),
            title_medium:    TextStyle::new(16.0, FontWeight::Medium),
            title_small:     TextStyle::new(14.0, FontWeight::Medium),
            body_large:      TextStyle::new(16.0, FontWeight::Regular),
            body_medium:     TextStyle::new(14.0, FontWeight::Regular),
            body_small:      TextStyle::new(12.0, FontWeight::Regular),
            label_large:     TextStyle::new(14.0, FontWeight::Medium),
            label_medium:    TextStyle::new(12.0, FontWeight::Medium),
            label_small:     TextStyle::new(11.0, FontWeight::Medium),
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
    fn typography_scale_sizes_are_ordered() {
        let t = Typography::default();
        assert!(t.display_large.size > t.headline_large.size,
            "display_large ({}) should be larger than headline_large ({})",
            t.display_large.size, t.headline_large.size);
        assert!(t.headline_large.size > t.body_large.size,
            "headline_large ({}) should be larger than body_large ({})",
            t.headline_large.size, t.body_large.size);
    }

    #[test]
    fn typography_default_display_large_is_57px() {
        let t = Typography::default();
        assert_eq!(t.display_large.size, 57.0);
    }

    #[test]
    fn typography_title_medium_is_medium_weight() {
        let t = Typography::default();
        assert_eq!(t.title_medium.weight, FontWeight::Medium);
    }

    #[test]
    fn typography_default_line_height() {
        let t = Typography::default();
        assert!((t.body_large.line_height - 1.4).abs() < 1e-6);
    }

    #[test]
    fn font_family_custom_stores_name() {
        let f = FontFamily::Custom("Inter".to_string());
        assert_eq!(f, FontFamily::Custom("Inter".to_string()));
        assert_ne!(f, FontFamily::System);
    }
}
