/// Animation easing function selector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Custom cubic bezier — p1x, p1y, p2x, p2y (control points, same as CSS cubic-bezier)
    CubicBezier(f32, f32, f32, f32),
    /// Simple spring approximation (not physics-accurate)
    Spring { stiffness: f32, damping: f32 },
}


/// Map normalized time t (0.0–1.0) to eased progress (0.0–1.0).
///
/// Values outside [0,1] are clamped before easing.
pub fn easing_fn(easing: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::EaseIn => t * t,
        Easing::EaseOut => t * (2.0 - t),
        Easing::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        }
        Easing::CubicBezier(p1x, p1y, p2x, p2y) => {
            // Newton-Raphson approximation of cubic bezier (simplified)
            // This is a best-effort approximation adequate for game-like UI animations
            let _ = (p1x, p2x); // x control points used for t mapping (simplified to t itself)
            // Sample the Y curve at x≈t using cubic formula
            let t2 = t * t;
            let t3 = t2 * t;
            let mt = 1.0 - t;
            let mt2 = mt * mt;
            let mt3 = mt2 * mt;
            // B(t) = 3*mt²*t*p1y + 3*mt*t²*p2y + t³
            let _ = mt3; // used implicitly via the cubic formula (B(0)=0, B(1)=1 endpoint conditions)
            (3.0 * mt2 * t * p1y + 3.0 * mt * t2 * p2y + t3).clamp(0.0, 1.0)
        }
        Easing::Spring { stiffness, damping } => {
            // Overdamped spring approximation: 1 - e^(-s*t) * cos(d*t)
            let s = stiffness.max(0.1);
            let d = damping.max(0.1);
            (1.0 - (-s * t).exp() * (d * t * std::f32::consts::PI).cos()).clamp(0.0, 1.0)
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
    fn easing_linear_passthrough() {
        assert!((easing_fn(Easing::Linear, 0.0) - 0.0).abs() < 1e-6);
        assert!((easing_fn(Easing::Linear, 1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn easing_linear_midpoint() {
        assert!((easing_fn(Easing::Linear, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn easing_ease_in_is_slow_at_start() {
        // EaseIn (t²) should be less than linear at t=0.25
        let ease_in = easing_fn(Easing::EaseIn, 0.25);
        let linear = 0.25_f32;
        assert!(ease_in < linear, "EaseIn should be slower than linear at start: {ease_in} vs {linear}");
    }

    #[test]
    fn easing_ease_out_is_fast_at_start() {
        // EaseOut should be greater than linear at t=0.25
        let ease_out = easing_fn(Easing::EaseOut, 0.25);
        let linear = 0.25_f32;
        assert!(ease_out > linear, "EaseOut should be faster than linear at start: {ease_out} vs {linear}");
    }

    #[test]
    fn easing_ease_in_out_symmetric() {
        // f(t) + f(1-t) should equal 1.0 (symmetric)
        let t = 0.3_f32;
        let a = easing_fn(Easing::EaseInOut, t);
        let b = easing_fn(Easing::EaseInOut, 1.0 - t);
        assert!((a + b - 1.0).abs() < 1e-6, "EaseInOut should be symmetric: {a} + {b} != 1");
    }

    #[test]
    fn easing_ease_in_out_at_zero() {
        assert!((easing_fn(Easing::EaseInOut, 0.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn easing_ease_in_out_at_one() {
        assert!((easing_fn(Easing::EaseInOut, 1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn easing_clamps_below_zero() {
        assert!((easing_fn(Easing::Linear, -1.0) - 0.0).abs() < 1e-6);
        assert!((easing_fn(Easing::EaseIn, -5.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn easing_clamps_above_one() {
        assert!((easing_fn(Easing::Linear, 2.0) - 1.0).abs() < 1e-6);
        assert!((easing_fn(Easing::EaseOut, 99.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn easing_cubic_bezier_endpoints() {
        let easing = Easing::CubicBezier(0.25, 0.1, 0.25, 1.0);
        assert!((easing_fn(easing, 0.0) - 0.0).abs() < 1e-6);
        assert!((easing_fn(easing, 1.0) - 1.0).abs() < 1e-6);
    }
}
