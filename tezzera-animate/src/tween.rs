use std::time::Duration;
use web_time::Instant;
use crate::{Easing, Lerp};

/// Time-based interpolation from one value to another using a chosen easing function.
pub struct Tween<T: Lerp> {
    pub from: T,
    pub to: T,
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
    started_at: Option<Instant>,
}

impl<T: Lerp> Tween<T> {
    pub fn new(from: T, to: T, duration: Duration) -> Self {
        Self {
            from,
            to,
            duration,
            easing: Easing::EaseInOut,
            delay: Duration::ZERO,
            started_at: None,
        }
    }

    pub fn easing(mut self, e: Easing) -> Self {
        self.easing = e;
        self
    }

    pub fn delay(mut self, d: Duration) -> Self {
        self.delay = d;
        self
    }

    pub fn start(&mut self) {
        self.started_at = Some(Instant::now());
    }

    pub fn reset(&mut self) {
        self.started_at = None;
    }

    /// Returns `(current_value, is_complete)`.
    ///
    /// Before `start()` is called, returns `(from, false)`.
    /// After the duration elapses, returns `(to, true)`.
    pub fn value(&self) -> (T, bool) {
        let Some(started) = self.started_at else {
            return (self.from.clone(), false);
        };
        let elapsed = started.elapsed();
        if elapsed < self.delay {
            return (self.from.clone(), false);
        }
        let t = ((elapsed - self.delay).as_secs_f32() / self.duration.as_secs_f32()).min(1.0);
        let eased = self.easing.eval(t);
        (T::lerp(&self.from, &self.to, eased), t >= 1.0)
    }

    pub fn is_complete(&self) -> bool {
        self.value().1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn tween_value_before_start_is_from() {
        let tween = Tween::new(0.0_f32, 1.0_f32, Duration::from_millis(100));
        let (val, complete) = tween.value();
        assert_eq!(val, 0.0);
        assert!(!complete);
    }

    #[test]
    fn tween_value_at_complete_is_to() {
        let mut tween = Tween::new(0.0_f32, 1.0_f32, Duration::from_millis(10))
            .easing(Easing::Linear);
        tween.start();
        // Wait for the tween to complete
        thread::sleep(Duration::from_millis(50));
        let (val, complete) = tween.value();
        assert!(complete, "tween should be complete");
        assert!((val - 1.0).abs() < 1e-5, "value should be 1.0, got {}", val);
    }

    #[test]
    fn tween_reset_returns_from() {
        let mut tween = Tween::new(5.0_f32, 10.0_f32, Duration::from_millis(10));
        tween.start();
        thread::sleep(Duration::from_millis(50));
        tween.reset();
        let (val, complete) = tween.value();
        assert_eq!(val, 5.0);
        assert!(!complete);
    }

    #[test]
    fn tween_with_delay_returns_from_during_delay() {
        let mut tween = Tween::new(0.0_f32, 1.0_f32, Duration::from_millis(100))
            .delay(Duration::from_secs(10));
        tween.start();
        let (val, complete) = tween.value();
        assert_eq!(val, 0.0);
        assert!(!complete);
    }
}
