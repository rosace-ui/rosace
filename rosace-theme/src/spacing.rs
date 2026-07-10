//! Spacing tokens — the standard set of logical-pixel gaps used across the UI.

/// Named spacing values in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Spacing {
    /// Extra-small gap (4 px).
    pub xs: f32,
    /// Small gap (8 px).
    pub sm: f32,
    /// Medium gap (16 px).
    pub md: f32,
    /// Large gap (24 px).
    pub lg: f32,
    /// Extra-large gap (32 px).
    pub xl: f32,
    /// Double extra-large gap (48 px).
    pub xxl: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: 4.0,
            sm: 8.0,
            md: 16.0,
            lg: 24.0,
            xl: 32.0,
            xxl: 48.0,
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
    fn spacing_defaults_are_correct() {
        let s = Spacing::default();
        assert_eq!(s.xs, 4.0);
        assert_eq!(s.sm, 8.0);
        assert_eq!(s.md, 16.0);
        assert_eq!(s.lg, 24.0);
        assert_eq!(s.xl, 32.0);
        assert_eq!(s.xxl, 48.0);
    }

    #[test]
    fn spacing_values_are_ascending() {
        let s = Spacing::default();
        assert!(s.xs < s.sm);
        assert!(s.sm < s.md);
        assert!(s.md < s.lg);
        assert!(s.lg < s.xl);
        assert!(s.xl < s.xxl);
    }
}
