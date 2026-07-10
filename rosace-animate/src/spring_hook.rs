use rosace_core::Context;
use rosace_state::Atom;

/// Persistent state for a spring simulation, stored across frames via hook state.
#[derive(Clone)]
pub struct SpringState {
    pub position: f32,
    pub velocity: f32,
    pub target:   f32,
    pub stiffness: f32,
    pub damping:   f32,
    pub mass:      f32,
}

impl SpringState {
    fn new(initial: f32) -> Self {
        Self {
            position:  initial,
            velocity:  0.0,
            target:    initial,
            stiffness: 200.0,
            damping:   20.0,
            mass:      1.0,
        }
    }

    fn step(&self, dt: f32) -> Self {
        let force = -self.stiffness * (self.position - self.target)
            - self.damping * self.velocity;
        let accel = force / self.mass;
        let new_vel = self.velocity + accel * dt;
        let new_pos = self.position + new_vel * dt;
        Self { position: new_pos, velocity: new_vel, ..self.clone() }
    }

    fn is_settled(&self) -> bool {
        (self.position - self.target).abs() < 0.01 && self.velocity.abs() < 0.01
    }
}

// Required by Atom<T>
unsafe impl Send for SpringState {}
unsafe impl Sync for SpringState {}

/// Read-only handle to a spring-animated f32 produced by [`use_spring`].
pub struct Animated {
    value: Atom<f32>,
}

impl Animated {
    /// Current interpolated value this frame.
    pub fn get(&self) -> f32 {
        self.value.get()
    }
}

/// Control handle returned by [`use_spring`].
///
/// Call [`animate_to`] from a button callback or any event handler to spring
/// toward a new target; the animation advances automatically every frame.
#[derive(Clone)]
pub struct SpringController {
    state: Atom<SpringState>,
}

impl SpringController {
    /// Drive the spring toward `target`. Takes effect on the next frame.
    pub fn animate_to(&self, target: f32) {
        self.state.update(|s| SpringState { target, ..s.clone() });
    }

    /// Adjust spring feel (call before the first `animate_to`).
    pub fn stiffness(self, k: f32) -> Self {
        self.state.update(|s| SpringState { stiffness: k, ..s.clone() });
        self
    }

    pub fn damping(self, d: f32) -> Self {
        self.state.update(|s| SpringState { damping: d, ..s.clone() });
        self
    }

    /// Jump directly to `value` with no animation (resets velocity).
    pub fn snap_to(&self, value: f32) {
        self.state.update(|s| SpringState {
            position: value,
            velocity: 0.0,
            target: value,
            ..s.clone()
        });
    }
}

/// Component hook that drives a spring-animated `f32` value.
///
/// Call this inside `Component::build`. On every frame the spring advances by
/// one tick (fixed 1/60 s) toward its current target and the result is written
/// into a persistent [`Atom<f32>`]. Widgets that read the returned [`Animated`]
/// value re-render each frame while the spring is in motion.
///
/// # Example
/// ```rust,ignore
/// let (display_value, ctrl) = use_spring(ctx, 0.0);
///
/// // In a button handler:
/// let c = ctrl.clone();
/// Button::new("Go").on_press(move || c.animate_to(100.0))
///
/// // Reading the animated value:
/// Text::new(format!("{:.0}", display_value.get()))
/// ```
pub fn use_spring(ctx: &mut Context, initial: f32) -> (Animated, SpringController) {
    // Two hook slots: one for the spring physics state, one for the output value.
    let state_atom: Atom<SpringState> = ctx.state(SpringState::new(initial));
    let value_atom: Atom<f32>         = ctx.state(initial);

    // Advance spring by one frame if it has not settled.
    // Cap dt to 1/60 s: Euler integration diverges at large steps (stiffness=200
    // requires dt < ~0.032 s for stability), so in slow debug builds we sub-step.
    let s = state_atom.get();
    if !s.is_settled() {
        let raw_dt = crate::frame_dt();
        let max_dt = 1.0_f32 / 60.0;
        let next = if raw_dt <= max_dt {
            s.step(raw_dt)
        } else {
            // Take multiple fixed-size steps to cover the real elapsed time.
            let steps = (raw_dt / max_dt).ceil() as u32;
            let sub_dt = raw_dt / steps as f32;
            let mut cur = s.clone();
            for _ in 0..steps {
                cur = cur.step(sub_dt);
            }
            cur
        };
        value_atom.set(next.position);
        state_atom.set(next);
    }

    let ctrl     = SpringController { state: state_atom };
    let animated = Animated { value: value_atom };
    (animated, ctrl)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_state_moves_toward_target() {
        let s = SpringState::new(0.0);
        let s2 = SpringState { target: 100.0, ..s }.step(0.016);
        assert!(s2.position > 0.0, "spring should move toward target");
    }

    #[test]
    fn spring_state_settles_at_target() {
        let mut s = SpringState::new(0.0);
        s.target = 100.0;
        for _ in 0..600 {
            s = s.step(0.016);
        }
        assert!(s.is_settled(), "spring should settle after 600 frames");
        assert!((s.position - 100.0).abs() < 1.0, "position should be near target");
    }

    #[test]
    fn spring_state_settled_at_start() {
        let s = SpringState::new(42.0);
        assert!(s.is_settled(), "spring with no target difference should be settled");
    }

    #[test]
    fn spring_controller_animate_to_changes_target() {
        let atom = rosace_state::use_atom(SpringState::new(0.0));
        let ctrl = SpringController { state: atom.clone() };
        ctrl.animate_to(50.0);
        assert_eq!(atom.get().target, 50.0);
    }

    #[test]
    fn spring_controller_snap_to_resets_position() {
        let atom = rosace_state::use_atom(SpringState::new(0.0));
        let ctrl = SpringController { state: atom.clone() };
        ctrl.snap_to(75.0);
        let s = atom.get();
        assert_eq!(s.position, 75.0);
        assert_eq!(s.velocity, 0.0);
        assert_eq!(s.target, 75.0);
    }

    #[test]
    fn animated_get_reads_value() {
        let atom = rosace_state::use_atom(3.14f32);
        let animated = Animated { value: atom.clone() };
        assert!((animated.get() - 3.14).abs() < 1e-5);
        atom.set(2.71);
        assert!((animated.get() - 2.71).abs() < 1e-5);
    }
}
