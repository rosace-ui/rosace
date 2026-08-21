//! A recycled slot must NOT be re-blitted.
//!
//! Replay-on-move re-blits a widget whose size is unchanged and whose origin
//! moved. A virtualized list is the case where that reasoning breaks: as you
//! scroll, slot 0 shows row 5, then row 6 — same size, new position, and
//! COMPLETELY DIFFERENT CONTENT. Re-blitting there paints row 5's pixels where
//! row 6 belongs.
//!
//! The type tag cannot catch it: both rows are the same widget type, so
//! `adopt_tag` sees no change. What distinguishes them is which INDEX painted,
//! so that is what this records.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

type Seen = Arc<Mutex<Vec<usize>>>;

/// Records the row index that actually ran `paint`. A slot that was re-blitted
/// records nothing, which is precisely the signal we are looking for.
struct Probe(usize, Seen);
impl Widget for Probe {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 200.0, height: 40.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.1.lock().unwrap().push(self.0);
        ctx.fill_rect(ctx.rect, Color::rgb((self.0 % 255) as u8, 40, 40));
    }
}

struct App(Seen);
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let seen = self.0.clone();
        ListView::builder(200, 40.0, move |i| Arc::new(Probe(i, seen.clone())) as _)
            .boxed()
    }
}

/// Scroll, then assert that the rows now on screen actually painted themselves
/// rather than inheriting a neighbour's pixels.
#[test]
fn scrolling_a_virtualized_list_repaints_recycled_slots() {
    let _guard = exclusive();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let mut e = FrameEngine::new(Box::new(App(seen.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(240, 200), SkiaCanvas::new(240, 200));

    e.paint(&mut a, &mut b, &[]);
    let first: Vec<usize> = seen.lock().unwrap().clone();
    assert!(first.contains(&0), "the first frame should show row 0, saw {first:?}");

    // Scroll well past the initial window.
    for _ in 0..10 {
        e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::Scroll {
            x: 120.0, y: 100.0, delta_x: 0.0, delta_y: -60.0,
        }]);
        e.paint(&mut a, &mut b, &[]);
    }

    // One more scroll, watching only what that frame paints.
    seen.lock().unwrap().clear();
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::Scroll {
        x: 120.0, y: 100.0, delta_x: 0.0, delta_y: -60.0,
    }]);
    e.paint(&mut a, &mut b, &[]);

    let after: Vec<usize> = seen.lock().unwrap().clone();
    assert!(!after.is_empty(),
        "after scrolling, NO row ran paint — every visible slot was re-blitted, so \
         the list is showing whichever rows those slots used to hold");
    assert!(after.iter().all(|&i| i > 0),
        "row 0 painted again after scrolling far past it: {after:?}");
}
