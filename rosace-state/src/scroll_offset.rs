//! Non-reactive scroll-offset channel for GPU-composited scroll layers (D090).
//!
//! A placed scroll layer's offset is purely a compositor concern: the content
//! is already rasterized into its own texture, so scrolling is a UV shift, not
//! a re-render. This channel holds each such layer's offset OUTSIDE the
//! reactive graph, keyed by its render-tree node id. Updating it requests a
//! present-only frame (`request_frame`) but dirties NO component — so the
//! frame skips build/paint and the platform re-composites with the new offset
//! as a uniform update. That is the "scroll produces no CPU paint" path.
//!
//! Main-thread only (event dispatch + present both run there), so a
//! thread-local map suffices — mirrors the widget-side registries.

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static SCROLL_OFFSETS: RefCell<HashMap<u64, [f32; 2]>> = RefCell::new(HashMap::new());
}

/// Current offset (logical px) for a placed layer, or `[0, 0]` if never set.
pub fn scroll_offset(id: u64) -> [f32; 2] {
    SCROLL_OFFSETS.with(|m| m.borrow().get(&id).copied().unwrap_or([0.0, 0.0]))
}

/// Set a placed layer's offset (logical px) and request a present-only frame.
/// Does not dirty any component — the next frame re-composites via a UV shift.
pub fn set_scroll_offset(id: u64, offset: [f32; 2]) {
    SCROLL_OFFSETS.with(|m| {
        m.borrow_mut().insert(id, offset);
    });
    crate::request_frame();
}

/// Add `(dx, dy)` to a placed layer's offset, clamped to `[0, max]` per axis.
pub fn scroll_offset_by(id: u64, dx: f32, dy: f32, max_x: f32, max_y: f32) {
    let [ox, oy] = scroll_offset(id);
    let nx = (ox + dx).clamp(0.0, max_x.max(0.0));
    let ny = (oy + dy).clamp(0.0, max_y.max(0.0));
    if [nx, ny] != [ox, oy] {
        set_scroll_offset(id, [nx, ny]);
    }
}

/// Drop a layer's retained offset (e.g. when its node is unmounted).
pub fn clear_scroll_offset(id: u64) {
    SCROLL_OFFSETS.with(|m| {
        m.borrow_mut().remove(&id);
    });
}
