use std::time::Duration;

/// The lifecycle state of an `AnimationController`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationState {
    Idle,
    Running,
    Paused,
    Complete,
}

/// Drives a time-based animation by tracking elapsed progress and lifecycle.
///
/// Call `tick(dt)` each frame with the elapsed time in seconds; it returns the
/// current progress value in `[0, 1]`.
pub struct AnimationController {
    pub duration: Duration,
    pub repeat: bool,
    pub reverse: bool,
    state: AnimationState,
    /// Normalised progress in [0.0, 1.0].
    progress: f32,
}

impl AnimationController {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            repeat: false,
            reverse: false,
            state: AnimationState::Idle,
            progress: 0.0,
        }
    }

    pub fn start(&mut self) {
        self.state = AnimationState::Running;
        self.progress = 0.0;
    }

    pub fn pause(&mut self) {
        if self.state == AnimationState::Running {
            self.state = AnimationState::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.state == AnimationState::Paused {
            self.state = AnimationState::Running;
        }
    }

    pub fn reset(&mut self) {
        self.state = AnimationState::Idle;
        self.progress = 0.0;
    }

    /// Advance the animation by `dt` seconds and return the current output
    /// progress (accounting for `reverse`).
    ///
    /// Has no effect unless the controller is `Running`.
    pub fn tick(&mut self, dt: f32) -> f32 {
        if self.state != AnimationState::Running {
            return self.progress;
        }
        let step = dt / self.duration.as_secs_f32();
        self.progress = (self.progress + step).min(1.0);
        if self.progress >= 1.0 {
            if self.repeat {
                self.progress = 0.0;
            } else {
                self.state = AnimationState::Complete;
            }
        }
        if self.reverse {
            1.0 - self.progress
        } else {
            self.progress
        }
    }

    pub fn state(&self) -> AnimationState {
        self.state
    }

    pub fn progress(&self) -> f32 {
        self.progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_starts_idle() {
        let c = AnimationController::new(Duration::from_secs(1));
        assert_eq!(c.state(), AnimationState::Idle);
        assert_eq!(c.progress(), 0.0);
    }

    #[test]
    fn controller_ticks_to_complete() {
        let mut c = AnimationController::new(Duration::from_secs(1));
        c.start();
        // One full second of ticks at 16 ms
        for _ in 0..63 {
            c.tick(0.016);
        }
        // Final tick pushes it over
        c.tick(0.016);
        assert_eq!(c.state(), AnimationState::Complete);
        assert_eq!(c.progress(), 1.0);
    }

    #[test]
    fn controller_repeat_resets_progress() {
        let mut c = AnimationController::new(Duration::from_millis(10));
        c.repeat = true;
        c.start();
        // Advance well past the duration in one big step
        c.tick(1.0);
        assert_eq!(c.state(), AnimationState::Running, "repeat: should stay Running");
        assert_eq!(c.progress(), 0.0, "repeat: progress should reset to 0");
    }

    #[test]
    fn controller_pause_and_resume() {
        let mut c = AnimationController::new(Duration::from_secs(1));
        c.start();
        c.tick(0.1);
        c.pause();
        assert_eq!(c.state(), AnimationState::Paused);
        let p = c.progress();
        c.tick(0.5); // should have no effect while paused
        assert_eq!(c.progress(), p, "progress should not change while paused");
        c.resume();
        assert_eq!(c.state(), AnimationState::Running);
    }

    #[test]
    fn controller_reverse_inverts_output() {
        let mut c = AnimationController::new(Duration::from_secs(1));
        c.reverse = true;
        c.start();
        let out = c.tick(0.5);
        // raw progress = 0.5, reverse => output 0.5 (1 - 0.5)
        assert!((out - 0.5).abs() < 1e-5);
        let out_end = c.tick(0.5);
        // raw progress = 1.0, reverse => output 0.0 (1 - 1.0)
        assert!((out_end - 0.0).abs() < 1e-5);
    }

    #[test]
    fn controller_reset_goes_idle() {
        let mut c = AnimationController::new(Duration::from_secs(1));
        c.start();
        c.tick(1.1);
        c.reset();
        assert_eq!(c.state(), AnimationState::Idle);
        assert_eq!(c.progress(), 0.0);
    }

    #[test]
    fn controller_tick_while_idle_has_no_effect() {
        let mut c = AnimationController::new(Duration::from_secs(1));
        let out = c.tick(0.5);
        assert_eq!(out, 0.0);
        assert_eq!(c.state(), AnimationState::Idle);
    }
}
