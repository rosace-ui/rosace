//! `StatefulWidget` lifecycle: mount once, dispose once.
//!
//! Written to FAIL against the pre-Stage-3 engine, where `Stateful<T>` kept
//! `built` and `node` on the WIDGET INSTANCE. A structural rebuild constructs a
//! fresh `Stateful`, so its `node` is `None`, so every rebuild looked like a
//! first paint: `on_mount` fired again and another `on_dispose` closure was
//! pushed onto the node (`begin` never clears `dispose`). N rebuilds meant N
//! mounts and N disposals.
//!
//! This is the same lesson Flutter encodes by putting `State` on the Element
//! rather than the Widget — the widget is config, thrown away every rebuild.

use rosace::prelude::*;
use rosace::widgets::tree::{BoxedWidget, LayoutCtx, PaintCtx, StatefulExt, StatefulWidget};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

struct Leaf;
impl Widget for Leaf {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 40.0, height: 20.0 } }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(4, 5, 6)); }
}

#[derive(Clone, Default)]
struct Counts {
    mounts: Arc<AtomicUsize>,
    disposes: Arc<AtomicUsize>,
    builds: Arc<AtomicUsize>,
}

struct Panel(Counts);
impl StatefulWidget for Panel {
    fn build(&self) -> BoxedWidget {
        self.0.builds.fetch_add(1, Ordering::SeqCst);
        Arc::new(Leaf)
    }
    fn on_mount(&self) { self.0.mounts.fetch_add(1, Ordering::SeqCst); }
    fn on_dispose(&self) { self.0.disposes.fetch_add(1, Ordering::SeqCst); }
}

struct App {
    counts: Counts,
    show: Arc<AtomicBool>,
}

impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let col = Column::new();
        if self.show.load(Ordering::SeqCst) {
            col.child(Panel(self.counts.clone()).stateful()).boxed()
        } else {
            col.boxed()
        }
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    counts: Counts,
    show: Arc<AtomicBool>,
}

fn harness() -> H {
    let counts = Counts::default();
    let show = Arc::new(AtomicBool::new(true));
    let e = FrameEngine::new(
        Box::new(App { counts: counts.clone(), show: show.clone() }),
        FontCache::embedded(),
    );
    H { e, a: SkiaCanvas::new(200, 200), b: SkiaCanvas::new(200, 200), counts, show }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn rebuild(&mut self) {
        rosace_state::dirty_set::reset_to_global_dirty();
        self.frame();
    }
}

/// `on_mount` means "this widget entered the tree", not "a widget object was
/// constructed". Rebuilding the tree around it must not re-mount it.
#[test]
fn on_mount_fires_once_no_matter_how_often_the_tree_rebuilds() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    assert_eq!(h.counts.mounts.load(Ordering::SeqCst), 1, "mounted on the first frame");

    h.rebuild();
    h.rebuild();
    h.rebuild();

    assert_eq!(h.counts.mounts.load(Ordering::SeqCst), 1,
        "on_mount fired again on a rebuild — a widget that subscribes in on_mount \
         would open one subscription per rebuild");
}

/// The other half: dispose handlers must not accumulate. Every extra rebuild
/// used to push another closure onto the node, so leaving the tree fired the
/// callback once per rebuild that had ever happened.
#[test]
fn on_dispose_fires_once_however_many_rebuilds_preceded_it() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    h.rebuild();
    h.rebuild();
    assert_eq!(h.counts.disposes.load(Ordering::SeqCst), 0, "still mounted");

    h.show.store(false, Ordering::SeqCst);
    h.rebuild();

    assert_eq!(h.counts.disposes.load(Ordering::SeqCst), 1,
        "dispose ran once per rebuild rather than once per removal — a widget \
         cancelling a subscription would cancel it repeatedly");
}

/// And the widget must still actually work: leaving and re-entering the tree is
/// a genuine new mount, not a suppressed one.
#[test]
fn re_entering_the_tree_mounts_again() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    h.show.store(false, Ordering::SeqCst);
    h.rebuild();
    assert_eq!(h.counts.disposes.load(Ordering::SeqCst), 1);

    h.show.store(true, Ordering::SeqCst);
    h.rebuild();

    assert_eq!(h.counts.mounts.load(Ordering::SeqCst), 2,
        "a widget that left and came back is a new mount");
}
