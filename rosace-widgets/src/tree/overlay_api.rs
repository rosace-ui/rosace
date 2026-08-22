use std::sync::Arc;
use rosace_core::types::{Point, Rect};
use rosace_render::Color;
use super::{Widget, PaintCtx, BoxedWidget};
use super::PromoteOpts;
use super::overlay::{
    FocusBehavior, InputBehavior, LayerPosition, ScrimConfig,
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
    /// Whether this overlay is showing. A VALUE, not a cell the widget owns:
    /// the app holds the state and is told when it should change.
    open:    bool,
    /// Called with `false` when the overlay asks to close (a scrim tap, or
    /// Back/Escape). `None` means it cannot be dismissed that way.
    on_open_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
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

    fn push(mut self, kind: OverlayKind, open: bool,
            content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        self.overlays.push(OverlayConfig {
            kind, open, on_open_change: None, content: Arc::new(content), dismissible: true,
        });
        self
    }

    /// Called when the most recently attached overlay wants to close.
    ///
    /// Without one, a scrim tap and Back/Escape have nowhere to report to, so
    /// the overlay is effectively non-dismissible.
    pub fn on_open_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        if let Some(last) = self.overlays.last_mut() {
            last.on_open_change = Some(Arc::new(f));
        }
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
    pub fn dropdown(self, open: bool,
                    content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        self.push(OverlayKind::Dropdown, open, content)
    }

    /// Attach a bottom sheet overlay to this widget.
    pub fn sheet(self, open: bool,
                 content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        self.push(OverlayKind::Sheet, open, content)
    }

    /// Attach a modal dialog overlay to this widget.
    pub fn dialog(self, open: bool,
                  content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        self.push(OverlayKind::Dialog, open, content)
    }

    /// Attach a CUSTOM-body tooltip overlay to this widget (content-aware:
    /// the closure builds any widget, not just a text label). The everyday
    /// string tooltip is the ergonomic [`super::WidgetExt::tooltip`].
    pub fn rich_tooltip(self, content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        // Always "open" — a tooltip's visibility is driven by hover, not by
        // app state (Phase 14).
        self.push(OverlayKind::Tooltip, true, content)
    }

    /// Attach a toast overlay to this widget. Use [`Toast::show`] to open it
    /// with auto-dismiss.
    ///
    /// [`Toast::show`]: super::toast::Toast::show
    pub fn toast(self, open: bool,
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

        for (i, cfg) in self.overlays.iter().enumerate() {
            if !cfg.open { continue; }

            let content = (cfg.content)();
            // `None` means "no scrim tap-to-dismiss" — a missing `on_tap`
            // already means exactly that, so non-dismissible needs no new path.
            let on_tap: Option<Arc<dyn Fn() + Send + Sync>> = match
                (cfg.dismissible, cfg.on_open_change.clone())
            {
                (true, Some(cb)) => Some(Arc::new(move || cb(false))),
                _ => None,
            };

            let (position, opts) = match cfg.kind {
                OverlayKind::Dropdown => (
                    LayerPosition::Absolute(Point {
                        x: anchor.origin.x,
                        y: anchor.origin.y + anchor.size.height,
                    }),
                    PromoteOpts {
                        // Invisible scrim: a tap anywhere outside the menu
                        // closes it (and is consumed) — standard menu
                        // behaviour.
                        scrim: Some(ScrimConfig {
                            color: Color::TRANSPARENT,
                            on_tap,
                            // NOT the anchor, deliberately: this entry point
                            // has always passed `None`, and promoted-first
                            // dispatch already consumes the tap rather than
                            // letting it reach the trigger underneath. Whether
                            // it SHOULD exempt its trigger is a behaviour
                            // question, not a migration one.
                            exclude_rect: None,
                        }),
                        input: InputBehavior::PassThrough,
                        focus: FocusBehavior::PassThrough,
                    },
                ),

                // A sheet is a MODAL SURFACE, like a Dialog: it owns the
                // clicks and scrolls that land on it, and only a tap on the
                // scrim OUTSIDE it dismisses. It was `PassThrough` once, so an
                // inside tap skipped the absorb step and fell straight through
                // to the scrim's on_tap — tapping any content inside the sheet
                // closed it (user-reported 2026-08-12).
                OverlayKind::Sheet => (
                    LayerPosition::BottomAnchored,
                    PromoteOpts {
                        scrim: Some(ScrimConfig {
                            color: Color::rgba(0, 0, 0, 100),
                            on_tap,
                            exclude_rect: None,
                        }),
                        input: InputBehavior::Block,
                        focus: FocusBehavior::Trap,
                    },
                ),

                OverlayKind::Dialog => (
                    LayerPosition::Centered,
                    PromoteOpts {
                        scrim: Some(ScrimConfig {
                            color: Color::rgba(0, 0, 0, 160),
                            on_tap,
                            exclude_rect: None,
                        }),
                        input: InputBehavior::Block,
                        focus: FocusBehavior::Trap,
                    },
                ),

                // Centered just above the hovered widget (user-reported: the
                // old right-edge Absolute position drifted far from the
                // anchor).
                OverlayKind::Tooltip => (
                    LayerPosition::AboveCentered(anchor),
                    PromoteOpts {
                        scrim: None,
                        input: InputBehavior::PassThrough,
                        focus: FocusBehavior::Inert,
                    },
                ),

                OverlayKind::Toast => (
                    LayerPosition::BottomCenter,
                    PromoteOpts {
                        scrim: None,
                        input: InputBehavior::PassThrough,
                        focus: FocusBehavior::Inert,
                    },
                ),
            };

            // Keyed by DECLARATION INDEX, not by the `open` atom's id. Slots
            // are positional and this loop skips closed overlays, so a host
            // declaring several would otherwise hand the second one the
            // first's node — and with it its animation and scroll state — the
            // moment the first closed.
            ctx.promote_keyed(i as u64, position, &*content, opts);
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
    fn dropdown(self, open: bool,
                content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).dropdown(open, content)
    }

    fn sheet(self, open: bool,
             content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).sheet(open, content)
    }

    fn dialog(self, open: bool,
              content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).dialog(open, content)
    }

    fn rich_tooltip(self,
               content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).rich_tooltip(content)
    }

    fn toast(self, open: bool,
             content: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> WithOverlay<Self> {
        WithOverlay::new(self).toast(open, content)
    }
}

impl<W: Widget + Send + Sync + 'static> OverlayApi for W {}
