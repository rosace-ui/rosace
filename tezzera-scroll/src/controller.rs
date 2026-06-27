use tezzera_state::Atom;

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
}

impl ScrollController {
    pub fn new() -> Self {
        Self {
            offset: tezzera_state::use_atom([0.0f32; 2]),
            content_size: tezzera_state::use_atom([0.0f32; 2]),
            viewport_size: tezzera_state::use_atom([0.0f32; 2]),
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
}
