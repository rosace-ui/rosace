use std::sync::Arc;

use super::{Widget, Children, PaintCtx};

/// Makes ANY widget clickable — the whole child rect becomes a press target
/// (clip-aware, z-ordered, persistent via the render tree).
///
/// Usually created through the blanket [`PressApi`]:
///
/// ```rust,ignore
/// Text::new("tap me").on_press(|| do_thing())
/// Card::new(content).on_press(open_details)
/// ```
///
/// Widgets with their own `on_press` builder (Button, ListTile) keep it —
/// inherent methods win. Press/hover visual feedback (the InkWell ripple)
/// arrives with the interaction-states work; this is the hit plumbing.
pub struct Pressable<W: Widget> {
    child: W,
    on_press: Arc<dyn Fn() + Send + Sync>,
}

impl<W: Widget + Send + Sync + 'static> Pressable<W> {
    pub fn new(child: W, on_press: impl Fn() + Send + Sync + 'static) -> Self {
        Self { child, on_press: Arc::new(on_press) }
    }
}

impl<W: Widget + Send + Sync + 'static> Widget for Pressable<W> {
    fn children(&self) -> Children<'_> {
        Children::One(&self.child)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let f = self.on_press.clone();
        ctx.on_press(move || f());
        // Quality Bar §5. Deliberately no label: `Pressable` wraps arbitrary
        // content, so its accessible NAME is whatever that content says.
        // Platform a11y layers derive a button's name from its descendants
        // (AccessKit does this for `Role::Button` explicitly), so declaring
        // the role alone gives "Save, button" rather than an empty control.
        // Inventing a label here would override the child's real text.
        ctx.semantics(super::SemanticsProps::new(rosace_core::Role::Button));
        let r = ctx.rect;
        ctx.paint_child(r, &self.child);
    }
    // layout, flex_factor: protocol defaults delegate to the child.
}

/// Wraps any widget with a long-press (≈500 ms) callback on its rect.
pub struct LongPressable<W: Widget> {
    child: W,
    on_long_press: Arc<dyn Fn() + Send + Sync>,
}

impl<W: Widget + Send + Sync + 'static> LongPressable<W> {
    pub fn new(child: W, f: impl Fn() + Send + Sync + 'static) -> Self {
        Self { child, on_long_press: Arc::new(f) }
    }
}

impl<W: Widget + Send + Sync + 'static> Widget for LongPressable<W> {
    fn children(&self) -> Children<'_> { Children::One(&self.child) }
    fn paint(&self, ctx: &mut PaintCtx) {
        let f = self.on_long_press.clone();
        ctx.on_long_press(move || f());
        // Quality Bar §5, same reasoning as `Pressable` above: declare the
        // role, never a label — the wrapped content supplies the name. This
        // was missing entirely, which mattered more here than anywhere else:
        // `PressApi` is a blanket impl over EVERY widget in the library, so a
        // long-press-only affordance was invisible to assistive tech
        // library-wide.
        ctx.semantics(super::SemanticsProps::new(rosace_core::Role::Button));
        let r = ctx.rect;
        ctx.paint_child(r, &self.child);
    }
}

/// `.on_press(cb)` / `.on_long_press(cb)` on any widget (D094 vocabulary —
/// never on_click/on_tap).
pub trait PressApi: Widget + Sized + Send + Sync + 'static {
    fn on_press(self, f: impl Fn() + Send + Sync + 'static) -> Pressable<Self> {
        Pressable::new(self, f)
    }
    fn on_long_press(self, f: impl Fn() + Send + Sync + 'static) -> LongPressable<Self> {
        LongPressable::new(self, f)
    }
}

impl<W: Widget + Send + Sync + 'static> PressApi for W {}
