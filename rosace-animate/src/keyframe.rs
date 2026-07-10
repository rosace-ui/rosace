use crate::{Easing, Lerp};

/// A single stop in a keyframe sequence.
#[derive(Debug, Clone)]
pub struct KeyframeStop<T: Lerp> {
    /// Normalized time position in [0.0, 1.0].
    pub t: f32,
    pub value: T,
    /// Easing applied when interpolating *from* this stop toward the next.
    pub easing: Easing,
}

/// Multi-stop keyframe sequence. Evaluate at any `t ∈ [0, 1]` to obtain an
/// interpolated value between the two surrounding stops.
pub struct Keyframe<T: Lerp> {
    stops: Vec<KeyframeStop<T>>,
}

impl<T: Lerp> Keyframe<T> {
    pub fn new() -> Self {
        Self { stops: Vec::new() }
    }

    /// Add a stop with `Easing::Linear` between this stop and the next.
    pub fn stop(mut self, t: f32, value: T) -> Self {
        self.stops.push(KeyframeStop {
            t,
            value,
            easing: Easing::Linear,
        });
        self.stops
            .sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
        self
    }

    /// Add a stop with a custom easing function for the segment that follows it.
    pub fn stop_with_easing(mut self, t: f32, value: T, easing: Easing) -> Self {
        self.stops.push(KeyframeStop { t, value, easing });
        self.stops
            .sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
        self
    }

    /// Evaluate the keyframe sequence at `t ∈ [0, 1]`.
    ///
    /// Returns `None` if the sequence has no stops.
    pub fn eval(&self, t: f32) -> Option<T> {
        if self.stops.is_empty() {
            return None;
        }
        if t <= self.stops[0].t {
            return Some(self.stops[0].value.clone());
        }
        let last = self.stops.last().unwrap();
        if t >= last.t {
            return Some(last.value.clone());
        }
        for i in 0..self.stops.len() - 1 {
            let a = &self.stops[i];
            let b = &self.stops[i + 1];
            if t >= a.t && t <= b.t {
                let seg_t = (t - a.t) / (b.t - a.t);
                let eased = a.easing.eval(seg_t);
                return Some(T::lerp(&a.value, &b.value, eased));
            }
        }
        None
    }

    pub fn stops(&self) -> &[KeyframeStop<T>] {
        &self.stops
    }
}

impl<T: Lerp> Default for Keyframe<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframe_empty_returns_none() {
        let kf: Keyframe<f32> = Keyframe::new();
        assert!(kf.eval(0.5).is_none());
    }

    #[test]
    fn keyframe_eval_at_stop_returns_exact_value() {
        let kf = Keyframe::new()
            .stop(0.0, 0.0_f32)
            .stop(0.5, 5.0_f32)
            .stop(1.0, 10.0_f32);
        assert_eq!(kf.eval(0.5).unwrap(), 5.0);
    }

    #[test]
    fn keyframe_eval_before_first_stop() {
        let kf = Keyframe::new().stop(0.5, 42.0_f32);
        assert_eq!(kf.eval(0.0).unwrap(), 42.0);
    }

    #[test]
    fn keyframe_eval_after_last_stop() {
        let kf = Keyframe::new().stop(0.5, 42.0_f32);
        assert_eq!(kf.eval(1.0).unwrap(), 42.0);
    }

    #[test]
    fn keyframe_eval_between_stops_interpolates() {
        let kf = Keyframe::new()
            .stop(0.0, 0.0_f32)
            .stop(1.0, 10.0_f32);
        // Linear easing by default — midpoint should be 5.0
        let mid = kf.eval(0.5).unwrap();
        assert!((mid - 5.0).abs() < 1e-5, "expected 5.0, got {}", mid);
    }

    #[test]
    fn keyframe_eval_between_stops_with_easing() {
        let kf = Keyframe::new()
            .stop_with_easing(0.0, 0.0_f32, Easing::EaseIn)
            .stop(1.0, 1.0_f32);
        let mid = kf.eval(0.5).unwrap();
        // EaseIn at t=0.5 => t^3 = 0.125
        assert!((mid - 0.125).abs() < 1e-5, "expected 0.125, got {}", mid);
    }

    #[test]
    fn keyframe_stops_are_sorted() {
        let kf = Keyframe::new()
            .stop(1.0, 10.0_f32)
            .stop(0.0, 0.0_f32)
            .stop(0.5, 5.0_f32);
        // Verify ordering via eval
        assert_eq!(kf.eval(0.0).unwrap(), 0.0);
        assert_eq!(kf.eval(1.0).unwrap(), 10.0);
    }
}
