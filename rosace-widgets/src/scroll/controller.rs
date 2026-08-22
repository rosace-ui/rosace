use std::sync::{Arc, Mutex};
use crate::scroll::physics::ScrollPhysics;

/// Controls a [`ScrollView`] programmatically.
///
/// All clones share the same underlying atoms so that separate handles can
/// observe and mutate the scroll position from different call sites.

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

/// Everything one scroll view is tracking, behind a single lock.
///
/// This was nine separate `Atom`s. Six of them were documented as
/// deliberately NOT subscribed to anything — they were reached for purely as
/// a `Clone + Send + Sync` cell, because `paint(&self)` cannot mutate and the
/// controller is cloned into callbacks, and `Atom` was the only such
/// primitive available when this was written. `mark_node_dirty` and
/// `widget_state` did not exist yet.
///
/// The three that WERE subscribed had a worse problem: `Atom` notifies
/// COMPONENTS, and there is exactly one component in the tree — so every
/// wheel notch dirtied the root, re-ran `build()`, and made the frame
/// structural, which disables every per-node cache in the framework.
#[derive(Default)]
struct ScrollState {
    offset: [f32; 2],
    content_size: [f32; 2],
    viewport_size: [f32; 2],
    /// Absolute screen point of the last streamed drag position, `None` when
    /// not currently dragging.
    last_drag_point: Option<[f32; 2]>,
    /// The drag's DOWN point, kept until [`ScrollController::DRAG_SLOP`] is
    /// exceeded.
    drag_origin: Option<[f32; 2]>,
    /// The real, currently-tracked drag/momentum velocity in px/s — computed
    /// from the actual offset delta each frame, never a fixed constant.
    velocity: [f32; 2],
    /// `offset` as of the last frame, used to derive `velocity` this frame.
    last_offset_for_velocity: [f32; 2],
    /// Whether this controller was pressed as of the last frame — detects the
    /// true→false transition that seeds momentum from the tracked velocity.
    was_pressed: bool,
    /// Real elapsed seconds since the last wheel/trackpad event. A duration
    /// rather than a per-frame flag because wheel events do not arrive one
    /// per frame — a flag sprang back the instant a single frame had no fresh
    /// event and produced visible jitter (found on a real trackpad).
    wheel_idle_time: f32,
    /// Repaint hook, installed by `PaintCtx::scroll_controller`. Marks the
    /// owning NODE — not a component.
    on_invalidate: Option<Arc<dyn Fn() + Send + Sync>>,
    /// App-facing notification, installed by [`ScrollController::on_scroll`].
    on_scroll: Option<Arc<dyn Fn([f32; 2]) + Send + Sync>>,
}

/// Scroll position and physics for one scrollable region.
///
/// All clones share the same state, so separate handles can observe and drive
/// the same scroll position.
#[derive(Clone)]
pub struct ScrollController(Arc<Mutex<ScrollState>>);

impl ScrollController {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(ScrollState {
            wheel_idle_time: f32::MAX,
            ..Default::default()
        })))
    }

    fn s(&self) -> std::sync::MutexGuard<'_, ScrollState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Called with `[x, y]` whenever the offset CHANGES.
    ///
    /// Change-gated deliberately: momentum decay writes the offset every
    /// frame, and once it settles those writes are all the same value. Firing
    /// on every write would call this 60×/s forever on a stationary list.
    ///
    /// During a real fling the offset genuinely does change each frame, so
    /// this genuinely fires each frame. Read it and render from it; storing it
    /// into reactive state re-dirties the app every frame, which is the cost
    /// this whole controller exists to avoid.
    pub fn on_scroll(&self, f: impl Fn([f32; 2]) + Send + Sync + 'static) {
        self.s().on_scroll = Some(Arc::new(f));
    }

    /// Set the offset with NO clamping — for widgets that drive the offset
    /// themselves and have already decided what is valid (`Carousel` snapping
    /// to a page, `Dismissible` tracking a swipe, `TextArea` following its
    /// caret). [`Self::scroll_to`] is the clamped, app-facing form.
    pub fn scroll_to_raw(&self, v: [f32; 2]) { self.set_offset(v); }

    /// Content extent as last measured by the scroll view.
    pub fn content_size(&self) -> [f32; 2] { self.s().content_size }
    /// Viewport extent as last measured by the scroll view.
    pub fn viewport_size(&self) -> [f32; 2] { self.s().viewport_size }
    /// Published by the scroll view each paint.
    pub fn set_content_size(&self, v: [f32; 2]) { self.set_measured(Some(v), None); }
    /// Published by the scroll view each paint.
    pub fn set_viewport_size(&self, v: [f32; 2]) { self.set_measured(None, Some(v)); }

    /// Repaint hook — see [`ScrollState::on_invalidate`].
    pub(crate) fn on_invalidate(&self, f: impl Fn() + Send + Sync + 'static) {
        self.s().on_invalidate = Some(Arc::new(f));
    }

    /// The single write path for `offset`: clamps nothing, dedups equal
    /// writes, and notifies.
    fn set_offset(&self, v: [f32; 2]) {
        let (invalidate, notify) = {
            let mut st = self.s();
            if st.offset == v {
                return;
            }
            st.offset = v;
            (st.on_invalidate.clone(), st.on_scroll.clone())
        };
        // Callbacks run OUTSIDE the lock: an app handler that reads the
        // controller back would otherwise deadlock on its own notification.
        if let Some(f) = invalidate { f(); }
        if let Some(f) = notify { f(v); }
    }

    fn set_measured(&self, content: Option<[f32; 2]>, viewport: Option<[f32; 2]>) {
        let invalidate = {
            let mut st = self.s();
            let mut changed = false;
            if let Some(c) = content {
                if st.content_size != c { st.content_size = c; changed = true; }
            }
            if let Some(v) = viewport {
                if st.viewport_size != v { st.viewport_size = v; changed = true; }
            }
            if !changed { return; }
            st.on_invalidate.clone()
        };
        if let Some(f) = invalidate { f(); }
    }

    /// Jump to an absolute position, clamped to valid bounds.
    pub fn scroll_to(&self, x: f32, y: f32) {
        let [cw, ch] = self.s().content_size;
        let [vw, vh] = self.s().viewport_size;
        let nx = x.clamp(0.0, (cw - vw).max(0.0));
        let ny = y.clamp(0.0, (ch - vh).max(0.0));
        self.set_offset([nx, ny]);
    }

    /// Scroll to the top (y = 0), preserving x.
    pub fn scroll_to_top(&self) {
        let [x, _] = self.s().offset;
        self.set_offset([x, 0.0]);
    }

    /// Scroll to the bottom (y = content_height − viewport_height), preserving x.
    pub fn scroll_to_bottom(&self) {
        let [x, _] = self.s().offset;
        let [_, ch] = self.s().content_size;
        let [_, vh] = self.s().viewport_size;
        self.set_offset([x, (ch - vh).max(0.0)]);
    }

    /// Add `(dx, dy)` to the current offset, clamped to valid bounds.
    pub fn scroll_by(&self, dx: f32, dy: f32) {
        let [ox, oy] = self.s().offset;
        let [cw, ch] = self.s().content_size;
        let [vw, vh] = self.s().viewport_size;
        let new_x = (ox + dx).clamp(0.0, (cw - vw).max(0.0));
        let new_y = (oy + dy).clamp(0.0, (ch - vh).max(0.0));
        self.set_offset([new_x, new_y]);
    }

    /// Returns the current `[offset_x, offset_y]`.
    pub fn offset(&self) -> [f32; 2] {
        self.s().offset
    }

    /// Snapshot the current position for later restoration.
    pub fn save_position(&self) -> [f32; 2] {
        self.s().offset
    }

    /// Restore a previously saved position.
    pub fn restore_position(&self, pos: [f32; 2]) {
        self.set_offset(pos);
    }

    // ── Drag-to-pan + momentum (D108/Phase 26 Step 2) ──────────────────────

    /// Drag slop (Phase 32 bug fix, user-reported): a press must travel
    /// this many logical px from its DOWN point before drag-to-pan
    /// engages. Without it, the 1-3 px of natural pointer jitter during a
    /// plain click pans the view — visible whenever the click lands on
    /// non-interactive content inside a scroll view (a hit falls through
    /// to the viewport's positional drag region). 6 px matches the common
    /// touch-slop convention (small enough that intentional drags feel
    /// instant, large enough that clicks never pan).
    pub const DRAG_SLOP: f32 = 6.0;

    /// Streamed absolute drag position → delta since the last call.
    /// Returns (0, 0) on the first call of a drag AND while the pointer
    /// stays within [`Self::DRAG_SLOP`] of the down point — see its doc.
    /// Call `end_drag` on release so the next drag starts fresh.
    pub fn drag_delta(&self, x: f32, y: f32) -> (f32, f32) {
        let prev = self.s().last_drag_point;
        self.s().last_drag_point = Some([x, y]);
        let Some([px, py]) = prev else {
            self.s().drag_origin = Some([x, y]);
            return (0.0, 0.0);
        };
        // Read the origin and RELEASE the lock before doing anything else.
        // `if let Some(..) = self.s().drag_origin { .. }` holds the guard for
        // the whole body in edition 2021, so the write below deadlocked
        // against the read that opened the block.
        let origin = self.s().drag_origin;
        if let Some([ox, oy]) = origin {
            if (x - ox).hypot(y - oy) <= Self::DRAG_SLOP {
                return (0.0, 0.0); // still a click, not a drag
            }
            self.s().drag_origin = None; // slop exceeded — drag is real
        }
        (x - px, y - py)
    }

    /// Clears drag-position tracking — call on release so the next drag
    /// doesn't diff against a stale point.
    pub fn end_drag(&self) {
        self.s().last_drag_point = None;
        self.s().drag_origin = None;
    }

    /// Recomputes `velocity` from the real offset delta since the last call,
    /// in px/s — the actual measured drag/momentum speed, never an assumed
    /// constant. Call once per frame while dragging or coasting. Clamped to
    /// `MAX_VELOCITY` — see its doc comment for why.
    pub fn track_velocity(&self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let now = self.s().offset;
        let prev = self.s().last_offset_for_velocity;
        let vx = ((now[0] - prev[0]) / dt).clamp(-MAX_VELOCITY, MAX_VELOCITY);
        let vy = ((now[1] - prev[1]) / dt).clamp(-MAX_VELOCITY, MAX_VELOCITY);
        self.s().velocity = [vx, vy];
        self.s().last_offset_for_velocity = now;
    }

    /// The most recently tracked velocity (px/s) — see `track_velocity`.
    pub fn velocity(&self) -> [f32; 2] {
        self.s().velocity
    }

    /// Sets the tracked velocity directly (px/s) — for input sources that
    /// aren't a continuous drag `track_velocity` can measure frame-to-frame
    /// (e.g. a discrete wheel/trackpad event), so `coast` still has a real
    /// speed to decay from once the events stop arriving. Clamped to
    /// `MAX_VELOCITY` — see its doc comment for why.
    pub fn set_velocity(&self, v: [f32; 2]) {
        self.s().velocity = [v[0].clamp(-MAX_VELOCITY, MAX_VELOCITY), v[1].clamp(-MAX_VELOCITY, MAX_VELOCITY)];
    }

    /// Whether this controller was `pressed` as of the last frame — used to
    /// detect the true→false transition that hands off to momentum.
    pub fn was_pressed(&self) -> bool {
        self.s().was_pressed
    }

    pub fn set_was_pressed(&self, v: bool) {
        self.s().was_pressed = v;
    }

    /// Called by a wheel/trackpad scroll callback when it fires — resets
    /// the idle clock to 0.
    pub fn mark_wheel_active(&self) {
        self.s().wheel_idle_time = 0.0;
    }

    /// Advances the wheel-idle clock by one real frame — call once per
    /// frame regardless of whether a wheel event landed.
    pub fn advance_wheel_idle(&self, dt: f32) {
        let t = self.s().wheel_idle_time;
        if t < f32::MAX / 2.0 {
            self.s().wheel_idle_time = t + dt;
        }
    }

    /// Whether a wheel/trackpad event landed within the last
    /// [`WHEEL_IDLE_GRACE`] real seconds — the caller uses this to hold off
    /// `coast`'s momentum/spring-back until the gesture has genuinely
    /// stopped, not just "no event in this exact frame" (see
    /// `wheel_idle_time`'s doc comment for why a single-frame check jittered).
    pub fn wheel_recently_active(&self) -> bool {
        self.s().wheel_idle_time < WHEEL_IDLE_GRACE
    }

    /// Current drag/momentum speed (px/s), for callers that just need "is
    /// this still visibly moving" (e.g. an auto-hiding scrollbar) without
    /// caring about direction. Zero once `coast` has fully settled.
    pub fn velocity_magnitude(&self) -> f32 {
        let [vx, vy] = self.s().velocity;
        (vx * vx + vy * vy).sqrt()
    }

    /// Whether the current offset sits past either bound — used by `coast`
    /// and by callers that need to keep a `Bounce` spring recovering even
    /// while something else (e.g. a still-live wheel-idle gate) is holding
    /// off the rest of `coast`'s own logic.
    pub fn is_overscrolled(&self) -> bool {
        let [ox, oy] = self.s().offset;
        let [cw, ch] = self.s().content_size;
        let [vw, vh] = self.s().viewport_size;
        let max_x = (cw - vw).max(0.0);
        let max_y = (ch - vh).max(0.0);
        ox < 0.0 || ox > max_x || oy < 0.0 || oy > max_y
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
            if self.is_overscrolled() {
                self.s().velocity = [0.0, 0.0];
                return self.settle_bounce(spring_stiffness, dt);
            }
        }
        let [vx, vy] = self.s().velocity; // px/s
        if vx.abs() > COAST_STOP_THRESHOLD || vy.abs() > COAST_STOP_THRESHOLD {
            let friction = match physics {
                ScrollPhysics::Momentum { friction } | ScrollPhysics::Bounce { friction, .. } => friction,
                _ => { self.s().velocity = [0.0, 0.0]; return false; }
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
            self.s().velocity = if nvx.abs() < COAST_STOP_THRESHOLD && nvy.abs() < COAST_STOP_THRESHOLD { [0.0, 0.0] } else { [nvx, nvy] };
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
        self.s().velocity = [0.0, 0.0];
        self.scroll_by(0.0, 0.0);
    }

    /// Applies a `(dx, dy)` step to the offset. Under `Bounce`, overscroll is
    /// allowed but resisted (35% magnitude) while already out of bounds and
    /// moving further out; moving back toward bounds is full-speed. Every
    /// other physics hard-clamps, identical to `scroll_by`.
    pub fn apply_momentum(&self, dx: f32, dy: f32, physics: ScrollPhysics) {
        let [ox, oy] = self.s().offset;
        let [cw, ch] = self.s().content_size;
        let [vw, vh] = self.s().viewport_size;
        let max_x = (cw - vw).max(0.0);
        let max_y = (ch - vh).max(0.0);
        match physics {
            ScrollPhysics::Bounce { .. } => {
                let nx = bounce_axis(ox, dx, max_x);
                let ny = bounce_axis(oy, dy, max_y);
                self.set_offset([nx, ny]);
            }
            _ => {
                let nx = (ox + dx).clamp(0.0, max_x);
                let ny = (oy + dy).clamp(0.0, max_y);
                self.set_offset([nx, ny]);
            }
        }
    }

    /// Like [`Self::apply_momentum`], but reports whether the offset
    /// actually moved — `false` means this scroll is already fully
    /// exhausted in this exact direction (hard-clamped with nothing left,
    /// or already stretched to `MAX_OVERSCROLL` under `Bounce`) and the
    /// delta was NOT applied at all. Callers driving nested scroll
    /// chains (an inner `ScrollView` sitting inside an outer one) use
    /// this to decide whether to also offer the same delta to an
    /// enclosing scrollable ancestor: keep walking outward until one
    /// reports `true`, or the chain runs out.
    pub fn try_apply_delta(&self, dx: f32, dy: f32, physics: ScrollPhysics) -> bool {
        let before = self.s().offset;
        self.apply_momentum(dx, dy, physics);
        self.s().offset != before
    }

    /// Eases an out-of-bounds offset back to the nearest valid bound —
    /// called once velocity has settled while `Bounce`-configured and still
    /// overscrolled. Same exponential-ease shape as `PaintCtx::animate_to`.
    /// Returns `true` while still settling (caller should keep requesting
    /// frames); `false` once within bounds (nothing left to do).
    pub fn settle_bounce(&self, spring_stiffness: f32, dt: f32) -> bool {
        let [ox, oy] = self.s().offset;
        let [cw, ch] = self.s().content_size;
        let [vw, vh] = self.s().viewport_size;
        let max_x = (cw - vw).max(0.0);
        let max_y = (ch - vh).max(0.0);
        let target_x = ox.clamp(0.0, max_x);
        let target_y = oy.clamp(0.0, max_y);
        if (ox - target_x).abs() < 0.5 && (oy - target_y).abs() < 0.5 {
            if ox != target_x || oy != target_y {
                self.set_offset([target_x, target_y]);
            }
            return false;
        }
        let alpha = 1.0 - (-dt * spring_stiffness).exp();
        let nx = ox + (target_x - ox) * alpha;
        let ny = oy + (target_y - oy) * alpha;
        self.set_offset([nx, ny]);
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

/// Where a revealed child should sit in the viewport.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScrollAlign {
    /// Scroll the minimum distance to bring it fully into view, and do
    /// nothing if it already is. What "reveal the field with the error"
    /// means, and the right default.
    #[default]
    Nearest,
    /// Align the child's leading edge with the viewport's.
    Start,
    /// Centre it.
    Center,
    /// Align trailing edges.
    End,
}

impl ScrollController {
    /// Scroll so that `child` — a rect in CONTENT space — sits in view.
    ///
    /// Returns the offset it moved to, or `None` when the viewport has not
    /// been measured yet (nothing has painted, so there is no viewport to
    /// reveal into) — deliberately not a silent scroll to zero.
    pub fn reveal(&self, child: rosace_core::types::Rect, align: ScrollAlign) -> Option<[f32; 2]> {
        let vp = self.viewport_size();
        if vp[0] <= 0.0 || vp[1] <= 0.0 {
            return None;
        }
        let cur = self.offset();
        let axis = |lead: f32, extent: f32, view: f32, cur: f32| -> f32 {
            match align {
                ScrollAlign::Start  => lead,
                ScrollAlign::End    => lead + extent - view,
                ScrollAlign::Center => lead + (extent - view) / 2.0,
                ScrollAlign::Nearest => {
                    if lead < cur {
                        lead                       // off the leading edge
                    } else if lead + extent > cur + view {
                        lead + extent - view       // off the trailing edge
                    } else {
                        cur                        // already fully visible
                    }
                }
            }
        };
        let x = axis(child.origin.x, child.size.width, vp[0], cur[0]);
        let y = axis(child.origin.y, child.size.height, vp[1], cur[1]);
        self.scroll_to(x, y);
        Some(self.offset())
    }
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
        c.set_content_size([content_w, content_h]);
        c.set_viewport_size([vp_w, vp_h]);
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
    fn click_jitter_within_slop_never_pans() {
        // The user-reported Phase 32 bug: a plain click's 1-3px pointer
        // jitter must not move the view.
        let c = ScrollController::new();
        assert_eq!(c.drag_delta(100.0, 100.0), (0.0, 0.0)); // down
        assert_eq!(c.drag_delta(102.0, 101.0), (0.0, 0.0)); // jitter
        assert_eq!(c.drag_delta(99.0, 100.0), (0.0, 0.0));  // jitter back
        // A real drag past the slop engages, diffing from the last point.
        assert_eq!(c.drag_delta(120.0, 100.0), (21.0, 0.0));
        // And keeps streaming normally afterwards.
        assert_eq!(c.drag_delta(125.0, 104.0), (5.0, 4.0));
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
    fn try_apply_delta_reports_true_while_room_remains() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        let moved = c.try_apply_delta(0.0, 50.0, ScrollPhysics::Momentum { friction: 0.92 });
        assert!(moved, "there's 400px of room (max_y=400), a 50px step must move it");
        assert_eq!(c.offset(), [0.0, 50.0]);
    }

    #[test]
    fn try_apply_delta_reports_false_once_hard_clamped_and_exhausted() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.scroll_by(0.0, 400.0); // already at max_y
        let moved = c.try_apply_delta(0.0, 50.0, ScrollPhysics::Momentum { friction: 0.92 });
        assert!(!moved, "already at the hard bound with no Bounce give — nothing left to absorb");
        assert_eq!(c.offset(), [0.0, 400.0], "the declined delta must not have been applied");
    }

    #[test]
    fn try_apply_delta_still_reports_true_for_resisted_bounce_overscroll() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        c.scroll_by(0.0, 400.0); // already at max_y
        let physics = ScrollPhysics::Bounce { friction: 0.92, spring_stiffness: 12.0 };
        let moved = c.try_apply_delta(0.0, 50.0, physics);
        assert!(moved, "Bounce still has overscroll room even at the hard bound — must consume it");
        assert!(c.offset()[1] > 400.0, "must have stretched past the hard bound, got {:?}", c.offset());
    }

    #[test]
    fn try_apply_delta_reports_false_once_bounce_overscroll_is_also_maxed_out() {
        let c = controller_with_size(500.0, 800.0, 300.0, 400.0);
        let physics = ScrollPhysics::Bounce { friction: 0.92, spring_stiffness: 12.0 };
        // Push well past MAX_OVERSCROLL with repeated large deltas.
        for _ in 0..50 {
            c.try_apply_delta(0.0, 500.0, physics);
        }
        let before = c.offset();
        let moved = c.try_apply_delta(0.0, 500.0, physics);
        assert!(!moved, "fully stretched to MAX_OVERSCROLL — genuinely exhausted, must decline");
        assert_eq!(c.offset(), before);
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
        c.scroll_to_raw([0.0, -20.0]); // simulate an overscroll above the top
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
        c.scroll_to_raw([50.0, 100.0]);
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
        c.scroll_to_raw([0.0, -60.0]); // already overscrolled above the top
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
