use super::{Widget, Children, PaintCtx};

/// Makes its subtree transparent to the pointer — clicks, scrolls, and hover
/// pass straight through to whatever is behind it. Useful for decorative
/// overlays that must not steal input.
///
/// ```rust,ignore
/// IgnorePointer::new(decorative_badge_overlay)
/// ```
pub struct IgnorePointer<W: Widget> { child: W }

impl<W: Widget + Send + Sync + 'static> IgnorePointer<W> {
    pub fn new(child: W) -> Self { Self { child } }
}

impl<W: Widget + Send + Sync + 'static> Widget for IgnorePointer<W> {
    fn children(&self) -> Children<'_> { Children::One(&self.child) }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.set_pointer_mode(1); // transparent
        let r = ctx.rect;
        self.child.paint(&mut ctx.child(r));
    }
}

/// Absorbs every pointer event over its rect — nothing inside or behind
/// receives clicks/scrolls. Useful for disabling a region or building a
/// modal barrier.
pub struct AbsorbPointer<W: Widget> { child: W }

impl<W: Widget + Send + Sync + 'static> AbsorbPointer<W> {
    pub fn new(child: W) -> Self { Self { child } }
}

impl<W: Widget + Send + Sync + 'static> Widget for AbsorbPointer<W> {
    fn children(&self) -> Children<'_> { Children::One(&self.child) }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.set_pointer_mode(2); // absorb
        let r = ctx.rect;
        self.child.paint(&mut ctx.child(r));
    }
}
