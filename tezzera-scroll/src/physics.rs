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
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        ScrollPhysics::Momentum { friction: 0.92 }
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
            ScrollPhysics::Momentum { friction } => {
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
}
