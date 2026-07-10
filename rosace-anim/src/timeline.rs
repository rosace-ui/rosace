use crate::easing::{easing_fn, Easing};
use crate::lerp::Lerp;

/// A single keyframe in a timeline.
#[derive(Debug, Clone)]
pub struct Keyframe<T: Lerp> {
    /// Normalized time (0.0–1.0).
    pub time: f32,
    pub value: T,
    pub easing: Easing,
}

impl<T: Lerp> Keyframe<T> {
    pub fn new(time: f32, value: T, easing: Easing) -> Self {
        Self {
            time: time.clamp(0.0, 1.0),
            value,
            easing,
        }
    }
    pub fn linear(time: f32, value: T) -> Self {
        Self::new(time, value, Easing::Linear)
    }
}

/// A sequence of keyframes — sample at any normalized time.
///
/// Keyframes do not need to be pre-sorted; `sample()` sorts them on first call.
pub struct Timeline<T: Lerp> {
    keyframes: Vec<Keyframe<T>>,
}

impl<T: Lerp> Timeline<T> {
    pub fn new() -> Self {
        Self {
            keyframes: Vec::new(),
        }
    }

    pub fn with_keyframe(mut self, kf: Keyframe<T>) -> Self {
        self.keyframes.push(kf);
        self
    }

    /// Build from a vec. Sorts by time.
    pub fn from_keyframes(mut kfs: Vec<Keyframe<T>>) -> Self {
        kfs.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self { keyframes: kfs }
    }

    /// Sample the interpolated value at normalized time t (0.0–1.0).
    pub fn sample(&self, t: f32) -> T {
        let t = t.clamp(0.0, 1.0);
        if self.keyframes.is_empty() {
            panic!("Timeline: no keyframes");
        }

        // Sort is O(n log n) — acceptable for small timelines
        let mut sorted = self.keyframes.clone();
        sorted.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if t <= sorted[0].time {
            return sorted[0].value.clone();
        }
        if t >= sorted[sorted.len() - 1].time {
            return sorted[sorted.len() - 1].value.clone();
        }

        // Find surrounding keyframes
        for i in 0..sorted.len() - 1 {
            let a = &sorted[i];
            let b = &sorted[i + 1];
            if t >= a.time && t <= b.time {
                let segment_t = (t - a.time) / (b.time - a.time);
                let eased_t = easing_fn(a.easing, segment_t);
                return T::lerp(&a.value, &b.value, eased_t);
            }
        }

        sorted.last().unwrap().value.clone()
    }

    pub fn keyframe_count(&self) -> usize {
        self.keyframes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }
    pub fn duration_keyframe_span(&self) -> (f32, f32) {
        if self.keyframes.is_empty() {
            return (0.0, 0.0);
        }
        let min = self
            .keyframes
            .iter()
            .map(|k| k.time)
            .fold(f32::INFINITY, f32::min);
        let max = self
            .keyframes
            .iter()
            .map(|k| k.time)
            .fold(f32::NEG_INFINITY, f32::max);
        (min, max)
    }
}

impl<T: Lerp> Default for Timeline<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_timeline() -> Timeline<f32> {
        Timeline::new()
            .with_keyframe(Keyframe::linear(0.0, 0.0))
            .with_keyframe(Keyframe::linear(1.0, 100.0))
    }

    #[test]
    fn timeline_new_empty() {
        let t: Timeline<f32> = Timeline::new();
        assert!(t.is_empty());
        assert_eq!(t.keyframe_count(), 0);
    }

    #[test]
    fn timeline_add_keyframe() {
        let t = Timeline::new().with_keyframe(Keyframe::linear(0.0, 0.0_f32));
        assert_eq!(t.keyframe_count(), 1);
    }

    #[test]
    fn timeline_sample_at_start() {
        let t = make_timeline();
        assert!((t.sample(0.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn timeline_sample_at_end() {
        let t = make_timeline();
        assert!((t.sample(1.0) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn timeline_sample_at_midpoint() {
        let t = make_timeline();
        assert!((t.sample(0.5) - 50.0).abs() < 1e-5);
    }

    #[test]
    fn timeline_from_keyframes_sorted() {
        // Provide keyframes out of order
        let kfs = vec![
            Keyframe::linear(1.0, 100.0_f32),
            Keyframe::linear(0.0, 0.0),
            Keyframe::linear(0.5, 50.0),
        ];
        let t = Timeline::from_keyframes(kfs);
        // Should still sample correctly
        assert!((t.sample(0.5) - 50.0).abs() < 1e-5);
        assert!((t.sample(0.0) - 0.0).abs() < 1e-5);
        assert!((t.sample(1.0) - 100.0).abs() < 1e-5);
    }

    #[test]
    fn timeline_keyframe_count() {
        let t = make_timeline();
        assert_eq!(t.keyframe_count(), 2);
    }

    #[test]
    fn timeline_sample_before_first_keyframe() {
        // First keyframe at 0.2, not 0.0
        let t = Timeline::new()
            .with_keyframe(Keyframe::linear(0.2, 10.0_f32))
            .with_keyframe(Keyframe::linear(1.0, 100.0));
        // t < 0.2 should clamp to first keyframe value
        assert!((t.sample(0.0) - 10.0).abs() < 1e-5);
        assert!((t.sample(0.1) - 10.0).abs() < 1e-5);
    }

    #[test]
    fn timeline_sample_after_last_keyframe() {
        // Last keyframe at 0.8
        let t = Timeline::new()
            .with_keyframe(Keyframe::linear(0.0, 0.0_f32))
            .with_keyframe(Keyframe::linear(0.8, 80.0));
        // t > 0.8 should clamp to last keyframe value
        assert!((t.sample(1.0) - 80.0).abs() < 1e-5);
        assert!((t.sample(0.9) - 80.0).abs() < 1e-5);
    }

    #[test]
    fn timeline_duration_span() {
        let t = make_timeline();
        let (start, end) = t.duration_keyframe_span();
        assert!((start - 0.0).abs() < 1e-6);
        assert!((end - 1.0).abs() < 1e-6);
    }
}
