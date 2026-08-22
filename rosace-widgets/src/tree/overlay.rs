use std::sync::Arc;
use rosace_core::types::{Point, Rect};
use rosace_render::Color;

// ── Public types ──────────────────────────────────────────────────────────────

/// Where the overlay widget is placed in window-pixel space.
#[derive(Clone, Debug)]
pub enum LayerPosition {
    /// Top-left corner at this point. Widget chooses its own size.
    Absolute(Point),
    /// Centered in the window. Widget chooses its own size.
    Centered,
    /// Anchored to the bottom edge, full-width. Widget chooses height.
    BottomAnchored,
    /// Horizontally centered, floating 24px above the bottom edge (toasts).
    BottomCenter,
    /// Centered horizontally over the anchor rect, floating just above it
    /// (tooltips). The anchor is in the ATTACHING widget's coordinate
    /// space — the engine remaps it to window space and clamps on-screen.
    AboveCentered(rosace_core::types::Rect),
    /// Fills the entire window.
    Fill,
}

/// Controls whether pointer events that miss the overlay widget's rect
/// fall through to entries below / the main tree, or are absorbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InputBehavior {
    /// Misses fall through to the next entry or main tree.
    #[default]
    PassThrough,
    /// Misses are absorbed (or trigger scrim dismiss if configured).
    Block,
}

/// Controls Tab focus traversal relative to this overlay entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FocusBehavior {
    /// Tab continues to entries below after this one is exhausted.
    #[default]
    PassThrough,
    /// Tab cycles only within this entry — cannot escape.
    Trap,
    /// No focusable nodes. Ignored by all Tab traversal.
    Inert,
}

/// Optional translucent background drawn before the overlay widget.
#[derive(Clone)]
pub struct ScrimConfig {
    pub color:  Color,
    /// If `Some`, called when a tap lands outside the overlay widget's rect.
    pub on_tap: Option<Arc<dyn Fn() + Send + Sync>>,
    /// A rect (in window space) that is exempt from `on_tap` even though
    /// it's outside the overlay widget itself — e.g. a Dropdown's own
    /// trigger button. Without this, clicking the trigger that opened the
    /// overlay both fires `on_tap` (closing it) AND falls through to the
    /// trigger's own base-tree click handler (reopening it) in the same
    /// event, so the dropdown could never close via its own trigger.
    pub exclude_rect: Option<Rect>,
}

impl std::fmt::Debug for ScrimConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrimConfig")
            .field("color", &self.color)
            .field("on_tap", &self.on_tap.as_ref().map(|_| "<Fn>"))
            .field("exclude_rect", &self.exclude_rect)
            .finish()
    }
}

// The overlay ENTRY type and its thread-local registry are gone. An overlay is
// a promoted node now: `PaintCtx::promote_at` places it, the node carries its
// focus policy and dismisser, and the render tree lays it out, paints it,
// hit-tests it and hands it to assistive tech like anything else. A parallel
// stack with its own retained trees and its own dispatch list was the third
// compositing mechanism in the engine; this is the commit where it stops
// existing.
