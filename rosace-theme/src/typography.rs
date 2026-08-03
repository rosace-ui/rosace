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

/// The complete Material Design 3–inspired type scale, bumped +1px across
/// every step (2026-08-03, user-requested: the MD3 baseline still read
/// small in practice) — the whole point of routing widgets through this
/// ONE scale (D127) instead of their own literals is that a tweak like
/// this now takes effect everywhere at once, with zero widget-file edits.
#[derive(Debug, Clone)]
pub struct Typography {
    pub display_large: TextStyle,   // 58 px
    pub display_medium: TextStyle,  // 46 px
    pub display_small: TextStyle,   // 37 px
    pub headline_large: TextStyle,  // 33 px
    pub headline_medium: TextStyle, // 29 px
    pub headline_small: TextStyle,  // 25 px
    pub title_large: TextStyle,     // 23 px
    pub title_medium: TextStyle,    // 17 px, medium weight
    pub title_small: TextStyle,     // 15 px, medium weight
    pub body_large: TextStyle,      // 17 px
    pub body_medium: TextStyle,     // 15 px
    pub body_small: TextStyle,      // 13 px
    pub label_large: TextStyle,     // 15 px, medium
    pub label_medium: TextStyle,    // 13 px, medium
    pub label_small: TextStyle,     // 12 px, medium
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            display_large:   TextStyle::new(58.0, FontWeight::Regular),
            display_medium:  TextStyle::new(46.0, FontWeight::Regular),
            display_small:   TextStyle::new(37.0, FontWeight::Regular),
            headline_large:  TextStyle::new(33.0, FontWeight::Regular),
            headline_medium: TextStyle::new(29.0, FontWeight::Regular),
            headline_small:  TextStyle::new(25.0, FontWeight::Regular),
            title_large:     TextStyle::new(23.0, FontWeight::Regular),
            title_medium:    TextStyle::new(17.0, FontWeight::Medium),
            title_small:     TextStyle::new(15.0, FontWeight::Medium),
            body_large:      TextStyle::new(17.0, FontWeight::Regular),
            body_medium:     TextStyle::new(15.0, FontWeight::Regular),
            body_small:      TextStyle::new(13.0, FontWeight::Regular),
            label_large:     TextStyle::new(15.0, FontWeight::Medium),
            label_medium:    TextStyle::new(13.0, FontWeight::Medium),
            label_small:     TextStyle::new(12.0, FontWeight::Medium),
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
    fn typography_default_display_large_is_58px() {
        let t = Typography::default();
        assert_eq!(t.display_large.size, 58.0);
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
