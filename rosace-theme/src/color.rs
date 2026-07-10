//! Color primitives: `Color` (f32 RGBA), `ColorScheme`, and `Palette`.

/// A color value with red, green, blue, and alpha channels, each in `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Creates an opaque color from RGB components in `[0.0, 1.0]`.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Creates a color from RGBA components in `[0.0, 1.0]`.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Creates an opaque color from a 24-bit hex value (`0xRRGGBB`).
    pub fn from_hex(hex: u32) -> Self {
        let r = ((hex >> 16) & 0xFF) as f32 / 255.0;
        let g = ((hex >> 8) & 0xFF) as f32 / 255.0;
        let b = (hex & 0xFF) as f32 / 255.0;
        Self::rgb(r, g, b)
    }

    /// Returns a copy of this color with the alpha channel replaced by `a`.
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Linearly interpolates between `self` and `other` by factor `t` (0 → self, 1 → other).
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// Fully opaque white.
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    /// Fully opaque black.
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
}

/// The semantic color roles used throughout a theme (Material Design 3 inspired).
#[derive(Debug, Clone)]
pub struct ColorScheme {
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub surface_variant: Color,
    pub background: Color,
    pub on_background: Color,
    pub error: Color,
    pub on_error: Color,
    pub outline: Color,
    pub shadow: Color,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_rgb_sets_alpha_to_one() {
        let c = Color::rgb(0.5, 0.5, 0.5);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn color_from_hex_parses_correctly() {
        // 0x6750A4 → r=103, g=80, b=164
        let c = Color::from_hex(0x6750A4);
        let expected_r = 103.0_f32 / 255.0;
        let expected_g = 80.0_f32 / 255.0;
        let expected_b = 164.0_f32 / 255.0;
        assert!((c.r - expected_r).abs() < 1e-5, "r mismatch: {}", c.r);
        assert!((c.g - expected_g).abs() < 1e-5, "g mismatch: {}", c.g);
        assert!((c.b - expected_b).abs() < 1e-5, "b mismatch: {}", c.b);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn color_from_hex_white() {
        let c = Color::from_hex(0xFFFFFF);
        assert!((c.r - 1.0).abs() < 1e-5);
        assert!((c.g - 1.0).abs() < 1e-5);
        assert!((c.b - 1.0).abs() < 1e-5);
    }

    #[test]
    fn color_lerp_midpoint() {
        let a = Color::rgb(0.0, 0.0, 0.0);
        let b = Color::rgb(1.0, 1.0, 1.0);
        let mid = a.lerp(b, 0.5);
        assert!((mid.r - 0.5).abs() < 1e-6);
        assert!((mid.g - 0.5).abs() < 1e-6);
        assert!((mid.b - 0.5).abs() < 1e-6);
        assert!((mid.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn color_lerp_t0_returns_self() {
        let a = Color::rgb(0.2, 0.4, 0.6);
        let b = Color::rgb(1.0, 1.0, 1.0);
        let result = a.lerp(b, 0.0);
        assert!((result.r - a.r).abs() < 1e-6);
        assert!((result.g - a.g).abs() < 1e-6);
        assert!((result.b - a.b).abs() < 1e-6);
    }

    #[test]
    fn color_lerp_t1_returns_other() {
        let a = Color::rgb(0.0, 0.0, 0.0);
        let b = Color::rgb(0.3, 0.6, 0.9);
        let result = a.lerp(b, 1.0);
        assert!((result.r - b.r).abs() < 1e-6);
        assert!((result.g - b.g).abs() < 1e-6);
        assert!((result.b - b.b).abs() < 1e-6);
    }

    #[test]
    fn color_with_alpha() {
        let c = Color::WHITE.with_alpha(0.5);
        assert_eq!(c.r, 1.0);
        assert_eq!(c.g, 1.0);
        assert_eq!(c.b, 1.0);
        assert!((c.a - 0.5).abs() < 1e-6);
    }

    #[test]
    fn color_constants() {
        assert_eq!(Color::WHITE.a, 1.0);
        assert_eq!(Color::BLACK.r, 0.0);
        assert_eq!(Color::TRANSPARENT.a, 0.0);
    }
}
