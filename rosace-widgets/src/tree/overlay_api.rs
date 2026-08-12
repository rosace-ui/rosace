use std::sync::Arc;
use rosace_core::types::{Point, Rect};
use rosace_render::Color;
use rosace_state::Atom;
use super::{Widget, PaintCtx, BoxedWidget};
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
    /// Whether a tap on the scrim closes this overlay. `true` by default —
    /// the common case, and what every overlay did unconditionally before.
    dismissible: bool,
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
        self.overlays.push(OverlayConfig {
            kind, open, content: Arc::new(content), dismissible: true,
        });
        self
    }

    /// Makes the most recently attached overlay refuse scrim dismissal, so it
    /// can only be closed by setting its `open` atom — the "you must choose"
    /// case (unsaved-changes prompts, required decisions, a sheet mid-upload).
    ///
    /// Dismissal was unconditional before this: every scrim tap closed, with
    /// no way to opt out. Standard elsewhere too — Flutter's `isDismissible`
    /// on bottom sheets and `barrierDismissible` on dialogs.
    ///
    /// ```rust,ignore
    /// widget.dialog(open, || confirm_discard()).non_dismissible()
    /// ```
    pub fn non_dismissible(mut self) -> Self {
        if let Some(last) = self.overlays.last_mut() {
            last.dismissible = false;
        }
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

    /// Attach a CUSTOM-body tooltip overlay to this widget (content-aware:
    /// the closure builds any widget, not just a text label). The everyday
    /// string tooltip is the ergonomic [`super::WidgetExt::tooltip`].
    pub fn rich_tooltip(self, content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        // Tooltip uses a permanent-true open atom — visibility is controlled by hover (Phase 14)
        let open = rosace_state::use_atom(true);
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
    fn children(&self) -> super::Children<'_> {
        super::Children::One(&self.inner)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        self.inner.paint(ctx);
        let anchor: Rect = ctx.rect;

        for cfg in &self.overlays {
            if !cfg.open.get() { continue; }

            let content = (cfg.content)();
            let open_atom = cfg.open.clone();
            // `None` here means "no scrim tap-to-dismiss" — the engine's
            // overlay routing already treats a missing `on_tap` that way, so
            // non-dismissible needs no new dispatch path.
            let dismissible = cfg.dismissible;

            let entry = match cfg.kind {
                OverlayKind::Dropdown => {
                    let pos = Point {
                        x: anchor.origin.x,
                        y: anchor.origin.y + anchor.size.height,
                    };
                    // Invisible scrim: a tap anywhere outside the menu closes
                    // it (and is consumed) — standard menu behavior.
                    let dismiss = Arc::new(move || open_atom.set(false));
                    OverlayEntry::new(LayerPosition::Absolute(pos), content)
                        .input(InputBehavior::PassThrough)
                        .focus(FocusBehavior::PassThrough)
                        .scrim(ScrimConfig {
                            color: Color::TRANSPARENT,
                            on_tap: dismissible.then_some(dismiss as Arc<dyn Fn() + Send + Sync>),
                        exclude_rect: None,
                        })
                }

                OverlayKind::Sheet => {
                    let dismiss = Arc::new(move || open_atom.set(false));
                    // A sheet is a MODAL SURFACE, like a Dialog: it owns the
                    // clicks and scrolls that land on it, and only a tap on
                    // the scrim OUTSIDE it dismisses. It was `PassThrough`,
                    // so an inside tap skipped the absorb step in the engine's
                    // overlay routing and fell straight through to the scrim's
                    // on_tap — tapping any content inside the sheet closed it
                    // (user-reported 2026-08-12). `Dialog` right below always
                    // had this correct.
                    OverlayEntry::new(LayerPosition::BottomAnchored, content)
                        .input(InputBehavior::Block)
                        .focus(FocusBehavior::Trap)
                        .scrim(ScrimConfig {
                            color: Color::rgba(0, 0, 0, 100),
                            on_tap: dismissible.then_some(dismiss as Arc<dyn Fn() + Send + Sync>),
                        exclude_rect: None,
                        })
                }

                OverlayKind::Dialog => {
                    let dismiss = Arc::new(move || open_atom.set(false));
                    OverlayEntry::new(LayerPosition::Centered, content)
                        .input(InputBehavior::Block)
                        .focus(FocusBehavior::Trap)
                        .scrim(ScrimConfig {
                            color: Color::rgba(0, 0, 0, 160),
                            on_tap: dismissible.then_some(dismiss as Arc<dyn Fn() + Send + Sync>),
                        exclude_rect: None,
                        })
                }

                OverlayKind::Tooltip => {
                    // Centered just above the hovered widget (user-reported:
                    // the old right-edge Absolute position drifted far from
                    // the anchor).
                    OverlayEntry::new(LayerPosition::AboveCentered(anchor), content)
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
    // layout, flex_factor: protocol defaults delegate to the child.
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

    fn rich_tooltip(self,
               content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).rich_tooltip(content)
    }

    fn toast(self, open: Atom<bool>,
             content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).toast(open, content)
    }
}

impl<W: Widget + Send + Sync + 'static> OverlayApi for W {}
