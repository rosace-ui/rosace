//! Scrolling must not rebuild the app.
//!
//! Written to FAIL against the pre-Stage-4a engine. `ScrollController`'s state
//! is held in `Atom`s that `PaintCtx::scroll_controller` subscribes to the
//! OWNING COMPONENT (`mod.rs:898-900`). There is exactly one component, so a
//! wheel event dirties the root, re-runs `build()`, and makes the frame
//! STRUCTURAL — which makes `paint_child` refuse every replay app-wide and
//! re-lays-out the whole tree.
//!
//! `scroll/controller.rs` says so itself:
//!
//! > the inner atoms are framework-created — nothing subscribes to them by
//! > default, so a scroll_to/wheel write would request a frame that repaints
//! > NOTHING (cache-hit). Subscribing the owning component makes controller
//! > writes dirty it.
//!
//! That was the only tool available before `mark_node_dirty` existed. It does
//! now, so scrolling can mark the scrolling node and leave the rest alone.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const N: usize = 8;

/// Counts its own paints, so we can see whether siblings were forced to repaint.
struct Row(Arc<Vec<AtomicUsize>>, usize);
impl Widget for Row {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 200.0, height: 30.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.0[self.1].fetch_add(1, Ordering::SeqCst);
        ctx.fill_rect(ctx.rect, Color::rgb(30, 40, 50));
    }
}

struct App {
    builds: Arc<AtomicUsize>,
    paints: Arc<Vec<AtomicUsize>>,
}

impl Component for App {
    fn build(&self, _ctx: &mut Context) -> Element {
        self.builds.fetch_add(1, Ordering::SeqCst);
        let mut col = Column::new();
        for i in 0..N {
            col = col.child(Row(self.paints.clone(), i));
        }
        ScrollView::new(col).into_element()
    }
}

/// A wheel event must not re-run `build()`. Today it does, because the scroll
/// offset lives in an `Atom` subscribed to the component.
#[test]
fn scrolling_does_not_rebuild_the_component() {
    let _guard = exclusive();
    let builds = Arc::new(AtomicUsize::new(0));
    let paints: Arc<Vec<AtomicUsize>> = Arc::new((0..N).map(|_| AtomicUsize::new(0)).collect());
    let mut e = FrameEngine::new(
        Box::new(App { builds: builds.clone(), paints: paints.clone() }),
        FontCache::embedded(),
    );
    let (mut a, mut b) = (SkiaCanvas::new(220, 120), SkiaCanvas::new(220, 120));

    e.paint(&mut a, &mut b, &[]);
    let after_first = builds.load(Ordering::SeqCst);
    assert!(after_first >= 1, "the first frame builds");

    // Three wheel notches over the viewport.
    for _ in 0..3 {
        e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::Scroll {
            x: 110.0, y: 60.0, delta_x: 0.0, delta_y: -20.0,
        }]);
        e.paint(&mut a, &mut b, &[]);
    }

    assert_eq!(builds.load(Ordering::SeqCst), after_first,
        "scrolling re-ran the component's build() — every scroll frame is structural, \
         so every per-node cache in the app is bypassed while the user drags");
}
