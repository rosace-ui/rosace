use std::sync::{Arc, Mutex};

use tezzera_core::types::{Point, Rect};
use tezzera_nav::ScreenTransition;
use tezzera_render::DrawCommand;
use super::{BoxedWidget, LayoutCtx, PaintCtx, Widget, avail_h, avail_w, intersect_rect};

/// Paints the current screen, and — while a `ScreenNav`-driven transition
/// is in progress — the previous screen too, each offset by the shared
/// `ScreenTransition`'s spring-eased enter/exit values (D108/Phase 26 Step
/// 3). Not generic over the app's route enum: it only needs already-built
/// widgets plus the transition handle `ScreenNav::transition_handle()`
/// returns, the same way `ScrollView` needs only a `ScrollController`, not
/// the app's own types.
///
/// `tzr new`'s generated `app.rs` uses this in place of handing the
/// current screen's widget straight to `Scaffold::new(...)`.
pub struct ScreenTransitionView {
    incoming: BoxedWidget,
    outgoing: Option<BoxedWidget>,
    transition: Arc<Mutex<ScreenTransition>>,
}

impl ScreenTransitionView {
    pub fn new(
        incoming: impl Widget + 'static,
        outgoing: Option<BoxedWidget>,
        transition: Arc<Mutex<ScreenTransition>>,
    ) -> Self {
        Self { incoming: Box::new(incoming), outgoing, transition }
    }
}

impl Widget for ScreenTransitionView {
    fn layout(&self, ctx: &LayoutCtx) -> tezzera_core::types::Size {
        let constraints = ctx.constraints;
        tezzera_core::types::Size { width: avail_w(constraints), height: avail_h(constraints) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let vp = ctx.rect;
        let dt = tezzera_animate::frame_dt().max(0.0001);

        let (ex, ey, ox, oy, _progress, is_complete) = {
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

            if let Some(outgoing) = &self.outgoing {
                let rect = Rect { origin: Point { x: vp.origin.x + ox, y: vp.origin.y + oy }, size: vp.size };
                let mut child_ctx = ctx.child(rect);
                child_ctx.clip_rect = Some(effective_clip);
                outgoing.paint(&mut child_ctx);
            }

            let rect = Rect { origin: Point { x: vp.origin.x + ex, y: vp.origin.y + ey }, size: vp.size };
            let mut child_ctx = ctx.child(rect);
            child_ctx.clip_rect = Some(effective_clip);
            self.incoming.paint(&mut child_ctx);

            ctx.record(DrawCommand::PopClip);
            ctx.request_animation();
        } else {
            // Steady state — paint only the incoming screen at zero offset,
            // identical output to handing it straight to Scaffold::new(...).
            self.incoming.paint(&mut ctx.child(vp));
        }
    }
}
