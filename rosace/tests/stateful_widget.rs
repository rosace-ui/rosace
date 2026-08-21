//! `StatefulWidget` end to end: refresh rebuilds, dispose fires, and neither
//! depends on an `Atom`.

use rosace::prelude::*;
use rosace::widgets::tree::{refresh_state, LayoutCtx, PaintCtx, StatefulExt, StatefulWidget};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Leaf(u32);
impl Widget for Leaf {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 50.0, height: 20.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(self.0 as u8, 0, 0));
        ctx.register_hit(Arc::new(|| refresh_state()));
    }
}

struct Panel {
    count: Arc<AtomicUsize>,
    builds: Arc<AtomicUsize>,
    disposed: Arc<AtomicUsize>,
}

impl StatefulWidget for Panel {
    fn build(&self) -> rosace::widgets::tree::BoxedWidget {
        self.builds.fetch_add(1, Ordering::SeqCst);
        Arc::new(Leaf(self.count.load(Ordering::SeqCst) as u32))
    }
    fn on_dispose(&self) { self.disposed.fetch_add(1, Ordering::SeqCst); }
}

struct App {
    count: Arc<AtomicUsize>,
    builds: Arc<AtomicUsize>,
    disposed: Arc<AtomicUsize>,
    show: Arc<std::sync::atomic::AtomicBool>,
}

impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        if !self.show.load(Ordering::SeqCst) {
            return Column::new().boxed();
        }
        Column::new()
            .child(Panel {
                count: self.count.clone(),
                builds: self.builds.clone(),
                disposed: self.disposed.clone(),
            }.stateful())
            .boxed()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    count: Arc<AtomicUsize>,
    builds: Arc<AtomicUsize>,
    disposed: Arc<AtomicUsize>,
    show: Arc<std::sync::atomic::AtomicBool>,
}

fn harness() -> H {
    let (count, builds, disposed) = (
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
    );
    let show = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let e = FrameEngine::new(
        Box::new(App {
            count: count.clone(), builds: builds.clone(),
            disposed: disposed.clone(), show: show.clone(),
        }),
        FontCache::embedded(),
    );
    H { e, a: SkiaCanvas::new(300, 300), b: SkiaCanvas::new(300, 300),
        count, builds, disposed, show }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
}

/// The core promise: marking the widget dirty re-runs `build`, with no atom
/// and no handle threaded through anything.

/// `dirty_set`'s global-dirty flag is PROCESS-wide, so a test that forces a
/// rebuild makes any concurrently-running test's frame structural. Serialise
/// the tests in this binary that depend on frame classification.
static FRAME_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn refresh_state_rebuilds_the_widget() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    assert_eq!(h.builds.load(Ordering::SeqCst), 1, "built once on the first frame");

    h.frame();
    assert_eq!(h.builds.load(Ordering::SeqCst), 1,
        "an idle frame must not rebuild");

    // Click the leaf, whose handler calls refresh_state().
    let rect = h.e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Leaf")).and_then(|n| n.rect)
        .expect("the leaf painted");
    let (cx, cy) = (rect.origin.x + 1.0, rect.origin.y + 1.0);
    h.count.fetch_add(1, Ordering::SeqCst);
    h.e.paint(&mut h.a, &mut h.b, &[
        rosace_platform::InputEvent::MouseDown { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
        rosace_platform::InputEvent::MouseUp   { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
    ]);
    h.frame();

    assert_eq!(h.builds.load(Ordering::SeqCst), 2,
        "refresh_state() from inside a handler must rebuild the widget that registered it");
}

/// Leaving the tree must fire `on_dispose`, so subscriptions can be released.
#[test]
fn leaving_the_tree_fires_on_dispose() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    assert_eq!(h.disposed.load(Ordering::SeqCst), 0, "still mounted");

    h.show.store(false, Ordering::SeqCst);
    rosace_state::dirty_set::reset_to_global_dirty();
    h.frame();

    assert_eq!(h.disposed.load(Ordering::SeqCst), 1,
        "a widget removed from the tree never got its dispose callback — anything it \
         subscribed to would leak");
}
