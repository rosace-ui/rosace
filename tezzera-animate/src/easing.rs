/// Easing functions that map a linear time value `t ∈ [0, 1]` to a curved output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    /// Cubic ease-in: accelerates from rest (t^3)
    EaseIn,
    /// Cubic ease-out: decelerates to rest (1-(1-t)^3)
    EaseOut,
    /// Cubic ease-in-out: smooth step
    EaseInOut,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    /// Overshoots slightly at the start
    EaseInBack,
    /// Overshoots slightly at the end
    EaseOutBack,
    EaseOutBounce,
    /// CSS-style cubic-bezier via control points p1x, p1y, p2x, p2y
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    pub fn eval(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t * t,
            Self::EaseOut => 1.0 - (1.0 - t).powi(3),
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0_f32).powi(3) / 2.0
                }
            }
            Self::EaseInQuad => t * t,
            Self::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Self::EaseInOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0_f32).powi(2) / 2.0
                }
            }
            Self::EaseInBack => {
                let c1 = 1.70158_f32;
                let c3 = c1 + 1.0;
                c3 * t * t * t - c1 * t * t
            }
            Self::EaseOutBack => {
                let c1 = 1.70158_f32;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
            Self::EaseOutBounce => Self::bounce_out(t),
            Self::CubicBezier(x1, y1, x2, y2) => {
                Self::cubic_bezier(*x1, *y1, *x2, *y2, t)
            }
        }
    }

    fn bounce_out(t: f32) -> f32 {
        let n1 = 7.5625_f32;
        let d1 = 2.75_f32;
        if t < 1.0 / d1 {
            n1 * t * t
        } else if t < 2.0 / d1 {
            let t = t - 1.5 / d1;
            n1 * t * t + 0.75
        } else if t < 2.5 / d1 {
            let t = t - 2.25 / d1;
            n1 * t * t + 0.9375
        } else {
            let t = t - 2.625 / d1;
            n1 * t * t + 0.984375
        }
    }

    fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
        // Newton-Raphson approximation for CSS cubic-bezier
        let cx = 3.0 * x1;
        let bx = 3.0 * (x2 - x1) - cx;
        let ax = 1.0 - cx - bx;
        let cy = 3.0 * y1;
        let by = 3.0 * (y2 - y1) - cy;
        let ay = 1.0 - cy - by;
        let x_for_t = |t: f32| ((ax * t + bx) * t + cx) * t;
        // Solve x_for_t(guess) == t via bisection
        let mut lo = 0.0_f32;
        let mut hi = 1.0_f32;
        let mut guess = t;
        for _ in 0..8 {
            let x = x_for_t(guess);
            if (x - t).abs() < 1e-4 {
                break;
            }
            if x < t {
                lo = guess;
            } else {
                hi = guess;
            }
            guess = (lo + hi) / 2.0;
        }
        ((ay * guess + by) * guess + cy) * guess
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_values() {
        assert_eq!(Easing::Linear.eval(0.0), 0.0);
        assert_eq!(Easing::Linear.eval(0.5), 0.5);
        assert_eq!(Easing::Linear.eval(1.0), 1.0);
    }

    #[test]
    fn ease_in_at_half_is_slower_than_linear() {
        // EaseIn accelerates slowly at start, so at t=0.5 output < 0.5
        assert!(Easing::EaseIn.eval(0.5) < 0.5);
    }

    #[test]
    fn ease_out_at_half_is_faster_than_linear() {
        // EaseOut decelerates at end, so at t=0.5 output > 0.5
        assert!(Easing::EaseOut.eval(0.5) > 0.5);
    }

    #[test]
    fn ease_in_boundary_values() {
        assert_eq!(Easing::EaseIn.eval(0.0), 0.0);
        assert_eq!(Easing::EaseIn.eval(1.0), 1.0);
    }

    #[test]
    fn ease_out_boundary_values() {
        assert_eq!(Easing::EaseOut.eval(0.0), 0.0);
        assert_eq!(Easing::EaseOut.eval(1.0), 1.0);
    }

    #[test]
    fn ease_out_bounce_at_one() {
        assert!((Easing::EaseOutBounce.eval(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn ease_out_bounce_at_zero() {
        assert!((Easing::EaseOutBounce.eval(0.0) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn cubic_bezier_linear_approximation() {
        // CubicBezier(0,0,1,1) should approximate linear
        let e = Easing::CubicBezier(0.0, 0.0, 1.0, 1.0);
        assert!((e.eval(0.5) - 0.5).abs() < 0.05);
    }

    #[test]
    fn clamp_outside_range() {
        // Values outside [0,1] should be clamped
        assert_eq!(Easing::Linear.eval(-0.5), 0.0);
        assert_eq!(Easing::Linear.eval(1.5), 1.0);
    }
}
