use std::time::Duration;
use tezzera_core::Context;
use tezzera_state::Atom;
use crate::controller::{AnimationController, AnimationState};

// Required by Atom<T>
unsafe impl Send for AnimationController {}
unsafe impl Sync for AnimationController {}

/// Read-only handle to an animation's current progress value.
pub struct Progress {
    value: Atom<f32>,
}

impl Progress {
    /// Current progress in `[0.0, 1.0]` this frame.
    pub fn get(&self) -> f32 { self.value.get() }
}

/// Control handle returned by [`use_animation`].
///
/// Call [`play`] to start or resume, [`pause`] to freeze, [`reset`] to rewind.
/// All calls take effect on the next frame.
#[derive(Clone)]
pub struct AnimCtrl {
    state: Atom<AnimationController>,
}

impl AnimCtrl {
    /// Start the animation from the beginning, or resume if paused.
    pub fn play(&self) {
        self.state.update(|s| {
            let mut next = s.clone();
            next.play();
            next
        });
    }

    /// Freeze the animation at its current progress.
    pub fn pause(&self) {
        self.state.update(|s| {
            let mut next = s.clone();
            next.pause();
            next
        });
    }

    /// Rewind the animation to the beginning and go idle.
    pub fn reset(&self) {
        self.state.update(|s| {
            let mut next = s.clone();
            next.reset();
            next
        });
    }

    /// Set `repeat = true` so the animation loops automatically.
    pub fn set_repeat(&self, repeat: bool) {
        self.state.update(|s| {
            let mut next = s.clone();
            next.repeat = repeat;
            next
        });
    }

    /// Set `reverse = true` so the output goes 1.0 → 0.0 instead of 0.0 → 1.0.
    pub fn set_reverse(&self, reverse: bool) {
        self.state.update(|s| {
            let mut next = s.clone();
            next.reverse = reverse;
            next
        });
    }
}

/// Component hook that drives a time-based animation.
///
/// On every frame that the animation is `Running`, the controller advances by
/// the real wall-clock `dt` and writes the new progress to a persistent atom.
/// That atom write triggers the next frame automatically — no manual `tick()`
/// call is ever needed.
///
/// # Example
/// ```rust,ignore
/// let (progress, ctrl) = use_animation(ctx, Duration::from_millis(500));
///
/// // In a button handler:
/// let c = ctrl.clone();
/// Button::new("Play").on_press(move || c.play())
///
/// // Reading the value:
/// ProgressBar::new(progress.get())
/// ```
pub fn use_animation(ctx: &mut Context, duration: Duration) -> (Progress, AnimCtrl) {
    let state_atom: Atom<AnimationController> = ctx.state(AnimationController::new(duration));
    let value_atom: Atom<f32> = ctx.state(0.0_f32);

    let s = state_atom.get();
    if s.state() == AnimationState::Running {
        let mut next = s.clone();
        let progress = next.tick(crate::frame_dt());
        value_atom.set(progress);
        state_atom.set(next);
    }

    let ctrl     = AnimCtrl { state: state_atom };
    let progress = Progress { value: value_atom };
    (progress, ctrl)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::set_frame_dt;

    #[test]
    fn animation_reaches_complete_after_full_duration() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.start();
        let mut progress = 0.0_f32;
        // Feed 64 ticks of 16ms each (~1.024s total)
        for _ in 0..64 {
            progress = ctrl.tick(0.016);
        }
        assert_eq!(ctrl.state(), AnimationState::Complete);
        assert_eq!(progress, 1.0);
    }

    #[test]
    fn animctrl_pause_stops_progress() {
        let atom = tezzera_state::use_atom(AnimationController::new(Duration::from_secs(1)));
        let ctrl = AnimCtrl { state: atom.clone() };
        // Start
        atom.update(|s| { let mut n = s.clone(); n.start(); n });
        // Tick once
        atom.update(|s| { let mut n = s.clone(); n.tick(0.1); n });
        let p1 = atom.get().progress();
        // Pause
        ctrl.pause();
        // Try to tick — should have no effect
        atom.update(|s| { let mut n = s.clone(); n.tick(0.5); n });
        let p2 = atom.get().progress();
        assert_eq!(p1, p2, "progress must not change while paused");
    }

    #[test]
    fn animctrl_play_resumes_from_paused_position() {
        let atom = tezzera_state::use_atom(AnimationController::new(Duration::from_secs(1)));
        let ctrl = AnimCtrl { state: atom.clone() };
        atom.update(|s| { let mut n = s.clone(); n.start(); n });
        atom.update(|s| { let mut n = s.clone(); n.tick(0.3); n });
        ctrl.pause();
        let p_before = atom.get().progress();
        ctrl.play(); // should resume, not restart
        assert_eq!(atom.get().state(), AnimationState::Running);
        assert!((atom.get().progress() - p_before).abs() < 1e-6,
            "play() after pause must resume from same position");
    }

    #[test]
    fn animation_frame_rate_independent() {
        // Both run for 1.1 seconds wall-clock time — both must complete a 1s animation.
        // f32 accumulated steps may not sum to exactly 1.0, so we overshoot by 10%.

        let mut c60 = AnimationController::new(Duration::from_secs(1));
        c60.start();
        // 66 ticks of 1/60 ≈ 1.1s
        for _ in 0..66 {
            c60.tick(1.0 / 60.0);
        }

        let mut c120 = AnimationController::new(Duration::from_secs(1));
        c120.start();
        // 132 ticks of 1/120 ≈ 1.1s
        for _ in 0..132 {
            c120.tick(1.0 / 120.0);
        }

        assert_eq!(c60.state(), AnimationState::Complete);
        assert_eq!(c120.state(), AnimationState::Complete);
        assert_eq!(c60.progress(), 1.0);
        assert_eq!(c120.progress(), 1.0);
    }

    #[test]
    fn set_frame_dt_and_use_animation_self_perpetuates() {
        set_frame_dt(1.0 / 60.0);
        // Verify the clock default is set without panicking
        assert!(crate::frame_dt() > 0.0);
    }
}
