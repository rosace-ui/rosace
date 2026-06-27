use tezzera_core::types::Rect;

/// Tracks which screen regions need repainting each frame.
///
/// Only dirty regions are repainted, avoiding full-screen redraws for small
/// updates such as a counter increment.  A `DirtyRegionTracker` starts in the
/// full-repaint state so the first frame always paints completely.
pub struct DirtyRegionTracker {
    dirty_rects: Vec<Rect>,
    full_repaint: bool,
}

impl DirtyRegionTracker {
    /// Create a new tracker in the full-repaint state.
    pub fn new() -> Self {
        Self {
            dirty_rects: Vec::new(),
            full_repaint: true,
        }
    }

    /// Mark a specific region as needing repaint.
    pub fn mark_dirty(&mut self, rect: Rect) {
        self.dirty_rects.push(rect);
    }

    /// Request a full repaint on the next frame, discarding any partial rects.
    pub fn mark_full_repaint(&mut self) {
        self.full_repaint = true;
        self.dirty_rects.clear();
    }

    /// Returns `true` if a full repaint has been requested.
    pub fn needs_full_repaint(&self) -> bool {
        self.full_repaint
    }

    /// Returns the list of dirty rects.
    ///
    /// This is empty when a full repaint is pending because [`mark_full_repaint`]
    /// clears partial rects.
    ///
    /// [`mark_full_repaint`]: DirtyRegionTracker::mark_full_repaint
    pub fn dirty_rects(&self) -> &[Rect] {
        &self.dirty_rects
    }

    /// Clear all dirty state after a frame completes.
    pub fn clear(&mut self) {
        self.dirty_rects.clear();
        self.full_repaint = false;
    }

    /// Returns `true` if any region is dirty (either a full repaint or at least
    /// one partial rect).
    pub fn is_dirty(&self) -> bool {
        self.full_repaint || !self.dirty_rects.is_empty()
    }
}

impl Default for DirtyRegionTracker {
    fn default() -> Self {
        Self::new()
    }
}
