/// Trait for types that can be linearly interpolated.
pub trait Lerp: Clone + Send + 'static {
    fn lerp(a: &Self, b: &Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(a: &f32, b: &f32, t: f32) -> f32 {
        a + (b - a) * t
    }
}

impl Lerp for f64 {
    fn lerp(a: &f64, b: &f64, t: f32) -> f64 {
        a + (b - a) * t as f64
    }
}

impl Lerp for [f32; 2] {
    fn lerp(a: &[f32; 2], b: &[f32; 2], t: f32) -> [f32; 2] {
        [f32::lerp(&a[0], &b[0], t), f32::lerp(&a[1], &b[1], t)]
    }
}

impl Lerp for [f32; 4] {
    fn lerp(a: &[f32; 4], b: &[f32; 4], t: f32) -> [f32; 4] {
        [
            f32::lerp(&a[0], &b[0], t),
            f32::lerp(&a[1], &b[1], t),
            f32::lerp(&a[2], &b[2], t),
            f32::lerp(&a[3], &b[3], t),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_f32_midpoint() {
        assert_eq!(f32::lerp(&0.0, &1.0, 0.5), 0.5);
    }

    #[test]
    fn lerp_f32_endpoints() {
        assert_eq!(f32::lerp(&2.0, &8.0, 0.0), 2.0);
        assert_eq!(f32::lerp(&2.0, &8.0, 1.0), 8.0);
    }

    #[test]
    fn lerp_f64_midpoint() {
        assert!((f64::lerp(&0.0, &1.0, 0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn lerp_array2_midpoint() {
        let result = <[f32; 2]>::lerp(&[0.0, 0.0], &[2.0, 4.0], 0.5);
        assert_eq!(result, [1.0, 2.0]);
    }

    #[test]
    fn lerp_array4_midpoint() {
        let result = <[f32; 4]>::lerp(&[0.0, 0.0, 0.0, 0.0], &[2.0, 4.0, 6.0, 8.0], 0.5);
        assert_eq!(result, [1.0, 2.0, 3.0, 4.0]);
    }
}
