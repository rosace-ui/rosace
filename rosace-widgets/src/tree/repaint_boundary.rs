use std::sync::Mutex;

use rosace_render::Picture;
use super::{Widget, Children, PaintCtx};

/// Caches an expensive subtree's Picture and replays it without re-running
/// the child's `paint()` — for large mostly-static content (chart backdrops,
/// icon grids). Records once and re-records when its rect changes; call
/// [`refresh`](super::refresh) on the node to force one.
///
/// Interactive regions declared inside are recorded at real screen
/// coordinates and persist across replay frames (D091), so clicks still work.
pub struct RepaintBoundary<W: Widget + Send + Sync + 'static> {
    pub child: W,
    cache: Mutex<Option<(rosace_core::types::Rect, Picture)>>,
}

impl<W: Widget + Send + Sync + 'static> RepaintBoundary<W> {
    pub fn new(child: W) -> Self {
        Self { child, cache: Mutex::new(None) }
    }

}

impl<W: Widget + Send + Sync + 'static> Widget for RepaintBoundary<W> {
    fn children(&self) -> Children<'_> { Children::One(&self.child) }

    fn paint(&self, ctx: &mut PaintCtx) {
        let rect = ctx.rect;
        let stale = {
            let cache = self.cache.lock().unwrap();
            match &*cache {
                Some((r, _)) => *r != rect,
                None => true,
            }
        };

        if stale {
            // Record at the real screen rect so hit regions land correctly.
            let child = &self.child;
            let pic = ctx.capture(rect, |cctx| child.paint(cctx));
            *self.cache.lock().unwrap() = Some((rect, pic));
        } else {
            // Preserve the captured sub-node (and its hit regions) this frame.
            ctx.keep_child_slot();
        }

        let cache = self.cache.lock().unwrap();
        if let Some((_, pic)) = &*cache {
            ctx.replay_offset(pic, 0.0, 0.0);
        }
    }
    // layout, flex_factor: protocol defaults delegate to the child.
}
