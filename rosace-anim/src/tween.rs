use crate::easing::{easing_fn, Easing};
use crate::lerp::Lerp;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationState {
    Idle,
    Running { elapsed: f32 },
    Finished,
}

impl AnimationState {
    pub fn is_running(&self) -> bool {
        matches!(self, AnimationState::Running { .. })
    }
    pub fn is_finished(&self) -> bool {
        matches!(self, AnimationState::Finished)
    }
    pub fn elapsed(&self) -> f32 {
        match self {
            AnimationState::Running { elapsed } => *elapsed,
            _ => 0.0,
        }
    }
}

/// Interpolates from `from` to `to` over `duration_secs` using `easing`.
#[derive(Debug, Clone)]
pub struct Tween<T: Lerp> {
    pub from: T,
    pub to: T,
    pub duration_secs: f32,
    pub easing: Easing,
}

impl<T: Lerp> Tween<T> {
    pub fn new(from: T, to: T, duration_secs: f32, easing: Easing) -> Self {
        Self {
            from,
            to,
            duration_secs: duration_secs.max(0.0001),
            easing,
        }
    }

    /// Sample the interpolated value at normalized t (0.0–1.0).
    pub fn sample(&self, t: f32) -> T {
        let progress = easing_fn(self.easing, t);
        T::lerp(&self.from, &self.to, progress)
    }

    /// Sample at an absolute time in seconds.
    pub fn sample_at(&self, elapsed: f32) -> T {
        self.sample(elapsed / self.duration_secs)
    }
}

/// Drives a `Tween<T>` with a state machine — call `tick(dt)` each frame.
#[derive(Debug, Clone)]
pub struct AnimationController<T: Lerp> {
    tween: Tween<T>,
    state: AnimationState,
    reversed: bool,
}

impl<T: Lerp> AnimationController<T> {
    pub fn new(tween: Tween<T>) -> Self {
        Self {
            tween,
            state: AnimationState::Idle,
            reversed: false,
        }
    }

    pub fn start(&mut self) {
        self.state = AnimationState::Running { elapsed: 0.0 };
        self.reversed = false;
    }

    pub fn reset(&mut self) {
        self.state = AnimationState::Idle;
        self.reversed = false;
    }

    pub fn reverse(&mut self) {
        self.reversed = !self.reversed;
        if self.state == AnimationState::Finished {
            self.state = AnimationState::Running { elapsed: 0.0 };
        }
    }

    /// Advance animation by `dt` seconds. Returns current interpolated value.
    pub fn tick(&mut self, dt: f32) -> T {
        match self.state {
            AnimationState::Idle => {
                if self.reversed {
                    T::lerp(&self.tween.from, &self.tween.to, 1.0)
                } else {
                    self.tween.from.clone()
                }
            }
            AnimationState::Running { elapsed } => {
                let new_elapsed = (elapsed + dt).min(self.tween.duration_secs);
                self.state = if new_elapsed >= self.tween.duration_secs {
                    AnimationState::Finished
                } else {
                    AnimationState::Running { elapsed: new_elapsed }
                };
                let t = new_elapsed / self.tween.duration_secs;
                let t = if self.reversed { 1.0 - t } else { t };
                self.tween.sample(t)
            }
            AnimationState::Finished => {
                let t = if self.reversed { 0.0 } else { 1.0 };
                self.tween.sample(t)
            }
        }
    }

    pub fn state(&self) -> AnimationState {
        self.state
    }
    pub fn tween(&self) -> &Tween<T> {
        &self.tween
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tween() -> Tween<f32> {
        Tween::new(0.0_f32, 100.0, 1.0, Easing::Linear)
    }

    #[test]
    fn tween_new() {
        let t = make_tween();
        assert!((t.from - 0.0).abs() < 1e-6);
        assert!((t.to - 100.0).abs() < 1e-6);
        assert!((t.duration_secs - 1.0).abs() < 1e-6);
    }

    #[test]
    fn tween_sample_at_zero() {
        let t = make_tween();
        assert!((t.sample(0.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn tween_sample_at_one() {
        let t = make_tween();
        assert!((t.sample(1.0) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn tween_sample_at_half() {
        let t = make_tween();
        assert!((t.sample(0.5) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn tween_sample_at_returns_same_as_sample() {
        let t = make_tween();
        let via_sample = t.sample(0.3);
        let via_sample_at = t.sample_at(0.3); // duration=1.0, so elapsed=0.3 → t=0.3
        assert!((via_sample - via_sample_at).abs() < 1e-6);
    }

    #[test]
    fn animation_state_is_running() {
        let s = AnimationState::Running { elapsed: 0.1 };
        assert!(s.is_running());
        assert!(!s.is_finished());
    }

    #[test]
    fn animation_state_is_finished() {
        let s = AnimationState::Finished;
        assert!(s.is_finished());
        assert!(!s.is_running());
    }

    #[test]
    fn animation_state_elapsed() {
        let s = AnimationState::Running { elapsed: 0.42 };
        assert!((s.elapsed() - 0.42).abs() < 1e-6);
        assert!((AnimationState::Idle.elapsed() - 0.0).abs() < 1e-6);
        assert!((AnimationState::Finished.elapsed() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn controller_new_idle() {
        let ctrl = AnimationController::new(make_tween());
        assert_eq!(ctrl.state(), AnimationState::Idle);
    }

    #[test]
    fn controller_start_sets_running() {
        let mut ctrl = AnimationController::new(make_tween());
        ctrl.start();
        assert!(ctrl.state().is_running());
    }

    #[test]
    fn controller_tick_advances() {
        let mut ctrl = AnimationController::new(make_tween());
        ctrl.start();
        let v = ctrl.tick(0.5);
        // At t=0.5 with linear easing from 0→100: should be ~50
        assert!((v - 50.0).abs() < 1e-4, "expected ~50, got {v}");
    }

    #[test]
    fn controller_tick_finishes() {
        let mut ctrl = AnimationController::new(make_tween());
        ctrl.start();
        ctrl.tick(2.0); // advance past duration
        assert!(ctrl.state().is_finished());
    }

    #[test]
    fn controller_reset() {
        let mut ctrl = AnimationController::new(make_tween());
        ctrl.start();
        ctrl.tick(0.5);
        ctrl.reset();
        assert_eq!(ctrl.state(), AnimationState::Idle);
    }

    #[test]
    fn controller_reverse_flag() {
        let mut ctrl = AnimationController::new(make_tween());
        ctrl.start();
        // Advance to finish
        ctrl.tick(2.0);
        assert!(ctrl.state().is_finished());
        // Reverse from finished — should restart Running
        ctrl.reverse();
        assert!(ctrl.state().is_running());
    }

    #[test]
    fn controller_tick_at_duration_returns_to() {
        let mut ctrl = AnimationController::new(make_tween());
        ctrl.start();
        let v = ctrl.tick(1.0);
        assert!((v - 100.0).abs() < 1e-4, "expected 100.0 at end, got {v}");
        assert!(ctrl.state().is_finished());
    }

    #[test]
    fn controller_zero_duration_safe() {
        // duration_secs is clamped to 0.0001, so this should not divide by zero
        let tween = Tween::new(0.0_f32, 100.0, 0.0, Easing::Linear);
        assert!(tween.duration_secs > 0.0);
        let mut ctrl = AnimationController::new(tween);
        ctrl.start();
        let v = ctrl.tick(0.001);
        assert!(v.is_finite(), "value should be finite: {v}");
    }
}
