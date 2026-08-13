use rosace_core::a11y::FocusNode;
use rosace_render::{Color, DrawCommand};
use super::{Widget, PaintCtx};

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
    /// `None` = the active theme's `colors.primary`.
    ring_color: Option<Color>,
    /// `None` = a square ring. Set this to match a rounded child's corners.
    radius:     Option<f32>,
}

impl<W: Widget + 'static> WithFocus<W> {
    pub fn new(inner: W, node: FocusNode) -> Self {
        Self { inner, node, ring_color: None, radius: None }
    }

    /// Override the focus-ring colour (defaults to the theme's primary).
    pub fn ring_color(mut self, c: Color) -> Self { self.ring_color = Some(c); self }

    /// Round the focus ring to match the wrapped widget's corners — a square
    /// halo around a rounded Button or TextInput reads as a rendering bug.
    pub fn radius(mut self, r: f32) -> Self { self.radius = Some(r); self }

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
    fn children(&self) -> super::Children<'_> {
        super::Children::One(&self.inner)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Register in DFS order so FocusManager can build the Tab cycle.
        ctx.register_focus(self.node.clone());

        self.inner.paint(ctx);

        // Draw a 2px focus ring when focused.
        //
        // The colour comes from the theme so this ring matches the ones the
        // controls draw for themselves (`switch`, `checkbox`, `chip` all read
        // `colors.primary`); it used to be a fixed blue that matched nothing.
        // The radius is settable because a square halo around a rounded
        // Button or TextInput reads as a rendering bug.
        if self.node.is_focused() {
            let rect = ctx.rect;
            let color = self.ring_color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.primary));
            match self.radius {
                Some(r) if r > 0.0 => ctx.stroke_rrect(rect, r, color, 2.0),
                _ => ctx.recorder.push(DrawCommand::StrokeRect { rect, color, width: 2.0 }),
            }
        }
    }
    // layout, flex_factor: protocol defaults delegate to the child.
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
