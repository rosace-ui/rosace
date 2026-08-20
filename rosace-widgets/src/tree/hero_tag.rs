use super::hero;
use super::{Children, LayoutCtx, PaintCtx, Widget};

/// Wraps a widget with a stable tag for Hero/shared-element morphing across
/// a screen transition (D108/Phase 26 Step 5). Outside an active transition
/// this is a total pass-through — same paint output as not wrapping at all.
/// While `ScreenTransitionView` has a transition in flight, a `Hero` on the
/// outgoing screen and one with the SAME tag on the incoming screen morph
/// into a single floating copy that flies between their two rects; see
/// `hero.rs` for the mechanism.
pub struct Hero<W: Widget> {
    tag: String,
    inner: W,
}

impl<W: Widget> Hero<W> {
    pub fn new(tag: impl Into<String>, inner: W) -> Self {
        Self { tag: tag.into(), inner }
    }
}

impl<W: Widget + Send + Sync + 'static> Widget for Hero<W> {
    fn children(&self) -> Children<'_> {
        Children::One(&self.inner)
    }

    fn layout(&self, ctx: &LayoutCtx) -> rosace_core::types::Size {
        // Own node, matching `paint`'s `paint_child` — see
        // `LayoutCtx::layout_child_uncached`.
        ctx.layout_child_uncached(ctx.constraints, &self.inner)
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let rect = ctx.rect;
        match hero::active_role() {
            Some(role) => {
                // Never replay a hero mid-flight, in either form.
                //
                // This `paint` has a SIDE EFFECT — it suppresses its own
                // drawing and registers a captured picture for
                // `ScreenTransitionView` to fly — and its output depends on
                // ambient state (`hero::active_role()`) rather than on this
                // widget's own config. A cached picture is therefore the
                // SUPPRESSED one, and replaying it after the transition settles
                // draws nothing.
                //
                // `request_animation` is the existing "always re-run me" signal,
                // and it is honest here: a hero in flight is animating.
                ctx.request_animation();
                // Suppressed here — captured instead. `ScreenTransitionView`
                // paints the morphed floating copy on top once both sides
                // of the transition have painted.
                let picture = ctx.capture(rect, |cctx| self.inner.paint(cctx));
                hero::register(self.tag.clone(), role, rect, picture);
            }
            None => {
                ctx.paint_child(rect, &self.inner);
            }
        }
    }
}

/// Builder sugar: `.hero_tag("id")` on any widget. Blanket-implemented,
/// same shape as [`super::OverlayApi`]/[`super::PressApi`].
pub trait HeroApi: Widget + Sized + Send + Sync + 'static {
    fn hero_tag(self, tag: impl Into<String>) -> Hero<Self> {
        Hero::new(tag, self)
    }
}

impl<W: Widget + Send + Sync + 'static> HeroApi for W {}
