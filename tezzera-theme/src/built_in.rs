//! Built-in light and dark themes.
//!
//! Replaced the original Material Design 3 palette (2026-07-08) — MD3's
//! purple-tinted surfaces (`#FFFBFE`/`#1C1B1F`) read as dated next to native
//! macOS/JetBrains-tool UI conventions. Dark is Darcula-inspired (neutral
//! charcoal panels, soft off-white text, a lavender-violet accent —
//! matching JetBrains' actual Darcula/New UI accent family, not MD3's
//! purple-on-navy); light is a genuinely neutral white/near-black pair
//! instead of MD3's warm-purple-tinted "white", with a deeper violet accent
//! for contrast. (Accent was blue at first landing, swapped to violet same
//! day per direct user feedback — "much better [dark contrast], but the
//! color choices are terrible" re: the blue.)

use crate::color::{Color, ColorScheme};
use crate::radius::BorderRadius;
use crate::spacing::Spacing;
use crate::theme::ThemeData;
use crate::typography::Typography;

/// Constructs the built-in light theme.
///
/// Primary: `#7C4DFF` (deep vivid violet). Neutral near-white surfaces and
/// near-black text — no warm/MD3 tint.
pub fn light_theme() -> ThemeData {
    ThemeData {
        animation: crate::AnimationConfig::default(),
        colors: ColorScheme {
            primary:               Color::from_hex(0x7C4DFF),
            on_primary:            Color::from_hex(0xFFFFFF),
            primary_container:     Color::from_hex(0xE9DDFF),
            on_primary_container:  Color::from_hex(0x2B0B54),
            secondary:             Color::from_hex(0x5B5F66),
            on_secondary:          Color::from_hex(0xFFFFFF),
            surface:               Color::from_hex(0xFFFFFF),
            on_surface:            Color::from_hex(0x1E1F22),
            surface_variant:       Color::from_hex(0xEFEFF0),
            background:            Color::from_hex(0xFAFAFB),
            on_background:         Color::from_hex(0x1E1F22),
            error:                 Color::from_hex(0xD5433D),
            on_error:              Color::from_hex(0xFFFFFF),
            outline:               Color::from_hex(0xC6C7CA),
            shadow:                Color::BLACK,
        },
        typography: Typography::default(),
        spacing: Spacing::default(),
        radius: BorderRadius::default(),
        is_dark: false,
        app_bar: crate::theme::AppBarStyle::default(),
        ext: std::collections::HashMap::new(),
    }
}

/// Constructs the built-in dark theme — the framework default (`App::new()`
/// starts dark; see `tezzera/src/lib.rs`).
///
/// Primary: `#BB86FC` (soft lavender-violet, Darcula/JetBrains-accent
/// family). Neutral charcoal panels (`#2B2D30`/`#1E1F22`, not MD3's
/// purple-black `#1C1B1F`) with soft off-white text (`#DFE1E5`, not stark
/// `#FFFFFF`) — easier on the eyes and closer to how native dark-mode dev
/// tools actually look.
pub fn dark_theme() -> ThemeData {
    ThemeData {
        animation: crate::AnimationConfig::default(),
        colors: ColorScheme {
            primary:               Color::from_hex(0xBB86FC),
            on_primary:            Color::from_hex(0x2B0B54),
            primary_container:     Color::from_hex(0x4A2E7A),
            on_primary_container:  Color::from_hex(0xEADDFF),
            secondary:             Color::from_hex(0x8C8F93),
            on_secondary:          Color::from_hex(0x1E1F22),
            surface:               Color::from_hex(0x2B2D30),
            on_surface:            Color::from_hex(0xDFE1E5),
            surface_variant:       Color::from_hex(0x393B40),
            background:            Color::from_hex(0x1E1F22),
            on_background:         Color::from_hex(0xDFE1E5),
            error:                 Color::from_hex(0xFF6B68),
            on_error:              Color::from_hex(0x1E1F22),
            outline:               Color::from_hex(0x4E5157),
            shadow:                Color::BLACK,
        },
        typography: Typography::default(),
        spacing: Spacing::default(),
        radius: BorderRadius::default(),
        is_dark: true,
        app_bar: crate::theme::AppBarStyle::default(),
        ext: std::collections::HashMap::new(),
    }
}

/// A Material (Android-flavored) theme (D105 Phase 23 Step 5): built on
/// [`light_theme`]'s MD3-purple tokens (already Material-appropriate), with
/// an AppBar left-title, 56dp-tall, elevated look — Android Material
/// app-bar conventions.
pub fn material() -> ThemeData {
    ThemeData {
        app_bar: crate::theme::AppBarStyle {
            title_align: crate::theme::TitleAlign::Leading,
            show_traffic_lights: false,
            height: 56.0,
            elevation: 4.0,
        },
        ..light_theme()
    }
}

/// A Cupertino (iOS-flavored) theme (D105 Phase 23 Step 5): an AppBar with a
/// centered title, 44pt-tall, flat (hairline-only) look — iOS navigation-bar
/// conventions — plus iOS system-blue accents in place of Material's purple,
/// since reusing [`light_theme`]'s MD3 palette verbatim would look
/// Android-branded on an iOS device. Every other token (typography, spacing,
/// radius) still comes from [`light_theme`] — only the accent color and the
/// AppBar structure are iOS-specific.
pub fn cupertino() -> ThemeData {
    ThemeData {
        colors: ColorScheme {
            primary:              Color::from_hex(0x007AFF),
            on_primary:           Color::from_hex(0xFFFFFF),
            primary_container:    Color::from_hex(0xD6E8FF),
            on_primary_container: Color::from_hex(0x00265A),
            ..light_theme().colors
        },
        app_bar: crate::theme::AppBarStyle {
            title_align: crate::theme::TitleAlign::Center,
            show_traffic_lights: false,
            height: 44.0,
            elevation: 0.0,
        },
        ..light_theme()
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
    fn light_theme_primary_is_deep_violet() {
        let theme = light_theme();
        let expected = Color::from_hex(0x7C4DFF);
        assert!((theme.colors.primary.r - expected.r).abs() < 1e-5);
        assert!((theme.colors.primary.g - expected.g).abs() < 1e-5);
        assert!((theme.colors.primary.b - expected.b).abs() < 1e-5);
    }

    #[test]
    fn dark_theme_primary_is_lavender_violet() {
        let theme = dark_theme();
        let expected = Color::from_hex(0xBB86FC);
        assert!((theme.colors.primary.r - expected.r).abs() < 1e-5);
        assert!((theme.colors.primary.g - expected.g).abs() < 1e-5);
        assert!((theme.colors.primary.b - expected.b).abs() < 1e-5);
    }

    #[test]
    fn dark_theme_surface_is_neutral_charcoal_not_purple_tinted() {
        // The old MD3 palette's dark surface (#1C1B1F) has a purple tint;
        // Darcula-style neutral gray is the whole point of this palette.
        let theme = dark_theme();
        let surface = theme.colors.surface;
        assert!((surface.r - surface.g).abs() < 0.02, "r/g should be near-equal (neutral gray)");
        assert!((surface.g - surface.b).abs() < 0.02, "g/b should be near-equal (neutral gray)");
    }

    #[test]
    fn light_theme_on_surface_is_not_pure_black() {
        // Near-black (#1E1F22), not #000000 — softer, matches native dark
        // text conventions instead of harsh pure black.
        let theme = light_theme();
        assert_ne!(theme.colors.on_surface, Color::BLACK);
        assert!(theme.colors.on_surface.r > 0.05);
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

    #[test]
    fn material_and_cupertino_are_structurally_distinct() {
        let m = material();
        let c = cupertino();
        assert_eq!(m.app_bar.title_align, crate::theme::TitleAlign::Leading);
        assert_eq!(c.app_bar.title_align, crate::theme::TitleAlign::Center);
        assert_eq!(m.app_bar.height, 56.0);
        assert_eq!(c.app_bar.height, 44.0);
    }

    #[test]
    fn cupertino_uses_ios_system_blue_not_the_light_theme_default() {
        let expected = Color::from_hex(0x007AFF);
        let c = cupertino();
        assert!((c.colors.primary.r - expected.r).abs() < 1e-5);
        assert!((c.colors.primary.g - expected.g).abs() < 1e-5);
        assert!((c.colors.primary.b - expected.b).abs() < 1e-5);
        // Material keeps light_theme()'s default blue already asserted by
        // `light_theme_primary_is_modern_blue` (material() is built on it).
        assert_ne!(c.colors.primary, material().colors.primary);
    }
}
