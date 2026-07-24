//! Non-reactive drag/coast state for `InteractiveViewer` (Phase 32), keyed by
//! render-tree node id — the same shape as [`crate::scroll_offset`] and for
//! the same reason: this state must survive across widget REBUILDS (a
//! momentum coast needs several frames to decay, each driven by
//! `request_animation`, and each of THOSE frames constructs a brand-new
//! `InteractiveViewer` value — per-instance fields would be wiped every
//! time). Keying by node id instead of living on the widget value is what
//! makes it actually persistent.
//!
//! Main-thread only (event dispatch + present both run there), so a
//! thread-local map suffices — mirrors `scroll_offset`'s own registries.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

thread_local! {
    /// Last absolute drag point + when it was observed (real wall-clock
    /// time, NOT the animation frame clock — a drag doesn't necessarily
    /// repaint between moves, so there's no per-frame dt to lean on here).
    static DRAG_LAST: RefCell<HashMap<u64, (f32, f32, Instant)>> = RefCell::new(HashMap::new());
    /// Current coast velocity, content-space px per real second.
    static VELOCITY: RefCell<HashMap<u64, (f32, f32)>> = RefCell::new(HashMap::new());
}

/// The last observed absolute drag point + its timestamp, if a drag is (or
/// was) in progress for this node.
pub fn drag_last(id: u64) -> Option<(f32, f32, Instant)> {
    DRAG_LAST.with(|m| m.borrow().get(&id).copied())
}

pub fn set_drag_last(id: u64, point: Option<(f32, f32)>) {
    DRAG_LAST.with(|m| {
        let mut m = m.borrow_mut();
        match point {
            Some((x, y)) => { m.insert(id, (x, y, Instant::now())); }
            None => { m.remove(&id); }
        }
    });
}

/// Current coast velocity (content-space px/real-second), `(0.0, 0.0)` if
/// never set or already settled.
pub fn pan_velocity(id: u64) -> (f32, f32) {
    VELOCITY.with(|m| m.borrow().get(&id).copied().unwrap_or((0.0, 0.0)))
}

pub fn set_pan_velocity(id: u64, v: (f32, f32)) {
    VELOCITY.with(|m| { m.borrow_mut().insert(id, v); });
}

/// Drop a node's retained drag/coast state (e.g. when its node is unmounted).
pub fn clear_pan_momentum(id: u64) {
    DRAG_LAST.with(|m| { m.borrow_mut().remove(&id); });
    VELOCITY.with(|m| { m.borrow_mut().remove(&id); });
}
