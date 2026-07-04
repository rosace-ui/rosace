use std::sync::Mutex;

use tezzera_render::Picture;
use tezzera_state::Atom;
use super::{Widget, Children, PaintCtx};

/// Caches an expensive subtree's Picture and replays it without re-running
/// the child's `paint()` — for large mostly-static content (chart backdrops,
/// icon grids). Records once and holds by default; pass `.repaint_when(atom)`
/// to re-record whenever that atom changes.
///
/// Interactive regions declared inside are recorded at real screen
/// coordinates and persist across replay frames (D091), so clicks still work.
pub struct RepaintBoundary<W: Widget + Send + Sync + 'static> {
    pub child: W,
    repaint_when: Vec<Atom<u64>>,
    cache: Mutex<Option<(tezzera_core::types::Rect, Picture, Vec<u64>)>>,
}

impl<W: Widget + Send + Sync + 'static> RepaintBoundary<W> {
    pub fn new(child: W) -> Self {
        Self { child, repaint_when: Vec::new(), cache: Mutex::new(None) }
    }

    /// Re-record whenever `atom` changes. Chain multiple.
    pub fn repaint_when(mut self, atom: Atom<u64>) -> Self {
        self.repaint_when.push(atom);
        self
    }
}

impl<W: Widget + Send + Sync + 'static> Widget for RepaintBoundary<W> {
    fn children(&self) -> Children<'_> { Children::One(&self.child) }

    fn paint(&self, ctx: &mut PaintCtx) {
        let rect = ctx.rect;
        let keys: Vec<u64> = self.repaint_when.iter().map(|a| a.get()).collect();

        let stale = {
            let cache = self.cache.lock().unwrap();
            match &*cache {
                Some((r, _, k)) => *r != rect || *k != keys,
                None => true,
            }
        };

        if stale {
            // Record at the real screen rect so hit regions land correctly.
            let child = &self.child;
            let pic = ctx.capture(rect, |cctx| child.paint(cctx));
            *self.cache.lock().unwrap() = Some((rect, pic, keys));
        } else {
            // Preserve the captured sub-node (and its hit regions) this frame.
            ctx.keep_child_slot();
        }

        let cache = self.cache.lock().unwrap();
        if let Some((_, pic, _)) = &*cache {
            ctx.replay_offset(pic, 0.0, 0.0);
        }
    }
    // layout, flex_factor: protocol defaults delegate to the child.
}
