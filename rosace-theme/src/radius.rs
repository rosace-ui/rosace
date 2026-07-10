//! Border-radius tokens — the standard corner-rounding values.

/// Named corner radii in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct BorderRadius {
    /// No rounding (0 px).
    pub none: f32,
    /// Small rounding (4 px).
    pub sm: f32,
    /// Medium rounding (8 px).
    pub md: f32,
    /// Large rounding (12 px).
    pub lg: f32,
    /// Extra-large rounding (16 px).
    pub xl: f32,
    /// Full pill shape (9999 px).
    pub full: f32,
}

impl Default for BorderRadius {
    fn default() -> Self {
        Self {
            none: 0.0,
            sm: 4.0,
            md: 8.0,
            lg: 12.0,
            xl: 16.0,
            full: 9999.0,
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
    fn border_radius_defaults_are_correct() {
        let r = BorderRadius::default();
        assert_eq!(r.none, 0.0);
        assert_eq!(r.sm, 4.0);
        assert_eq!(r.md, 8.0);
        assert_eq!(r.lg, 12.0);
        assert_eq!(r.xl, 16.0);
        assert_eq!(r.full, 9999.0);
    }

    #[test]
    fn border_radius_values_are_ascending() {
        let r = BorderRadius::default();
        assert!(r.none < r.sm);
        assert!(r.sm < r.md);
        assert!(r.md < r.lg);
        assert!(r.lg < r.xl);
        assert!(r.xl < r.full);
    }
}
