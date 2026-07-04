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
        let r = ctx.rect;
        self.child.paint(&mut ctx.child(r));
    }
    // layout, flex_factor: protocol defaults delegate to the child.
}

/// `.on_press(cb)` on any widget (D094 vocabulary — never on_click/on_tap).
pub trait PressApi: Widget + Sized + Send + Sync + 'static {
    fn on_press(self, f: impl Fn() + Send + Sync + 'static) -> Pressable<Self> {
        Pressable::new(self, f)
    }
}

impl<W: Widget + Send + Sync + 'static> PressApi for W {}
