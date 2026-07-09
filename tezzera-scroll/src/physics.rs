/// The axis or axes along which a [`ScrollView`] scrolls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDirection {
    Vertical,
    Horizontal,
    Both,
}

/// Physics model that governs how a [`ScrollView`] responds to input and decelerates.
#[derive(Debug, Clone, Copy)]
pub enum ScrollPhysics {
    /// Natural momentum with friction decay. `friction` in (0.0, 1.0) — 0.92 is a natural feel.
    Momentum { friction: f32 },
    /// Stops immediately on release.
    Clamped,
    /// Snaps to page boundaries.
    Paged { page_size: f32 },
    /// Momentum with rubber-band overscroll (D108/Phase 26 Step 2) — content
    /// can be dragged past its edge (resisted) and springs back once
    /// released, the iOS scroll feel. `friction` decays velocity same as
    /// [`ScrollPhysics::Momentum`]; `spring_stiffness` governs how quickly
    /// an out-of-bounds offset eases back to the nearest bound once
    /// velocity has settled (see `ScrollController::settle_bounce`) — an
    /// exponential ease, the same shape `PaintCtx::animate_to` already uses
    /// elsewhere in this codebase, not a full mass-spring simulation.
    Bounce { friction: f32, spring_stiffness: f32 },
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        ScrollPhysics::Momentum { friction: 0.92 }
    }
}

/// Per-widget-type default physics, keyed by platform (D108/Phase 26 Step 2).
/// This is the ONLY place platform is consulted for scroll behavior — one
/// pure lookup, never branches scattered through widget code — and it is
/// always the lowest-priority source: an app's own theme `ext` value or an
/// explicit `.physics(...)` on a `ScrollView` both override it. See
/// `tezzera-widgets/src/tree/scroll_view.rs`'s `resolve_physics`.
#[derive(Debug, Clone, Copy)]
pub struct ScrollStyle {
    pub physics: ScrollPhysics,
}

impl ScrollStyle {
    /// iOS/macOS default to rubber-band `Bounce` (the platform-native feel);
    /// every other platform defaults to plain `Momentum`. Android's overscroll
    /// "glow" is a separate visual effect on similar physics, not modeled
    /// here — out of scope (see `.steering/PHASE_26.md`).
    pub fn default_for_platform(platform: tezzera_core::Platform) -> ScrollPhysics {
        // friction=0.88 (not the earlier 0.92) — 0.92 measured out to a
        // 1.2s-1.9s coast tail even for realistic release speeds (confirmed
        // by direct calculation during real trackpad testing), which read
        // as sluggish/stuck rather than a natural decelerating glide.
        // Combined with `COAST_STOP_THRESHOLD`/`MAX_VELOCITY`, 0.88 brings
        // the full realistic range down to ~0.35s-0.7s.
        match platform {
            tezzera_core::Platform::Ios | tezzera_core::Platform::MacOs => {
                ScrollPhysics::Bounce { friction: 0.88, spring_stiffness: 12.0 }
            }
            _ => ScrollPhysics::Momentum { friction: 0.88 },
        }
    }
}

/// Per-frame simulation state for momentum scrolling.
pub struct MomentumState {
    pub velocity_x: f32,
    pub velocity_y: f32,
}

impl MomentumState {
    pub fn new() -> Self {
        Self {
            velocity_x: 0.0,
            velocity_y: 0.0,
        }
    }

    /// Apply a drag delta to velocity (called on pointer move).
    pub fn push(&mut self, dx: f32, dy: f32) {
        self.velocity_x = dx;
        self.velocity_y = dy;
    }

    /// Advance momentum simulation by one frame. Returns `(dx, dy)` to apply to offset.
    pub fn tick(&mut self, physics: ScrollPhysics) -> (f32, f32) {
        match physics {
            ScrollPhysics::Clamped => {
                let out = (self.velocity_x, self.velocity_y);
                self.velocity_x = 0.0;
                self.velocity_y = 0.0;
                out
            }
            ScrollPhysics::Momentum { friction } | ScrollPhysics::Bounce { friction, .. } => {
                let out = (self.velocity_x, self.velocity_y);
                self.velocity_x *= friction;
                self.velocity_y *= friction;
                // Stop tiny residual motion.
                if self.velocity_x.abs() < 0.5 {
                    self.velocity_x = 0.0;
                }
                if self.velocity_y.abs() < 0.5 {
                    self.velocity_y = 0.0;
                }
                out
            }
            ScrollPhysics::Paged { page_size: _ } => {
                // Snap: return remaining distance toward nearest page boundary.
                let out = (self.velocity_x, self.velocity_y);
                self.velocity_x = 0.0;
                self.velocity_y = 0.0;
                out
            }
        }
    }

    /// Returns `true` when both velocity components are below the stop threshold.
    pub fn is_settled(&self) -> bool {
        self.velocity_x.abs() < 0.5 && self.velocity_y.abs() < 0.5
    }
}

impl Default for MomentumState {
    fn default() -> Self {
        Self::new()
    }
}

/// Clamp an offset so it stays within valid scroll bounds.
pub fn clamp_offset(offset: [f32; 2], content: [f32; 2], viewport: [f32; 2]) -> [f32; 2] {
    let max_x = (content[0] - viewport[0]).max(0.0);
    let max_y = (content[1] - viewport[1]).max(0.0);
    [offset[0].clamp(0.0, max_x), offset[1].clamp(0.0, max_y)]
}

/// Snap `offset` to the nearest multiple of `page_size`.
pub fn snap_to_page(offset: f32, page_size: f32) -> f32 {
    (offset / page_size).round() * page_size
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_offset_returns_zero_when_content_fits_viewport() {
        let result = clamp_offset([5.0, 10.0], [100.0, 200.0], [150.0, 250.0]);
        assert_eq!(result, [0.0, 0.0]);
    }

    #[test]
    fn clamp_offset_keeps_offset_within_bounds() {
        // content 500x800, viewport 300x400 → max_x=200, max_y=400
        let result = clamp_offset([250.0, 450.0], [500.0, 800.0], [300.0, 400.0]);
        assert_eq!(result[0], 200.0);
        assert_eq!(result[1], 400.0);
    }

    #[test]
    fn clamp_offset_allows_valid_offset() {
        let result = clamp_offset([50.0, 100.0], [500.0, 800.0], [300.0, 400.0]);
        assert_eq!(result[0], 50.0);
        assert_eq!(result[1], 100.0);
    }

    #[test]
    fn snap_to_page_rounds_to_nearest_boundary() {
        assert_eq!(snap_to_page(260.0, 200.0), 200.0);
        assert_eq!(snap_to_page(350.0, 200.0), 400.0);
        assert_eq!(snap_to_page(0.0, 200.0), 0.0);
    }

    #[test]
    fn momentum_state_tick_clamped_zeroes_velocity() {
        let mut state = MomentumState::new();
        state.push(50.0, 30.0);
        let (dx, dy) = state.tick(ScrollPhysics::Clamped);
        assert_eq!(dx, 50.0);
        assert_eq!(dy, 30.0);
        assert_eq!(state.velocity_x, 0.0);
        assert_eq!(state.velocity_y, 0.0);
    }

    #[test]
    fn momentum_state_tick_momentum_decays_velocity() {
        let mut state = MomentumState::new();
        state.push(100.0, 80.0);
        state.tick(ScrollPhysics::Momentum { friction: 0.92 });
        assert!((state.velocity_x - 92.0).abs() < 0.01);
        assert!((state.velocity_y - 73.6).abs() < 0.01);
    }

    #[test]
    fn momentum_state_tick_momentum_stops_tiny_residual() {
        let mut state = MomentumState::new();
        state.velocity_x = 0.3;
        state.velocity_y = 0.4;
        state.tick(ScrollPhysics::Momentum { friction: 0.92 });
        assert_eq!(state.velocity_x, 0.0);
        assert_eq!(state.velocity_y, 0.0);
    }

    #[test]
    fn momentum_state_is_settled_when_both_below_threshold() {
        let mut state = MomentumState::new();
        assert!(state.is_settled());
        state.push(10.0, 5.0);
        assert!(!state.is_settled());
        state.velocity_x = 0.4;
        state.velocity_y = 0.4;
        assert!(state.is_settled());
    }

    #[test]
    fn momentum_state_tick_bounce_decays_velocity_same_as_momentum() {
        let mut state = MomentumState::new();
        state.push(100.0, 80.0);
        state.tick(ScrollPhysics::Bounce { friction: 0.92, spring_stiffness: 12.0 });
        assert!((state.velocity_x - 92.0).abs() < 0.01);
        assert!((state.velocity_y - 73.6).abs() < 0.01);
    }

    #[test]
    fn default_for_platform_is_bounce_on_ios_and_macos() {
        assert!(matches!(
            ScrollStyle::default_for_platform(tezzera_core::Platform::Ios),
            ScrollPhysics::Bounce { .. }
        ));
        assert!(matches!(
            ScrollStyle::default_for_platform(tezzera_core::Platform::MacOs),
            ScrollPhysics::Bounce { .. }
        ));
    }

    #[test]
    fn default_for_platform_is_momentum_elsewhere() {
        for p in [
            tezzera_core::Platform::Android,
            tezzera_core::Platform::Windows,
            tezzera_core::Platform::Linux,
            tezzera_core::Platform::Web,
        ] {
            assert!(matches!(ScrollStyle::default_for_platform(p), ScrollPhysics::Momentum { .. }));
        }
    }
}
