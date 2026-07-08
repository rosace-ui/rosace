//! `tezzera-theme` — design-token system for the TEZZERA UI framework.
//!
//! # Overview
//!
//! This crate provides the complete theming layer:
//!
//! - [`Color`] / [`ColorScheme`] — f32 RGBA colors and semantic color roles.
//! - [`Typography`] — Material Design 3 type scale.
//! - [`Spacing`] — named spacing tokens.
//! - [`BorderRadius`] — named corner-radius tokens.
//! - [`Shadows`] — named elevation/shadow tokens.
//! - [`ThemeData`] — all tokens in one struct.
//! - [`TezzeraTheme`] — trait for custom themes.
//! - [`built_in`] — [`light_theme()`] and [`dark_theme()`] factory functions.
//! - [`use_theme()`] / [`set_theme()`] — global theme access via a reactive atom.
//!
//! [`light_theme()`]: built_in::light_theme
//! [`dark_theme()`]: built_in::dark_theme

pub mod built_in;
pub mod color;
pub mod provider;
pub mod radius;
pub mod shadow;
pub mod spacing;
pub mod theme;
pub mod themes;
pub mod typography;

// Flat re-exports for convenience.
pub use color::{Color, ColorScheme};
pub use provider::{set_theme, use_theme, set_animations};
pub use radius::BorderRadius;
pub use shadow::{ShadowLayer, Shadows};
pub use spacing::Spacing;
pub use theme::{AnimationConfig, ThemeData, TezzeraTheme, AppBarStyle, TitleAlign};
pub use themes::Themes;
pub use typography::{FontFamily, FontWeight, TextStyle, Typography};

// ---------------------------------------------------------------------------
// Integration-level tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_lerp_integration() {
        let red   = Color::rgb(1.0, 0.0, 0.0);
        let blue  = Color::rgb(0.0, 0.0, 1.0);
        let mid   = red.lerp(blue, 0.5);
        assert!((mid.r - 0.5).abs() < 1e-6);
        assert!((mid.b - 0.5).abs() < 1e-6);
    }

    #[test]
    fn color_from_hex_integration() {
        let c = Color::from_hex(0xFF0000); // pure red
        assert!((c.r - 1.0).abs() < 1e-5);
        assert!(c.g.abs() < 1e-5);
        assert!(c.b.abs() < 1e-5);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn spacing_defaults_integration() {
        let s = Spacing::default();
        assert_eq!(s.xs, 4.0);
        assert_eq!(s.sm, 8.0);
    }

    #[test]
    fn border_radius_defaults_integration() {
        let r = BorderRadius::default();
        assert_eq!(r.none, 0.0);
        assert_eq!(r.full, 9999.0);
    }

    #[test]
    fn light_theme_is_dark_false() {
        let theme = built_in::light_theme();
        assert!(!theme.is_dark);
    }

    #[test]
    fn dark_theme_is_dark_true() {
        let theme = built_in::dark_theme();
        assert!(theme.is_dark);
    }

    #[test]
    fn typography_scale_order() {
        let typo = Typography::default();
        assert!(typo.display_large.size > typo.headline_large.size);
        assert!(typo.headline_large.size > typo.body_large.size);
    }

    #[test]
    fn use_theme_returns_valid_theme_integration() {
        let theme = use_theme();
        assert!(theme.spacing.md > 0.0);
        assert!(theme.radius.md >= 0.0);
        // Typography sanity check
        assert!(theme.typography.display_large.size > 0.0);
    }
}
