use crate::physics::ScrollPhysics;
use rosace_state::Atom;

/// Controls a [`ScrollView`] programmatically.
///
/// All clones share the same underlying atoms so that separate handles can
/// observe and mutate the scroll position from different call sites.
#[derive(Clone)]
pub struct ScrollController {
    /// Current scroll offset `[x, y]` in pixels.
    pub offset: Atom<[f32; 2]>,
    pub content_size: Atom<[f32; 2]>,
    pub viewport_size: Atom<[f32; 2]>,
    /// Drag-to-pan + momentum bookkeeping (D108/Phase 26 Step 2). Internal —
    /// deliberately NOT subscribed to the owning component in `for_ctx`
    /// (unlike `offset`/`content_size`/`viewport_size`): these are written
    /// every frame during a drag/momentum decay, and the visible repaint
    /// already flows through `offset`'s own subscribed writes — subscribing
    /// these too would dirty the whole component every frame for no benefit.
    /// Absolute screen point of the last streamed drag position, `None`
    /// when not currently dragging.
    last_drag_point: Atom<Option<[f32; 2]>>,
    /// The real, currently-tracked drag/momentum velocity in px/s — computed
    /// from the actual offset delta each frame while dragging, never a
    /// fixed/assumed constant.
    velocity: Atom<[f32; 2]>,
    /// `offset` as of the last frame, used to derive `velocity` this frame.
    last_offset_for_velocity: Atom<[f32; 2]>,
    /// Whether this controller was `pressed` (per `PaintCtx::pressed()`) as
    /// of the last frame — detects the true→false transition that seeds
    /// momentum from the tracked velocity.
    was_pressed: Atom<bool>,
    /// Real elapsed time (seconds) since the last wheel/trackpad event —
    /// reset to 0 by the wheel callback, advanced each frame by
    /// `advance_wheel_idle`. Used (not a per-frame boolean) because wheel
    /// events don't arrive on a perfectly regular one-per-frame schedule —
    /// an earlier per-frame flag version sprang back the instant a single
    /// frame happened to have no fresh event, then got pushed forward again
    /// by the next one, producing a visible jitter/oscillation right at the
    /// boundary (found via real trackpad testing — described as "vibration,
    /// scroll a little up and down"). A short real-time grace period
    /// (`WHEEL_IDLE_GRACE`) instead of a single-frame check absorbs that
    /// irregularity.
    wheel_idle_time: Atom<f32>,
}

/// How long (real seconds) with no wheel event before a gesture is
/// considered truly over and momentum/spring-back are allowed to run.
/// Short enough that release still feels immediate, long enough to absorb
/// normal gaps between OS-delivered wheel events during one continuous
/// gesture.
pub const WHEEL_IDLE_GRACE: f32 = 0.12;

/// Maximum tracked velocity (px/s), clamped in `track_velocity`/
/// `set_velocity`. Without a cap, a very fast flick produced a
/// proportionally very long coast — found via real trackpad testing +
/// direct calculation: at friction=0.92 even a modest 200px/s release took
/// 1.2s to decay below the stop threshold, and higher speeds took longer
/// still (1.9s+ at 6000px/s) — real touch/trackpad scroll views (iOS,
/// Android) cap max fling velocity for exactly this reason, so a hard flick
/// doesn't feel unbounded/sluggish at the tail. 2500px/s is a fast, real
/// flick speed, not an arbitrary round number — chosen so the slowest
/// legitimate release still coasts, while capping how long the tail can run.
pub const MAX_VELOCITY: f32 = 2500.0;

/// Velocity magnitude (px/s) below which coasting is considered "stopped."
/// Raised from an earlier 0.5 — at 0.92 friction the tail from 0.5 down to
/// truly zero motion is imperceptible but still real seconds of elapsed
/// time; 15px/s is still much slower than any perceptible motion but cuts
/// the long, invisible tail short. Combined with `MAX_VELOCITY` and a
/// slightly higher friction (`ScrollStyle::default_for_platform`), brings
/// total coast time for the full realistic velocity range down to
/// ~0.35s-0.7s (confirmed by direct calculation), instead of 1.2s-1.9s+.
pub const COAST_STOP_THRESHOLD: f32 = 15.0;

impl ScrollController {
    /// Create (or retrieve) a controller persisted in component state — the
    /// scroll position survives rebuilds. Follows the hook rules: call
    /// unconditionally in `build()`, stable order.
    pub fn for_ctx(ctx: &mut rosace_core::Context) -> Self {
        let ctrl = ctx.state(Self::new()).get();
        // The inner atoms are framework-created (use_atom) — nothing
        // subscribes to them by default, so a scroll_to/wheel write would
        // request a frame that repaints NOTHING (cache-hit). Subscribing the
        // owning component makes controller writes dirty it like ctx.state
        // atoms do. (Duplicate subscribes are ignored.)
        let id = ctx.component_id();
        ctrl.offset.subscribe(id);
        ctrl.content_size.subscribe(id);
        ctrl.viewport_size.subscribe(id);
        ctrl
    }

    pub fn new() -> Self {
        Self {
            offset: rosace_state::use_atom([0.0f32; 2]),
            content_size: rosace_state::use_atom([0.0f32; 2]),
            viewport_size: rosace_state::use_atom([0.0f32; 2]),
            last_drag_point: rosace_state::use_atom(None),
            velocity: rosace_state::use_atom([0.0f32; 2]),
            last_offset_for_velocity: rosace_state::use_atom([0.0f32; 2]),
            was_pressed: rosace_state::use_atom(false),
            wheel_idle_time: rosace_state::use_atom(f32::MAX),
        }
    }

    /// Jump to an absolute position, clamped to valid bounds.
    pub fn scroll_to(&self, x: f32, y: f32) {
        let [cw, ch] = self.content_size.get();
        let [vw, vh] = self.viewport_size.get();
        let nx = x.clamp(0.0, (cw - vw).max(0.0));
        let ny = y.clamp(0.0, (ch - vh).max(0.0));
        self.offset.set([nx, ny]);
    }

    /// Scroll to the top (y = 0), preserving x.
    pub fn scroll_to_top(&self) {
        let [x, _] = self.offset.get();
        self.offset.set([x, 0.0]);
    }

    /// Scroll to the bottom (y = content_height − viewport_height), preserving x.
    pub fn scroll_to_bottom(&self) {
        let [x, _] = self.offset.get();
        let [_, ch] = self.content_size.get();
        let [_, vh] = self.viewport_size.get();
        self.offset.set([x, (ch - vh).max(0.0)]);
    }

    /// Add `(dx, dy)` to the current offset, clamped to valid bounds.
    pub fn scroll_by(&self, dx: f32, dy: f32) {
        let [ox, oy] = self.offset.get();
        let [cw, ch] = self.content_size.get();
        let [vw, vh] = self.viewport_size.get();
        let new_x = (ox + dx).clamp(0.0, (cw - vw).max(0.0));
        let new_y = (oy + dy).clamp(0.0, (ch - vh).max(0.0));
        self.offset.set([new_x, new_y]);
    }

    /// Returns the current `[offset_x, offset_y]`.
    pub fn offset(&self) -> [f32; 2] {
        self.offset.get()
    }

    /// Snapshot the current position for later restoration.
    pub fn save_position(&self) -> [f32; 2] {
        self.offset.get()
    }

    /// Restore a previously saved position.
    pub fn restore_position(&self, pos: [f32; 2]) {
        self.offset.set(pos);
    }

    // ── Drag-to-pan + momentum (D108/Phase 26 Step 2) ──────────────────────

    /// Streamed absolute drag position → delta since the last call (0 on the
    /// first call of a drag, since there's no prior point to diff against).
    /// Call `end_drag` on release so the next drag starts fresh.
    pub fn drag_delta(&self, x: f32, y: f32) -> (f32, f32) {
        let prev = self.last_drag_point.get();
        self.last_drag_point.set(Some([x, y]));
        match prev {
            Some([px, py]) => (x - px, y - py),
            None => (0.0, 0.0),
        }
    }

    /// Clears drag-position tracking — call on release so the next drag
    /// doesn't diff against a stale point.
    pub fn end_drag(&self) {
        self.last_drag_point.set(None);
    }

    /// Recomputes `velocity` from the real offset delta since the last call,
    /// in px/s — the actual measured drag/momentum speed, never an assumed
    /// constant. Call once per frame while dragging or coasting. Clamped to
    /// `MAX_VELOCITY` — see its doc comment for why.
    pub fn track_velocity(&self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let now = self.offset.get();
        let prev = self.last_offset_for_velocity.get();
        let vx = ((now[0] - prev[0]) / dt).clamp(-MAX_VELOCITY, MAX_VELOCITY);
        let vy = ((now[1] - prev[1]) / dt).clamp(-MAX_VELOCITY, MAX_VELOCITY);
        self.velocity.set([vx, vy]);
        self.last_offset_for_velocity.set(now);
    }

    /// The most recently tracked velocity (px/s) — see `track_velocity`.
    pub fn velocity(&self) -> [f32; 2] {
        self.velocity.get()
    }

    /// Sets the tracked velocity directly (px/s) — for input sources that
    /// aren't a continuous drag `track_velocity` can measure frame-to-frame
    /// (e.g. a discrete wheel/trackpad event), so `coast` still has a real
    /// speed to decay from once the events stop arriving. Clamped to
    /// `MAX_VELOCITY` — see its doc comment for why.
    pub fn set_velocity(&self, v: [f32; 2]) {
        self.velocity.set([v[0].clamp(-MAX_VELOCITY, MAX_VELOCITY), v[1].clamp(-MAX_VELOCITY, MAX_VELOCITY)]);
    }

    /// Whether this controller was `pressed` as of the last frame — used to
    /// detect the true→false transition that hands off to momentum.
    pub fn was_pressed(&self) -> bool {
        self.was_pressed.get()
    }

    pub fn set_was_pressed(&self, v: bool) {
        self.was_pressed.set(v);
    }

    /// Called by a wheel/trackpad scroll callback when it fires — resets
    /// the idle clock to 0.
    pub fn mark_wheel_active(&self) {
        self.wheel_idle_time.set(0.0);
    }

    /// Advances the wheel-idle clock by one real frame — call once per
    /// frame regardless of whether a wheel event landed.
    pub fn advance_wheel_idle(&self, dt: f32) {
        let t = self.wheel_idle_time.get();
        if t < f32::MAX / 2.0 {
            self.wheel_idle_time.set(t + dt);
        }
    }

    /// Whether a wheel/trackpad event landed within the last
    /// [`WHEEL_IDLE_GRACE`] real seconds — the caller uses this to hold off
    /// `coast`'s momentum/spring-back until the gesture has genuinely
    /// stopped, not just "no event in this exact frame" (see
    /// `wheel_idle_time`'s doc comment for why a single-frame check jittered).
    pub fn wheel_recently_active(&self) -> bool {
        self.wheel_idle_time.get() < WHEEL_IDLE_GRACE
    }

    /// Advances one frame of post-release momentum/bounce, using the real
    /// velocity `track_velocity`/`set_velocity` measured from actual input.
    /// Returns `true` while still moving/settling (caller should keep
    /// requesting frames); `false` once fully at rest.
    pub fn coast(&self, physics: ScrollPhysics, dt: f32) -> bool {
        // Under `Bounce`, an ALREADY-overscrolled offset springs back
        // immediately, regardless of remaining velocity — matching real
        // platforms (iOS `UIScrollView`, Android `OverScroller`), which
        // switch to spring recovery the instant the edge is crossed rather
        // than waiting for velocity to fully decay first. The first version
        // of this function waited for velocity to decay below the 0.5
        // threshold before ever calling `settle_bounce` — at friction=0.92
        // that's measured at ~1.35s for a real 400px/s release velocity
        // (confirmed by direct calculation, not assumed), during which the
        // view sat frozen mid-overscroll. Matches real trackpad testing:
        // "scroll, blank space, ~1 second pause, then springs back."
        if let ScrollPhysics::Bounce { spring_stiffness, .. } = physics {
            let [ox, oy] = self.offset.get();
            let [cw, ch] = self.content_size.get();
            let [vw, vh] = self.viewport_size.get();
            let max_x = (cw - vw).max(0.0);
            let max_y = (ch - vh).max(0.0);
            if ox < 0.0 || ox > max_x || oy < 0.0 || oy > max_y {
                self.velocity.set([0.0, 0.0]);
                return self.settle_bounce(spring_stiffness, dt);
            }
        }
        let [vx, vy] = self.velocity.get(); // px/s
        if vx.abs() > COAST_STOP_THRESHOLD || vy.abs() > COAST_STOP_THRESHOLD {
            let friction = match physics {
                ScrollPhysics::Momentum { friction } | ScrollPhysics::Bounce { friction, .. } => friction,
                _ => { self.velocity.set([0.0, 0.0]); return false; }
            };
            let dt = dt.max(0.0001);
            // Move by the real per-frame distance at the CURRENT velocity —
            // found via real on-device testing that applying the raw px/s
            // value directly (velocity as if it were "pixels this frame")
            // moved hundreds of pixels in a single frame instead of a smooth
            // coast; the headless test's large synthetic `dt` had masked
            // this unit mismatch.
            self.apply_momentum(vx * dt, vy * dt, physics);
            // Decay is exponential in REAL elapsed time, not a flat
            // per-call multiplier — `friction` is tuned as "per 1/60s
            // tick," so scale the exponent by dt. A flat `*= friction` per
            // `coast()` call (this function's first version) made total
            // coast distance depend on how often the caller happened to
            // call it — twice the calls per second decayed twice as fast
            // in real time — same exponential-ease shape `PaintCtx::
            // animate_to` already uses elsewhere for the same reason.
            let decay = friction.powf(dt / (1.0 / 60.0));
            let (nvx, nvy) = (vx * decay, vy * decay);
            self.velocity.set(if nvx.abs() < COAST_STOP_THRESHOLD && nvy.abs() < COAST_STOP_THRESHOLD { [0.0, 0.0] } else { [nvx, nvy] });
            return true;
        }
        if let ScrollPhysics::Bounce { spring_stiffness, .. } = physics {
            return self.settle_bounce(spring_stiffness, dt);
        }
        false
    }

    /// Hard-stops all coasting immediately and clamps the offset into
    /// bounds — used when animations are globally disabled, so release
    /// never coasts or bounces.
    pub fn stop_coasting(&self) {
        self.velocity.set([0.0, 0.0]);
        self.scroll_by(0.0, 0.0);
    }

    /// Applies a `(dx, dy)` step to the offset. Under `Bounce`, overscroll is
    /// allowed but resisted (35% magnitude) while already out of bounds and
    /// moving further out; moving back toward bounds is full-speed. Every
    /// other physics hard-clamps, identical to `scroll_by`.
    pub fn apply_momentum(&self, dx: f32, dy: f32, physics: ScrollPhysics) {
        let [ox, oy] = self.offset.get();
        let [cw, ch] = self.content_size.get();
        let [vw, vh] = self.viewport_size.get();
        let max_x = (cw - vw).max(0.0);
        let max_y = (ch - vh).max(0.0);
        match physics {
            ScrollPhysics::Bounce { .. } => {
                let nx = bounce_axis(ox, dx, max_x);
                let ny = bounce_axis(oy, dy, max_y);
                self.offset.set([nx, ny]);
            }
            _ => {
                let nx = (ox + dx).clamp(0.0, max_x);
                let ny = (oy + dy).clamp(0.0, max_y);
                self.offset.set([nx, ny]);
            }
        }
    }

    /// Eases an out-of-bounds offset back to the nearest valid bound —
    /// called once velocity has settled while `Bounce`-configured and still
    /// overscrolled. Same exponential-ease shape as `PaintCtx::animate_to`.
    /// Returns `true` while still settling (caller should keep requesting
    /// frames); `false` once within bounds (nothing left to do).
    pub fn settle_bounce(&self, spring_stiffness: f32, dt: f32) -> bool {
        let [ox, oy] = self.offset.get();
        let [cw, ch] = self.content_size.get();
        let [vw, vh] = self.viewport_size.get();
        let max_x = (cw - vw).max(0.0);
        let max_y = (ch - vh).max(0.0);
        let target_x = ox.clamp(0.0, max_x);
        let target_y = oy.clamp(0.0, max_y);
        if (ox - target_x).abs() < 0.5 && (oy - target_y).abs() < 0.5 {
            if ox != target_x || oy != target_y {
                self.offset.set([target_x, target_y]);
            }
            return false;
        }
        let alpha = 1.0 - (-dt * spring_stiffness).exp();
        let nx = ox + (target_x - ox) * alpha;
        let ny = oy + (target_y - oy) * alpha;
        self.offset.set([nx, ny]);
        true
    }
}

/// Rubber-band a single axis: resisted whenever a step would INCREASE the
/// overscroll magnitude (whether starting exactly at the bound or already
/// past it), full-speed whenever it would decrease it or stays in bounds.
/// Standalone free fn so it's directly unit-testable without an Atom.
/// Maximum overscroll distance past either edge under `Bounce` — matches
/// the ballpark of iOS's own `UIScrollView` bounce limit. Without a cap,
/// resistance (35% per step) only slows growth, it doesn't stop it — many
/// repeated wheel/momentum steps in the same direction could push the
/// offset arbitrarily far past the real content into blank space with no
/// visible edge to spring back from. Found via real trackpad testing (the
/// user scrolled into "some blank" past the end of the list), not
/// predicted up front.
const MAX_OVERSCROLL: f32 = 120.0;

fn bounce_axis(offset: f32, delta: f32, max: f32) -> f32 {
    let overscroll = |o: f32| if o < 0.0 { o } else if o > max { o - max } else { 0.0 };
    let before = overscroll(offset);
    let raw_next = offset + delta;
    let after_raw = overscroll(raw_next);
    let next = if after_raw.abs() > before.abs() {
        offset + delta * 0.35
    } else {
        raw_next
    };
    next.clamp(-MAX_OVERSCROLL, max + MAX_OVERSCROLL)
}

impl Default for ScrollController {
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

    fn controller_with_size(content_w: f32, content_h: f32, vp_w: f32, vp_h: f32) -> ScrollController {
        let c = ScrollController::new();
        c.content_size.set([content_w, content_h]);
        c.viewport_size.set([vp_w, vp_h]);
        c
    }

    #[test]
    fn scroll_by_clamps_to_bounds() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.scroll_by(9999.0, 9999.0);
        let [x, y] = c.offset();
        assert_eq!(x, 200.0); // max_x = 500 - 300
        assert_eq!(y, 400.0); // max_y = 800 - 400
    }

    #[test]
    fn scroll_by_negative_clamps_to_zero() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.scroll_by(100.0, 100.0);
        c.scroll_by(-9999.0, -9999.0);
        let [x, y] = c.offset();
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn scroll_to_top_sets_y_to_zero() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.scroll_by(50.0, 200.0);
        c.scroll_to_top();
        let [_x, y] = c.offset();
        assert_eq!(y, 0.0);
    }

    #[test]
    fn scroll_to_bottom_sets_y_to_max() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.scroll_to_bottom();
        let [_x, y] = c.offset();
        assert_eq!(y, 400.0); // 800 - 400
    }

    #[test]
    fn save_and_restore_position() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.scroll_by(50.0, 100.0);
        let pos = c.save_position();
        c.scroll_by(50.0, 100.0);
        c.restore_position(pos);
        assert_eq!(c.offset(), [50.0, 100.0]);
    }

    #[test]
    fn drag_delta_is_zero_on_first_call_then_real_deltas_after() {
        let c = ScrollController::new();
        assert_eq!(c.drag_delta(100.0, 50.0), (0.0, 0.0));
        assert_eq!(c.drag_delta(110.0, 45.0), (10.0, -5.0));
        assert_eq!(c.drag_delta(90.0, 45.0), (-20.0, 0.0));
    }

    #[test]
    fn end_drag_resets_so_the_next_drag_starts_fresh() {
        let c = ScrollController::new();
        c.drag_delta(100.0, 100.0);
        c.end_drag();
        assert_eq!(c.drag_delta(150.0, 120.0), (0.0, 0.0));
    }

    #[test]
    fn track_velocity_reflects_the_real_offset_speed() {
        let c = controller_with_size(500.0, 2000.0, 300.0, 400.0);
        c.scroll_by(0.0, 100.0);
        c.track_velocity(0.5); // 100px in 0.5s = 200px/s
        assert_eq!(c.velocity(), [0.0, 200.0]);
    }

    #[test]
    fn was_pressed_round_trips() {
        let c = ScrollController::new();
        assert!(!c.was_pressed());
        c.set_was_pressed(true);
        assert!(c.was_pressed());
    }

    #[test]
    fn apply_momentum_hard_clamps_under_momentum_physics() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.apply_momentum(9999.0, 9999.0, ScrollPhysics::Momentum { friction: 0.92 });
        assert_eq!(c.offset(), [200.0, 400.0]); // same hard bounds as scroll_by
    }

    #[test]
    fn apply_momentum_allows_resisted_overscroll_under_bounce() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        let physics = ScrollPhysics::Bounce { friction: 0.92, spring_stiffness: 12.0 };
        c.apply_momentum(0.0, -40.0, physics); // drag past the top edge
        let [_, y] = c.offset();
        assert!(y < 0.0, "overscroll must go negative under Bounce, got {y}");
        assert_eq!(y, -14.0, "resisted to 35% of the raw delta"); // -40 * 0.35
    }

    #[test]
    fn apply_momentum_moving_back_toward_bounds_is_not_resisted() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        let physics = ScrollPhysics::Bounce { friction: 0.92, spring_stiffness: 12.0 };
        c.apply_momentum(0.0, -40.0, physics); // overscroll to y = -14
        c.apply_momentum(0.0, 14.0, physics);  // moving back toward 0: full speed
        let [_, y] = c.offset();
        assert!((y - 0.0).abs() < 0.01, "expected to land back at 0, got {y}");
    }

    #[test]
    fn settle_bounce_eases_an_overscrolled_offset_back_to_the_bound() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.offset.set([0.0, -20.0]); // simulate an overscroll above the top
        let mut still_settling = true;
        for _ in 0..200 {
            still_settling = c.settle_bounce(12.0, 0.05);
            if !still_settling {
                break;
            }
        }
        assert!(!still_settling, "must eventually settle");
        assert_eq!(c.offset(), [0.0, 0.0]);
    }

    #[test]
    fn settle_bounce_is_a_no_op_when_already_in_bounds() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.offset.set([50.0, 100.0]);
        assert!(!c.settle_bounce(12.0, 0.05));
        assert_eq!(c.offset(), [50.0, 100.0]);
    }

    #[test]
    fn coast_springs_back_immediately_when_already_overscrolled_under_bounce_not_after_velocity_decays() {
        // Regression test for a real bug found via real trackpad testing +
        // direct calculation (not assumed): the first version of `coast`
        // wouldn't call `settle_bounce` until velocity decayed below the
        // 0.5 threshold — at friction=0.92 and a real ~400px/s release
        // velocity that's ~1.35s of the view sitting frozen mid-overscroll
        // before any spring-back motion began at all. Real platforms spring
        // back the instant the edge is crossed, independent of velocity.
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.offset.set([0.0, -60.0]); // already overscrolled above the top
        c.set_velocity([0.0, -400.0]); // still carrying a lot of speed
        let physics = ScrollPhysics::Bounce { friction: 0.92, spring_stiffness: 12.0 };

        let still_active = c.coast(physics, 1.0 / 60.0);

        assert!(still_active, "must still be settling, not yet at rest");
        let [_, y] = c.offset();
        assert!(
            y > -60.0,
            "must have started easing back toward the bound on the VERY FIRST call, not stayed frozen at -60 while velocity decays: got {y}"
        );
        assert_eq!(c.velocity(), [0.0, 0.0], "velocity is superseded by spring recovery once overscrolled");
    }

    #[test]
    fn set_velocity_clamps_to_max_velocity() {
        // Regression test for a real finding from live testing: an
        // unbounded velocity meant a very fast flick produced a
        // proportionally very long coast, feeling sluggish/stuck rather
        // than snappy. A very fast raw estimate must be capped.
        let c = ScrollController::new();
        c.set_velocity([0.0, 100_000.0]);
        assert_eq!(c.velocity(), [0.0, MAX_VELOCITY]);
        c.set_velocity([0.0, -100_000.0]);
        assert_eq!(c.velocity(), [0.0, -MAX_VELOCITY]);
    }

    #[test]
    fn track_velocity_clamps_to_max_velocity() {
        let c = controller_with_size(500.0, 100_000.0, 300.0, 400.0);
        c.scroll_by(0.0, 10_000.0); // a huge one-frame jump (not realistic input, just exercising the clamp)
        c.track_velocity(1.0 / 60.0); // would be 600_000 px/s unclamped
        assert_eq!(c.velocity(), [0.0, MAX_VELOCITY]);
    }

    #[test]
    fn full_realistic_velocity_range_settles_within_under_a_second() {
        // Confirms the tuned friction/threshold/clamp combination (0.88,
        // 15px/s, 2500px/s) actually delivers what the direct-calculation
        // analysis promised — every realistic release speed, including the
        // clamped maximum, settles in well under a second, not the
        // 1.2s-1.9s+ the original 0.92/0.5px/s combination measured out to.
        let physics = ScrollPhysics::Momentum { friction: 0.88 };
        for v0 in [200.0, 800.0, 2500.0, 100_000.0] {
            let c = controller_with_size(500.0, 1_000_000.0, 300.0, 400.0);
            c.set_velocity([0.0, v0]);
            let mut elapsed = 0.0;
            let dt = 1.0 / 60.0;
            while c.coast(physics, dt) && elapsed < 5.0 {
                elapsed += dt;
            }
            assert!(elapsed < 1.0, "v0={v0} took {elapsed:.2}s to settle, expected well under 1s");
        }
    }

    #[test]
    fn coast_applies_a_dt_scaled_step_not_the_raw_px_per_second_value() {
        // Regression test for a real bug found via on-device testing: velocity
        // is tracked in px/s, but `MomentumState`'s friction model is a
        // discrete per-tick decay expecting a per-frame pixel amount. The
        // first implementation applied the raw px/s value directly — at a
        // realistic 60fps dt this meant a single `coast()` call could jump
        // hundreds of pixels in one frame instead of a smooth step.
        let c = controller_with_size(500.0, 100_000.0, 300.0, 400.0);
        c.set_velocity([0.0, 800.0]); // a fast but real drag speed, px/s
        let dt = 1.0 / 60.0; // a realistic frame time, NOT a large synthetic one
        c.coast(ScrollPhysics::Momentum { friction: 0.92 }, dt);
        let [_, y] = c.offset();
        // At 800 px/s over one ~60fps frame, the real step is ~13.3px — a
        // step anywhere near the raw 800 value would mean the unit bug is
        // back.
        assert!(y < 50.0, "one frame of coast at 800px/s, dt=1/60 must move roughly 13px, not the raw velocity, got {y}");
        assert!(y > 0.0, "must still move forward some real amount, got {y}");
    }

    #[test]
    fn coast_velocity_decay_is_dt_independent_over_a_fixed_time_span() {
        // The dt-scaling fix must not make total coast distance depend on
        // how finely the frames are chopped up — half as much movement per
        // tick, twice as many ticks over the same wall-clock time, should
        // land at roughly the same total distance.
        let physics = ScrollPhysics::Momentum { friction: 0.92 };
        let coarse = controller_with_size(500.0, 100_000.0, 300.0, 400.0);
        coarse.set_velocity([0.0, 600.0]);
        for _ in 0..30 {
            coarse.coast(physics, 1.0 / 30.0); // 1 real second, 30 ticks
        }

        let fine = controller_with_size(500.0, 100_000.0, 300.0, 400.0);
        fine.set_velocity([0.0, 600.0]);
        for _ in 0..60 {
            fine.coast(physics, 1.0 / 60.0); // 1 real second, 60 ticks
        }

        let [_, y_coarse] = coarse.offset();
        let [_, y_fine] = fine.offset();
        let diff = (y_coarse - y_fine).abs();
        assert!(
            diff < y_coarse.max(y_fine) * 0.15,
            "total coast distance over the same real time must be roughly frame-rate independent: coarse={y_coarse} fine={y_fine}"
        );
    }

    #[test]
    fn wheel_recently_active_is_true_immediately_after_marking_then_false_once_the_grace_period_elapses() {
        let c = ScrollController::new();
        assert!(!c.wheel_recently_active(), "nothing marked yet");
        c.mark_wheel_active();
        assert!(c.wheel_recently_active(), "must report recently active right after marking");
        // Advance in small steps (mirrors real per-frame calls), same
        // total as slightly more than the grace period.
        for _ in 0..20 {
            c.advance_wheel_idle(WHEEL_IDLE_GRACE / 10.0);
        }
        assert!(!c.wheel_recently_active(), "must go stale once the grace period has elapsed");
    }

    #[test]
    fn wheel_recently_active_survives_a_gap_shorter_than_the_grace_period() {
        // The exact bug this replaced: a single frame with no wheel event
        // must NOT immediately flip this to false — only a real gap of at
        // least WHEEL_IDLE_GRACE seconds should.
        let c = ScrollController::new();
        c.mark_wheel_active();
        c.advance_wheel_idle(WHEEL_IDLE_GRACE * 0.3); // a short gap, e.g. one uneven frame
        assert!(c.wheel_recently_active(), "a short gap within the grace period must not reset activity");
    }
}
