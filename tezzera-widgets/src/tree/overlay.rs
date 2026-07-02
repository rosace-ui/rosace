use std::cell::RefCell;
use std::sync::Arc;
use tezzera_core::types::Point;
use tezzera_render::Color;
use super::BoxedWidget;

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LayerId(pub u64);

impl LayerId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        LayerId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for LayerId {
    fn default() -> Self { LayerId::new() }
}

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
    /// Fills the entire window.
    Fill,
}

/// Controls whether pointer events that miss the overlay widget's rect
/// fall through to entries below / the main tree, or are absorbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputBehavior {
    /// Misses fall through to the next entry or main tree.
    PassThrough,
    /// Misses are absorbed (or trigger scrim dismiss if configured).
    Block,
}

/// Controls Tab focus traversal relative to this overlay entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusBehavior {
    /// Tab continues to entries below after this one is exhausted.
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
}

impl std::fmt::Debug for ScrimConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrimConfig")
            .field("color", &self.color)
            .field("on_tap", &self.on_tap.as_ref().map(|_| "<Fn>"))
            .finish()
    }
}

/// A single entry in the overlay stack.
///
/// Entries are painted top-to-bottom in insertion order (last = topmost).
/// See D058 in DECISIONS.md for the full architecture.
pub struct OverlayEntry {
    pub id:       LayerId,
    pub position: LayerPosition,
    pub widget:   BoxedWidget,
    pub input:    InputBehavior,
    pub focus:    FocusBehavior,
    pub scrim:    Option<ScrimConfig>,
}

impl OverlayEntry {
    pub fn new(position: LayerPosition, widget: impl super::Widget + 'static) -> Self {
        Self {
            id: LayerId::new(),
            position,
            widget: Box::new(widget),
            input: InputBehavior::PassThrough,
            focus: FocusBehavior::PassThrough,
            scrim: None,
        }
    }

    pub fn input(mut self, b: InputBehavior) -> Self { self.input = b; self }
    pub fn focus(mut self, b: FocusBehavior) -> Self { self.focus = b; self }
    pub fn scrim(mut self, s: ScrimConfig) -> Self { self.scrim = Some(s); self }
}

// ── Thread-local registry ─────────────────────────────────────────────────────

thread_local! {
    static OVERLAY_ENTRIES: RefCell<Vec<OverlayEntry>> = RefCell::new(Vec::new());
}

/// Push an overlay entry from within a widget's `paint()` call.
/// The entry will be composited above the main tree for this frame.
pub fn push_overlay(entry: OverlayEntry) {
    OVERLAY_ENTRIES.with(|v| v.borrow_mut().push(entry));
}

/// Drain all pending overlay entries. Called once per frame by the render loop
/// after the main paint pass, before the second (overlay) recorder pass.
pub fn drain_overlays() -> Vec<OverlayEntry> {
    OVERLAY_ENTRIES.with(|v| v.borrow_mut().drain(..).collect())
}

/// Clear any leftover overlay entries from the previous frame.
pub fn clear_overlays() {
    OVERLAY_ENTRIES.with(|v| v.borrow_mut().clear());
}
