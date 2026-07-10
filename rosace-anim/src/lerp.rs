use rosace_theme::Color;

/// Linear interpolation between two values.
pub trait Lerp: Clone {
    fn lerp(a: &Self, b: &Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(a: &f32, b: &f32, t: f32) -> f32 {
        a + (b - a) * t
    }
}

impl Lerp for f64 {
    fn lerp(a: &f64, b: &f64, t: f32) -> f64 {
        a + (b - a) * (t as f64)
    }
}

impl Lerp for Color {
    fn lerp(a: &Color, b: &Color, t: f32) -> Color {
        Color {
            r: a.r + (b.r - a.r) * t,
            g: a.g + (b.g - a.g) * t,
            b: a.b + (b.b - a.b) * t,
            a: a.a + (b.a - a.a) * t,
        }
    }
}

impl Lerp for [f32; 2] {
    fn lerp(a: &[f32; 2], b: &[f32; 2], t: f32) -> [f32; 2] {
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }
}

impl Lerp for [f32; 4] {
    fn lerp(a: &[f32; 4], b: &[f32; 4], t: f32) -> [f32; 4] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::Lerp as _;
    use super::*;

    #[test]
    fn lerp_f32_at_zero() {
        assert!((f32::lerp(&0.0, &100.0, 0.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_f32_at_one() {
        assert!((f32::lerp(&0.0, &100.0, 1.0) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_f32_midpoint() {
        assert!((f32::lerp(&0.0, &100.0, 0.5) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_f64_midpoint() {
        assert!((f64::lerp(&0.0, &100.0, 0.5) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn lerp_color_midpoint() {
        let a = Color::rgb(0.0, 0.0, 0.0);
        let b = Color::rgb(1.0, 1.0, 1.0);
        let mid = <Color as Lerp>::lerp(&a, &b, 0.5);
        assert!((mid.r - 0.5).abs() < 1e-6);
        assert!((mid.g - 0.5).abs() < 1e-6);
        assert!((mid.b - 0.5).abs() < 1e-6);
    }

    #[test]
    fn lerp_color_at_zero() {
        let a = Color::rgb(0.2, 0.4, 0.6);
        let b = Color::rgb(1.0, 1.0, 1.0);
        let result = <Color as Lerp>::lerp(&a, &b, 0.0);
        assert!((result.r - a.r).abs() < 1e-6);
        assert!((result.g - a.g).abs() < 1e-6);
        assert!((result.b - a.b).abs() < 1e-6);
    }

    #[test]
    fn lerp_color_at_one() {
        let a = Color::rgb(0.0, 0.0, 0.0);
        let b = Color::rgb(0.3, 0.6, 0.9);
        let result = <Color as Lerp>::lerp(&a, &b, 1.0);
        assert!((result.r - b.r).abs() < 1e-6);
        assert!((result.g - b.g).abs() < 1e-6);
        assert!((result.b - b.b).abs() < 1e-6);
    }

    #[test]
    fn lerp_vec2_midpoint() {
        let a = [0.0_f32, 0.0];
        let b = [10.0_f32, 20.0];
        let mid = <[f32; 2]>::lerp(&a, &b, 0.5);
        assert!((mid[0] - 5.0).abs() < 1e-6);
        assert!((mid[1] - 10.0).abs() < 1e-6);
    }
}
