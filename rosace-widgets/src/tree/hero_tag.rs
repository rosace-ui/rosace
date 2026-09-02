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

                // Keep THIS node out of the replay cache for the whole
                // transition, and only this node.
                //
                // Two things depend on it. A replayed node never runs, so it
                // would stop registering and the pair would dissolve
                // mid-flight. And a node hidden for the flight caches an
                // empty picture — the frame the flight ends is an ordinary
                // cache-hit frame, so an ancestor would re-blit that emptiness
                // and the element would be gone for good.
                //
                // This replaces a global ban on replay for the duration of any
                // flight, which cost every other widget in BOTH screens a
                // full re-record every frame: measured 1838 us/frame against
                // 1170 settled, ~84 leaf paints per frame on a 40-row page.
                // Flutter rebuilds the heroes, not the world.
                super::mark_node_dirty(ctx.node);

                if hero::is_flying(&self.tag) {
                    // A flight is in the air, so stand aside — the promoted
                    // copy is the one on screen.
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
