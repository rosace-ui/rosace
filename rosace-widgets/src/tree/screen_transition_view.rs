use std::sync::{Arc, Mutex};

use rosace_core::types::{Point, Rect, Size};
use rosace_nav::ScreenTransition;
use rosace_render::DrawCommand;
use super::hero::{self, HeroRole};
use super::{BoxedWidget, LayoutCtx, PaintCtx, Widget, avail_h, avail_w, intersect_rect};

/// Paints the current screen, and — while a `ScreenNav`-driven transition
/// is in progress — the previous screen too, each offset by the shared
/// `ScreenTransition`'s spring-eased enter/exit values (D108/Phase 26 Step
/// 3). Not generic over the app's route enum: it only needs already-built
/// widgets plus opaque `u64` identity keys (`ScreenNav::current_key`/
/// `previous_key`/`stack_keys`) and the transition handle
/// `ScreenNav::transition_handle()` returns — the same way `ScrollView`
/// needs only a `ScrollController`, not the app's own types.
///
/// # Per-screen persistence (2026-08-01)
/// Each screen's subtree is addressed by its `incoming_key`/`outgoing_key`
/// through `PaintCtx::child_keyed`, not positionally — see
/// `render_tree.rs`'s module doc ("Identity" section) for the full story.
/// Without this, two different screens landing at the same tree position
/// (which they always do here — every screen paints as "the incoming
/// child") would alias scroll offset, animation state, and everything else
/// sticky onto whatever screen last occupied that position: navigating to a
/// new screen could inherit a stale, out-of-bounds scroll offset (visibly
/// springing back to valid bounds on arrival), and navigating back would
/// find its OWN position reset instead of where it was left. With keys,
/// each screen gets its own permanent slot, reused (state intact, exactly
/// where it was left) whenever that screen becomes current again, and
/// released via `valid_keys`/`prune_keyed_children` only once it's actually
/// popped off the nav stack — mirroring Flutter's `Navigator`, which keeps
/// every pushed route's Element tree alive in its `Overlay` until popped,
/// not just the current one.
///
/// `rsc new`'s generated `app.rs` uses this in place of handing the
/// current screen's widget straight to `Scaffold::new(...)`.
pub struct ScreenTransitionView {
    incoming: BoxedWidget,
    incoming_key: u64,
    outgoing: Option<BoxedWidget>,
    outgoing_key: Option<u64>,
    transition: Arc<Mutex<ScreenTransition>>,
    /// Every route currently on the nav stack (`ScreenNav::stack_keys()`) —
    /// anything cached under a key NOT in this list gets released this
    /// frame (see `RenderTree::prune_keyed_children`).
    valid_keys: Vec<u64>,
}

impl ScreenTransitionView {
    pub fn new(
        incoming: impl Widget + 'static,
        incoming_key: u64,
        outgoing: Option<BoxedWidget>,
        outgoing_key: Option<u64>,
        transition: Arc<Mutex<ScreenTransition>>,
        valid_keys: Vec<u64>,
    ) -> Self {
        Self {
            incoming: Arc::new(incoming),
            incoming_key,
            outgoing,
            outgoing_key,
            transition,
            valid_keys,
        }
    }
}

impl Widget for ScreenTransitionView {
    fn layout(&self, ctx: &LayoutCtx) -> rosace_core::types::Size {
        let constraints = ctx.constraints;
        rosace_core::types::Size { width: avail_w(constraints), height: avail_h(constraints) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let vp = ctx.rect;
        let dt = rosace_animate::frame_dt().max(0.0001);

        // Release any screen no longer on the nav stack — see this struct's
        // own doc comment and `RenderTree::prune_keyed_children`.
        ctx.tree.borrow_mut().prune_keyed_children(ctx.node, &self.valid_keys);

        let (ex, ey, ox, oy, progress, is_complete) = {
            let mut t = self.transition.lock().unwrap_or_else(|e| e.into_inner());
            t.set_viewport(vp.size.width, vp.size.height);
            t.update(dt)
        };

        let animating = !is_complete && ctx.theme.animation.enabled;

        if animating {
            // Clip both layers to the viewport — an in-flight slide must
            // not paint outside its own screen's bounds, same reasoning as
            // ScrollView::paint_base's clip around its child.
            ctx.record(DrawCommand::PushClip { rect: vp });
            let effective_clip = ctx.clip_rect.and_then(|parent| intersect_rect(parent, vp)).unwrap_or(vp);

            // D108/Phase 26 Step 5: any `Hero`-tagged widget painted while a
            // role is active captures itself instead of painting in place —
            // see `hero.rs`. Both sides always get marked (even when there's
            // no `outgoing` widget yet, e.g. the very first screen) so a
            // stale role never leaks into an unrelated later paint pass.
            if let Some(outgoing) = &self.outgoing {
                hero::set_active_role(Some(HeroRole::Outgoing));
                let rect = Rect { origin: Point { x: vp.origin.x + ox, y: vp.origin.y + oy }, size: vp.size };
                let mut child_ctx = match self.outgoing_key {
                    Some(key) => ctx.child_keyed(rect, key),
                    None => ctx.child(rect),
                };
                child_ctx.set_clip(Some(effective_clip));
                outgoing.paint(&mut child_ctx);
            }

            hero::set_active_role(Some(HeroRole::Incoming));
            let rect = Rect { origin: Point { x: vp.origin.x + ex, y: vp.origin.y + ey }, size: vp.size };
            let mut child_ctx = ctx.child_keyed(rect, self.incoming_key);
            child_ctx.set_clip(Some(effective_clip));
            self.incoming.paint(&mut child_ctx);
            hero::set_active_role(None);

            // Promote each matched Hero once, above both screens, laid out at
            // a rect LERP'd between its two ends by the transition's progress
            // — the floating "flight" element. It is a LIVE widget: it
            // reflows at each interpolated size and keeps animating.
            //
            // Keyed by tag so a flight keeps its node across frames. With
            // positional slots, a screen with two heroes would hand the
            // second one the first's node the moment either landed, and with
            // it the first's animation state.
            let t = progress.clamp(0.0, 1.0);
            for flight in hero::drain_pairs() {
                let interp = lerp_rect(flight.from, flight.to, t);
                let mut h = std::collections::hash_map::DefaultHasher::new();
                std::hash::Hash::hash(&flight.tag, &mut h);
                let key = std::hash::Hasher::finish(&h);
                ctx.promote_laid_out(key, interp, &*flight.widget);
            }

            ctx.record(DrawCommand::PopClip);
            ctx.request_animation();
        } else {
            // Steady state — paint only the incoming screen at zero offset,
            // identical output to handing it straight to Scaffold::new(...).
            // No active role: `Hero`-tagged widgets are plain pass-throughs.
            self.incoming.paint(&mut ctx.child_keyed(vp, self.incoming_key));
        }
    }
}

/// Linear interpolation between two rects' position AND size at `t` (0..1).
fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect {
        origin: Point {
            x: a.origin.x + (b.origin.x - a.origin.x) * t,
            y: a.origin.y + (b.origin.y - a.origin.y) * t,
        },
        size: Size {
            width: a.size.width + (b.size.width - a.size.width) * t,
            height: a.size.height + (b.size.height - a.size.height) * t,
        },
    }
}
