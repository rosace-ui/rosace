use std::sync::Arc;

use super::hero;
use super::{BoxedWidget, Children, LayoutCtx, PaintCtx, Widget};

/// Tags a widget for shared-element morphing across a screen transition.
///
/// Outside an active transition this is a total pass-through — same paint
/// output as not wrapping at all. While `ScreenTransitionView` has a
/// transition in flight, a `Hero` on the outgoing screen and one with the SAME
/// tag on the incoming screen are paired, and a single live copy is promoted
/// to the root layer and flown between their two rects. See `hero.rs` for the
/// mechanism and for what does not survive the flight.
pub struct Hero {
    tag: String,
    /// Held as a `BoxedWidget` (an `Arc<dyn Widget>`) so registering it for
    /// the flight is a refcount bump rather than a capture.
    inner: BoxedWidget,
}

impl Hero {
    pub fn new(tag: impl Into<String>, inner: impl Widget + 'static) -> Self {
        Self { tag: tag.into(), inner: Arc::new(inner) }
    }
}

impl Widget for Hero {
    fn children(&self) -> Children<'_> {
        Children::One(&*self.inner)
    }

    fn layout(&self, ctx: &LayoutCtx) -> rosace_core::types::Size {
        // Own node, matching `paint`'s `paint_child` — see
        // `LayoutCtx::layout_child_uncached`.
        ctx.layout_child_uncached(ctx.constraints, &*self.inner)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        match hero::active_role() {
            Some(role) => {
                // Suppressed on both sides for the duration of the flight —
                // the promoted copy is the one on screen. Registering the
                // widget itself (not a captured Picture) is what lets that
                // copy be live.
                hero::register(self.tag.clone(), role, ctx.rect, Arc::clone(&self.inner));
            }
            None => {
                let rect = ctx.rect;
                ctx.paint_child(rect, &*self.inner);
            }
        }
    }
}

/// Builder sugar: `.hero_tag("id")` on any widget.
pub trait HeroApi: Widget + Sized + Send + Sync + 'static {
    fn hero_tag(self, tag: impl Into<String>) -> Hero {
        Hero::new(tag, self)
    }
}

impl<W: Widget + Send + Sync + 'static> HeroApi for W {}
