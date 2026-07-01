use tezzera_a11y::FocusNode;
use tezzera_core::types::Size;
use tezzera_render::{Color, DrawCommand};
use super::{Widget, LayoutCtx, PaintCtx};

// ── WithFocus wrapper ─────────────────────────────────────────────────────────

/// Wraps a widget with a [`FocusNode`], enabling explicit focus graph wiring
/// and reactive focus-ring rendering.
///
/// Created by the [`FocusApi`] builder methods. `focused()` is a reactive
/// `Atom<bool>` — set it to `true` (via `FocusNode::request()`) to draw the
/// focus ring around this widget.
pub struct WithFocus<W: Widget> {
    inner:      W,
    node:       FocusNode,
}

impl<W: Widget + 'static> WithFocus<W> {
    pub fn new(inner: W, node: FocusNode) -> Self {
        Self { inner, node }
    }

    /// Wire an explicit Tab-forward neighbor.
    pub fn focus_next_node(self, next: FocusNode) -> Self {
        self.node.set_next(next);
        self
    }

    /// Wire an explicit Shift+Tab / reverse neighbor.
    pub fn focus_prev_node(self, prev: FocusNode) -> Self {
        self.node.set_prev(prev);
        self
    }

    /// The focus node attached to this widget (cloned — cheap Arc clone).
    pub fn node(&self) -> FocusNode { self.node.clone() }
}

impl<W: Widget + Send + Sync + 'static> Widget for WithFocus<W> {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        self.inner.layout(ctx)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Register in DFS order so FocusManager can build the Tab cycle.
        ctx.register_focus(self.node.clone());

        self.inner.paint(ctx);

        // Draw a 2px focus ring when focused.
        if self.node.is_focused() {
            let rect = ctx.rect;
            ctx.recorder.push(DrawCommand::StrokeRect {
                rect,
                color: Color::rgba(100, 160, 255, 220),
                width: 2.0,
            });
        }
    }

    fn flex_factor(&self) -> f32 { self.inner.flex_factor() }
}

// ── FocusApi trait — blanket impl for all widgets ─────────────────────────────

/// Builder methods that attach a [`FocusNode`] to any widget.
///
/// ```rust,ignore
/// let email  = FocusNode::new();
/// let pass   = FocusNode::new();
/// let submit = FocusNode::new();
///
/// TextInput::new("Email").focus_node(email.clone())
///     .focus_next_node(pass.clone())
///
/// TextInput::new("Password").focus_node(pass.clone())
///     .focus_next_node(submit.clone())
///     .focus_prev_node(email.clone())
///
/// Button::new("Login").focus_node(submit.clone())
///     .focus_prev_node(pass.clone())
/// ```
pub trait FocusApi: Widget + Sized + Send + Sync + 'static {
    /// Attach a focus node. This enables focus-ring rendering and explicit
    /// neighbor wiring.
    fn focus_node(self, node: FocusNode) -> WithFocus<Self> {
        WithFocus::new(self, node)
    }
}

impl<W: Widget + Send + Sync + 'static> FocusApi for W {}
