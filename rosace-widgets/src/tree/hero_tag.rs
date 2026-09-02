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
        let rect = ctx.rect;
        match hero::active_role() {
            Some(role) => {
                // Register either way: pairing is what decides whether a
                // flight happens, and that is only known once BOTH sides have
                // painted.
                hero::register(self.tag.clone(), role, rect, Arc::clone(&self.inner));

                if hero::is_flying(&self.tag) {
                    // A flight is in the air, so stand aside — the promoted
                    // copy is the one on screen.
                    //
                    // Marking this node dirty is not optional. A hidden node
                    // caches a picture with nothing in it; the frame the
                    // flight ends is an ordinary cache-hit frame, so an
                    // ancestor would re-blit that empty picture and the
                    // element would be gone for good. This is the explicit
                    // invalidation Flutter gets from `setState` on the
                    // endpoints when a flight starts and ends.
                    super::mark_node_dirty(ctx.node);
                } else {
                    // No flight for this tag — either it has none on the other
                    // side, or the pair has not formed yet. Paint normally: a
                    // widget must never disappear just because some OTHER
                    // hero is flying.
                    ctx.paint_child(rect, &*self.inner);
                }
            }
            None => {
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
