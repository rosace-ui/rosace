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

    /// Step the spring simulation forward by `dt` seconds.
    ///
    /// Returns the current position after the step.
    pub fn update(&mut self, dt: f32) -> f32 {
        let force = -self.stiffness * (self.position - self.target)
            - self.damping * self.velocity;
        let accel = force / self.mass;
        self.velocity += accel * dt;
        self.position += self.velocity * dt;
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
}
