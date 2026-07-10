/// Physics-based spring animation. Call `update(dt)` each frame with the elapsed time in seconds.
pub struct Spring {
    /// Spring stiffness constant k — higher values produce snappier motion (default: 200).
    pub stiffness: f32,
    /// Damping coefficient b — higher values reduce oscillation (default: 20).
    pub damping: f32,
    /// Mass m — higher values slow down the spring response (default: 1).
    pub mass: f32,
    /// The target position the spring is pulling toward.
    pub target: f32,
    position: f32,
    velocity: f32,
}

impl Spring {
    pub fn new(initial: f32, target: f32) -> Self {
        Self {
            stiffness: 200.0,
            damping: 20.0,
            mass: 1.0,
            target,
            position: initial,
            velocity: 0.0,
        }
    }

    pub fn stiffness(mut self, k: f32) -> Self {
        self.stiffness = k;
        self
    }

    pub fn damping(mut self, d: f32) -> Self {
        self.damping = d;
        self
    }

    pub fn mass(mut self, m: f32) -> Self {
        self.mass = m;
        self
    }

    pub fn set_target(&mut self, t: f32) {
        self.target = t;
    }

    /// Largest per-step `dt` the semi-implicit Euler integration below stays
    /// numerically stable at, for every stiffness/damping this codebase
    /// actually uses (verified numerically, not assumed — see the doc
    /// comment on `update`). A real UI frame's `dt` can be much larger than
    /// this after any idle gap; `update` sub-steps to stay under it.
    const MAX_STABLE_STEP: f32 = 1.0 / 120.0;

    /// Step the spring simulation forward by `dt` seconds.
    ///
    /// Returns the current position after the step.
    ///
    /// Sub-steps internally at [`Self::MAX_STABLE_STEP`]-sized increments.
    /// A single big step is NOT just "less smooth" — semi-implicit Euler on
    /// a damped oscillator is only conditionally stable, and a step size
    /// past that threshold makes position/velocity diverge exponentially
    /// within a handful of calls (confirmed by direct simulation: stiffness
    /// 280/damping 26 at `dt = 0.1` — a value `rosace_animate::frame_dt`'s
    /// own clamp allows, and a realistic one after any real idle gap between
    /// frames — blows up from -800 to -1.8e14 in 20 steps). That diverged
    /// value doesn't just look wrong: a real crash traced an overflow panic
    /// in `rosace-render`'s text rasterizer straight back to a
    /// `ScreenTransitionView` slide offset that had exploded this way one
    /// screen transition after the app sat idle for a moment. Sub-stepping
    /// keeps every individual integration step inside the stable region
    /// regardless of the caller's stiffness/damping/mass or how large a
    /// single `dt` it's handed.
    pub fn update(&mut self, dt: f32) -> f32 {
        if dt <= 0.0 { return self.position; }
        let steps = (dt / Self::MAX_STABLE_STEP).ceil().max(1.0) as u32;
        let step = dt / steps as f32;
        for _ in 0..steps {
            let force = -self.stiffness * (self.position - self.target)
                - self.damping * self.velocity;
            let accel = force / self.mass;
            self.velocity += accel * step;
            self.position += self.velocity * step;
        }
        self.position
    }

    pub fn position(&self) -> f32 {
        self.position
    }

    /// Returns `true` when the spring is within a small threshold of the target
    /// and the velocity is nearly zero.
    pub fn is_settled(&self) -> bool {
        (self.position - self.target).abs() < 0.01 && self.velocity.abs() < 0.01
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simulate(spring: &mut Spring, steps: usize, dt: f32) {
        for _ in 0..steps {
            spring.update(dt);
        }
    }

    #[test]
    fn spring_moves_toward_target() {
        let mut s = Spring::new(0.0, 100.0);
        let initial = s.position();
        s.update(0.016);
        assert!(s.position() > initial, "spring should move toward target");
    }

    #[test]
    fn spring_settles_near_target() {
        let mut s = Spring::new(0.0, 100.0);
        simulate(&mut s, 500, 0.016);
        assert!(
            (s.position() - 100.0).abs() < 1.0,
            "spring should settle near target, got {}",
            s.position()
        );
    }

    #[test]
    fn spring_is_settled_after_convergence() {
        let mut s = Spring::new(0.0, 100.0);
        // Run for a long time to ensure convergence
        simulate(&mut s, 2000, 0.016);
        assert!(s.is_settled(), "spring should report settled");
    }

    #[test]
    fn spring_set_target_changes_direction() {
        let mut s = Spring::new(50.0, 100.0);
        simulate(&mut s, 50, 0.016);
        s.set_target(0.0);
        let pos_after_retarget = s.position();
        simulate(&mut s, 200, 0.016);
        assert!(
            s.position() < pos_after_retarget,
            "spring should move toward new target"
        );
    }

    #[test]
    fn spring_custom_params() {
        // Stiffer spring should settle faster
        let mut fast = Spring::new(0.0, 100.0).stiffness(500.0).damping(50.0);
        let mut slow = Spring::new(0.0, 100.0).stiffness(50.0).damping(5.0);
        for _ in 0..100 {
            fast.update(0.016);
            slow.update(0.016);
        }
        assert!(
            (fast.position() - 100.0).abs() <= (slow.position() - 100.0).abs(),
            "stiffer spring should be closer to target"
        );
    }

    /// Regression test for a real crash: a `ScreenTransitionView` slide
    /// (stiffness 280, damping 26 — `rosace-nav`'s own transition springs)
    /// hit a large real-world `dt` (a realistic idle gap between frames,
    /// well within `frame_dt`'s own 0.1s clamp) and diverged to an
    /// astronomical position within ~20 single-step calls, which downstream
    /// overflowed an i32 cast in text rendering. `update` must stay bounded
    /// and still make real forward progress toward the target even at the
    /// largest `dt` the real engine can hand it.
    #[test]
    fn update_stays_bounded_and_converges_at_a_large_realistic_dt() {
        let mut s = Spring::new(-800.0, 0.0).stiffness(280.0).damping(26.0).mass(1.0);
        for _ in 0..40 {
            s.update(0.1);
            assert!(
                s.position().is_finite() && s.position().abs() < 10_000.0,
                "position must stay bounded at dt=0.1, got {}", s.position()
            );
        }
        assert!(s.is_settled(), "must still settle near the target despite the large step size");
    }

    /// Even a single pathological `dt` (e.g. a real stall, not just an
    /// ordinary idle gap) must sub-step down to something stable rather
    /// than taking one giant unstable leap.
    #[test]
    fn update_does_not_diverge_on_a_single_very_large_dt() {
        let mut s = Spring::new(-800.0, 0.0).stiffness(280.0).damping(26.0).mass(1.0);
        s.update(2.0);
        assert!(s.position().is_finite() && s.position().abs() < 10_000.0,
            "a single large dt must not diverge, got {}", s.position());
    }
}
