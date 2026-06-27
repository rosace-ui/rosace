//! Built-in light and dark themes (Material Design 3 inspired).

use crate::color::{Color, ColorScheme};
use crate::radius::BorderRadius;
use crate::spacing::Spacing;
use crate::theme::ThemeData;
use crate::typography::Typography;

/// Constructs the built-in light theme.
///
/// Primary: `#6750A4` (MD3 purple).
pub fn light_theme() -> ThemeData {
    ThemeData {
        colors: ColorScheme {
            primary:               Color::from_hex(0x6750A4),
            on_primary:            Color::from_hex(0xFFFFFF),
            primary_container:     Color::from_hex(0xEADDFF),
            on_primary_container:  Color::from_hex(0x21005D),
            secondary:             Color::from_hex(0x625B71),
            on_secondary:          Color::from_hex(0xFFFFFF),
            surface:               Color::from_hex(0xFFFBFE),
            on_surface:            Color::from_hex(0x1C1B1F),
            surface_variant:       Color::from_hex(0xE7E0EC),
            background:            Color::from_hex(0xFFFBFE),
            on_background:         Color::from_hex(0x1C1B1F),
            error:                 Color::from_hex(0xB3261E),
            on_error:              Color::from_hex(0xFFFFFF),
            outline:               Color::from_hex(0x79747E),
            shadow:                Color::BLACK,
        },
        typography: Typography::default(),
        spacing: Spacing::default(),
        radius: BorderRadius::default(),
        is_dark: false,
    }
}

/// Constructs the built-in dark theme.
///
/// Primary: `#D0BCFF` (MD3 purple, dark variant).
pub fn dark_theme() -> ThemeData {
    ThemeData {
        colors: ColorScheme {
            primary:               Color::from_hex(0xD0BCFF),
            on_primary:            Color::from_hex(0x381E72),
            primary_container:     Color::from_hex(0x4F378B),
            on_primary_container:  Color::from_hex(0xEADDFF),
            secondary:             Color::from_hex(0xCCC2DC),
            on_secondary:          Color::from_hex(0x332D41),
            surface:               Color::from_hex(0x1C1B1F),
            on_surface:            Color::from_hex(0xE6E1E5),
            surface_variant:       Color::from_hex(0x49454F),
            background:            Color::from_hex(0x1C1B1F),
            on_background:         Color::from_hex(0xE6E1E5),
            error:                 Color::from_hex(0xF2B8B5),
            on_error:              Color::from_hex(0x601410),
            outline:               Color::from_hex(0x938F99),
            shadow:                Color::BLACK,
        },
        typography: Typography::default(),
        spacing: Spacing::default(),
        radius: BorderRadius::default(),
        is_dark: true,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_theme_is_not_dark() {
        let theme = light_theme();
        assert!(!theme.is_dark, "light theme should have is_dark = false");
    }

    #[test]
    fn dark_theme_is_dark() {
        let theme = dark_theme();
        assert!(theme.is_dark, "dark theme should have is_dark = true");
    }

    #[test]
    fn light_theme_primary_is_md3_purple() {
        let theme = light_theme();
        let expected = Color::from_hex(0x6750A4);
        assert!((theme.colors.primary.r - expected.r).abs() < 1e-5);
        assert!((theme.colors.primary.g - expected.g).abs() < 1e-5);
        assert!((theme.colors.primary.b - expected.b).abs() < 1e-5);
    }

    #[test]
    fn dark_theme_primary_is_light_purple() {
        let theme = dark_theme();
        let expected = Color::from_hex(0xD0BCFF);
        assert!((theme.colors.primary.r - expected.r).abs() < 1e-5);
        assert!((theme.colors.primary.g - expected.g).abs() < 1e-5);
        assert!((theme.colors.primary.b - expected.b).abs() < 1e-5);
    }

    #[test]
    fn built_in_themes_have_default_spacing() {
        let s = Spacing::default();
        let light = light_theme();
        assert_eq!(light.spacing.md, s.md);
        let dark = dark_theme();
        assert_eq!(dark.spacing.md, s.md);
    }

    #[test]
    fn built_in_themes_have_default_radius() {
        let r = BorderRadius::default();
        let light = light_theme();
        assert_eq!(light.radius.md, r.md);
    }
}
