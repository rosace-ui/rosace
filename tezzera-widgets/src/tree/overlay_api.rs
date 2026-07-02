use std::sync::Arc;
use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::Color;
use tezzera_state::Atom;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::overlay::{
    FocusBehavior, InputBehavior, LayerPosition, OverlayEntry, ScrimConfig,
};

// ── OverlayKind ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    /// Anchored at trigger bottom-left. PassThrough input. No scrim.
    Dropdown,
    /// Bottom of window. PassThrough input. Dim scrim with tap-to-dismiss.
    Sheet,
    /// Centered. Blocks input. Traps focus. Dim scrim with tap-to-dismiss.
    Dialog,
    /// Anchored at trigger top-right. PassThrough. Inert. No scrim.
    Tooltip,
    /// Floating above the bottom edge, centered. PassThrough. Inert. No scrim.
    Toast,
}

// ── Overlay config entry ──────────────────────────────────────────────────────

struct OverlayConfig {
    kind:    OverlayKind,
    open:    Atom<bool>,
    content: Arc<dyn Fn() -> BoxedWidget + Send + Sync>,
}

// ── WithOverlay wrapper ───────────────────────────────────────────────────────

/// Wraps a widget with co-located overlay declarations.
///
/// Created by the [`OverlayApi`] builder methods. Implements [`Widget`] and
/// can be chained with further `.dropdown()` / `.sheet()` / `.dialog()` calls.
pub struct WithOverlay<W: Widget> {
    inner:    W,
    overlays: Vec<OverlayConfig>,
}

impl<W: Widget + 'static> WithOverlay<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, overlays: Vec::new() }
    }

    fn push(mut self, kind: OverlayKind, open: Atom<bool>,
            content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        self.overlays.push(OverlayConfig { kind, open, content: Arc::new(content) });
        self
    }

    /// Attach a dropdown overlay to this widget.
    pub fn dropdown(self, open: Atom<bool>,
                    content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        self.push(OverlayKind::Dropdown, open, content)
    }

    /// Attach a bottom sheet overlay to this widget.
    pub fn sheet(self, open: Atom<bool>,
                 content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        self.push(OverlayKind::Sheet, open, content)
    }

    /// Attach a modal dialog overlay to this widget.
    pub fn dialog(self, open: Atom<bool>,
                  content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        self.push(OverlayKind::Dialog, open, content)
    }

    /// Attach a tooltip overlay to this widget.
    pub fn tooltip(self, content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        // Tooltip uses a permanent-true open atom — visibility is controlled by hover (Phase 14)
        let open = tezzera_state::use_atom(true);
        self.push(OverlayKind::Tooltip, open, content)
    }

    /// Attach a toast overlay to this widget. Use [`Toast::show`] to open it
    /// with auto-dismiss.
    ///
    /// [`Toast::show`]: super::toast::Toast::show
    pub fn toast(self, open: Atom<bool>,
                 content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        self.push(OverlayKind::Toast, open, content)
    }
}

impl<W: Widget + Send + Sync + 'static> Widget for WithOverlay<W> {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        self.inner.layout(ctx)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        self.inner.paint(ctx);
        let anchor: Rect = ctx.rect;

        for cfg in &self.overlays {
            if !cfg.open.get() { continue; }

            let content = (cfg.content)();
            let open_atom = cfg.open.clone();

            let entry = match cfg.kind {
                OverlayKind::Dropdown => {
                    let pos = Point {
                        x: anchor.origin.x,
                        y: anchor.origin.y + anchor.size.height,
                    };
                    OverlayEntry::new(LayerPosition::Absolute(pos), content)
                        .input(InputBehavior::PassThrough)
                        .focus(FocusBehavior::PassThrough)
                }

                OverlayKind::Sheet => {
                    let dismiss = Arc::new(move || open_atom.set(false));
                    OverlayEntry::new(LayerPosition::BottomAnchored, content)
                        .input(InputBehavior::PassThrough)
                        .focus(FocusBehavior::PassThrough)
                        .scrim(ScrimConfig {
                            color: Color::rgba(0, 0, 0, 100),
                            on_tap: Some(dismiss),
                        })
                }

                OverlayKind::Dialog => {
                    let dismiss = Arc::new(move || open_atom.set(false));
                    OverlayEntry::new(LayerPosition::Centered, content)
                        .input(InputBehavior::Block)
                        .focus(FocusBehavior::Trap)
                        .scrim(ScrimConfig {
                            color: Color::rgba(0, 0, 0, 160),
                            on_tap: Some(dismiss),
                        })
                }

                OverlayKind::Tooltip => {
                    let pos = Point {
                        x: anchor.origin.x + anchor.size.width,
                        y: anchor.origin.y,
                    };
                    OverlayEntry::new(LayerPosition::Absolute(pos), content)
                        .input(InputBehavior::PassThrough)
                        .focus(FocusBehavior::Inert)
                }

                OverlayKind::Toast => {
                    OverlayEntry::new(LayerPosition::BottomCenter, content)
                        .input(InputBehavior::PassThrough)
                        .focus(FocusBehavior::Inert)
                }
            };

            // Attach to the render-tree node (D091): the entry persists across
            // cache-hit frames and is cleared when this node repaints — an
            // open dialog can no longer vanish on the MouseUp frame.
            ctx.attach_overlay(entry);
        }
    }

    fn flex_factor(&self) -> f32 { self.inner.flex_factor() }
}

// ── OverlayApi trait — blanket impl for all widgets ───────────────────────────

/// Builder methods that attach co-located overlay declarations to any widget.
///
/// Each method wraps the widget in a [`WithOverlay`] (or extends an existing
/// one) and stores the open-state atom + content factory. The framework pushes
/// the correct [`OverlayEntry`] automatically when the atom is true.
///
/// ```rust,ignore
/// Button::new("Settings")
///     .sheet(is_open.clone(), || SettingsSheet::new())
///
/// Button::new("Delete")
///     .dialog(confirm_open.clone(), || {
///         Dialog::new("Are you sure?")
///             .action("Cancel", || confirm_open.set(false))
///             .action("Delete", on_delete.clone())
///     })
/// ```
pub trait OverlayApi: Widget + Sized + Send + Sync + 'static {
    fn dropdown(self, open: Atom<bool>,
                content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).dropdown(open, content)
    }

    fn sheet(self, open: Atom<bool>,
             content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).sheet(open, content)
    }

    fn dialog(self, open: Atom<bool>,
              content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).dialog(open, content)
    }

    fn tooltip(self,
               content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).tooltip(content)
    }

    fn toast(self, open: Atom<bool>,
             content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).toast(open, content)
    }
}

impl<W: Widget + Send + Sync + 'static> OverlayApi for W {}
