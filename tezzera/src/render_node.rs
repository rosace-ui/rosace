use std::sync::Arc;
use tezzera_core::types::{Key, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::Picture;

/// A persistent node in the render tree, one per native widget position.
///
/// Caches the last layout inputs/outputs and the last painted Picture so that
/// unchanged subtrees can be replayed without calling `widget.layout()` or
/// `widget.paint()` again. The dirty flags control which work is skipped.
pub struct RenderNode {
    /// Widget type name — used by the reconciler to detect type mismatches.
    pub tag:  &'static str,
    /// Optional reconciler key — for keyed sibling matching.
    pub key:  Option<Key>,

    // ── Layout cache ──────────────────────────────────────────────────────

    /// Constraints used for the last successful layout pass.
    pub last_constraints: Option<Constraints>,
    /// Size returned by the last layout pass.
    pub cached_size:      Option<Size>,

    // ── Paint cache ───────────────────────────────────────────────────────

    /// Display list produced by the last paint pass.
    pub cached_picture: Option<Arc<Picture>>,
    /// World-space rect occupied by this node after the last paint pass.
    pub cached_rect:    Option<Rect>,
    /// When true, the widget must be re-painted this frame.
    pub paint_dirty:    bool,

    // ── Tree structure ────────────────────────────────────────────────────

    pub children: Vec<RenderNode>,
}

impl RenderNode {
    /// A fresh, fully-dirty node. Forces layout + paint on the first frame.
    pub fn new(tag: &'static str, key: Option<Key>) -> Self {
        Self {
            tag,
            key,
            last_constraints: None,
            cached_size:      None,
            cached_picture:   None,
            cached_rect:      None,
            paint_dirty:      true,
            children:         Vec::new(),
        }
    }

    /// Mark this node for re-paint (e.g. after a reconciler type mismatch).
    pub fn invalidate(&mut self) {
        self.paint_dirty    = true;
        self.cached_picture = None;
    }
}
