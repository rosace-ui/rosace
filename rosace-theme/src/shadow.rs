//! Shadow tokens — elevation levels expressed as named shadow descriptors.

use crate::color::Color;

/// A single drop-shadow descriptor.
#[derive(Debug, Clone, Copy)]
pub struct ShadowLayer {
    /// Horizontal offset in logical pixels.
    pub offset_x: f32,
    /// Vertical offset in logical pixels.
    pub offset_y: f32,
    /// Blur radius in logical pixels.
    pub blur: f32,
    /// Spread radius in logical pixels.
    pub spread: f32,
    /// Shadow color (includes opacity via alpha channel).
    pub color: Color,
}

impl ShadowLayer {
    /// A shadow layer with no visible effect.
    pub const NONE: Self = Self {
        offset_x: 0.0,
        offset_y: 0.0,
        blur: 0.0,
        spread: 0.0,
        color: Color::TRANSPARENT,
    };
}

/// Named elevation levels, each expressed as a `ShadowLayer`.
#[derive(Debug, Clone, Copy)]
pub struct Shadows {
    /// No shadow (elevation 0).
    pub none: ShadowLayer,
    /// Subtle shadow (elevation 1).
    pub sm: ShadowLayer,
    /// Medium shadow (elevation 2).
    pub md: ShadowLayer,
    /// Pronounced shadow (elevation 3).
    pub lg: ShadowLayer,
}

impl Default for Shadows {
    fn default() -> Self {
        let shadow_color = Color::BLACK.with_alpha(0.15);
        Self {
            none: ShadowLayer::NONE,
            sm: ShadowLayer {
                offset_x: 0.0,
                offset_y: 1.0,
                blur: 3.0,
                spread: 0.0,
                color: shadow_color,
            },
            md: ShadowLayer {
                offset_x: 0.0,
                offset_y: 2.0,
                blur: 6.0,
                spread: 0.0,
                color: shadow_color,
            },
            lg: ShadowLayer {
                offset_x: 0.0,
                offset_y: 4.0,
                blur: 12.0,
                spread: 0.0,
                color: shadow_color,
            },
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
    fn shadow_none_is_transparent() {
        let s = Shadows::default();
        assert_eq!(s.none.color.a, 0.0);
        assert_eq!(s.none.blur, 0.0);
    }

    #[test]
    fn shadow_blur_is_ascending() {
        let s = Shadows::default();
        assert!(s.none.blur < s.sm.blur);
        assert!(s.sm.blur < s.md.blur);
        assert!(s.md.blur < s.lg.blur);
    }
}
