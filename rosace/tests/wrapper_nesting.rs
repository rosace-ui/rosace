//! Every wrapper × every content shape, painted through the real engine.
//!
//! # Why this exists
//!
//! Per-node layout caching made `layout` consume node slots. A wrapper whose
//! `layout` delegates with its own ctx then leaks its CHILD's slots into its
//! OWN node, so layout fills them with grandchildren while paint puts the child
//! at slot 0. The result is a child inheriting a sibling's cached size —
//! silently, and rendering plausibly.
//!
//! Three of these shipped in one session and were found by launching the
//! showcase and clicking: `Responsive`, `Semantics<W>`, `InteractiveViewer<W>`.
//! Not one was caught by 1600+ unit tests, because each needs a specific
//! nesting the tests never happened to build.
//!
//! So this does not test a behaviour — it **enumerates the shapes**. Wrappers
//! are combinatorial: any wrapper may contain any content, and the bug lives in
//! the pair, not in either half. Checking the product is the only way to cover
//! the class rather than the instances found so far.
//!
//! # What counts as a failure
//!
//! `paint_child`'s slot-misalignment `debug_assert!` panics on a mismatch, so a
//! bad pair fails this test by name in debug builds. There is deliberately no
//! pixel comparison: a misaligned cache produces plausible output, which is the
//! entire reason this class of bug survives normal testing.

use rosace::prelude::*;
use rosace::widgets::tree::{BoxedWidget, LayoutCtx, PaintCtx, StatefulExt, StatefulWidget};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Arc, Mutex, MutexGuard};

/// `dirty_set`'s global flag is process-wide; serialise as the other
/// frame-classification tests do.
static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

struct Leaf;
impl Widget for Leaf {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 60.0, height: 24.0 } }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(7, 8, 9)); }
}

struct Panel;
impl StatefulWidget for Panel {
    fn build(&self) -> BoxedWidget {
        // Deliberately a MULTI-CHILD subtree. A childless leaf hides slot
        // leaking entirely — which is exactly why the original `Stateful`
        // tests passed while the bug was live.
        Arc::new(Row::new().child(Leaf).child(Leaf))
    }
}

/// Content shapes a wrapper might contain. Each has children of its own, so a
/// leaked slot has somewhere to leak to.
fn contents() -> Vec<(&'static str, fn() -> BoxedWidget)> {
    vec![
        ("Row",       || Arc::new(Row::new().child(Leaf).child(Leaf)) as BoxedWidget),
        ("Column",    || Arc::new(Column::new().child(Leaf).child(Leaf)) as BoxedWidget),
        ("Container", || Arc::new(Container::new().child(Row::new().child(Leaf))) as BoxedWidget),
        ("Stack",     || Arc::new(Stack::new().child(Leaf).child(Leaf)) as BoxedWidget),
        ("Card",      || Arc::new(Card::new(Column::new().child(Leaf).child(Leaf))) as BoxedWidget),
    ]
}

/// Wrappers — the widgets that delegate layout to a child they also paint.
fn wrappers() -> Vec<(&'static str, fn(BoxedWidget) -> BoxedWidget)> {
    vec![
        ("bare",              |c| c),
        ("Semantics",         |c| Arc::new(Semantics::new(c)) as BoxedWidget),
        ("Pressable",         |c| Arc::new(Pressable::new(c, || {})) as BoxedWidget),
        ("ScrollView",        |c| Arc::new(ScrollView::new(c)) as BoxedWidget),
        ("Container",         |c| Arc::new(Container::new().child(c)) as BoxedWidget),
        ("Card",              |c| Arc::new(Card::new(c)) as BoxedWidget),
        ("Hero",              |c| Arc::new(Hero::new("t", c)) as BoxedWidget),
        ("Dismissible",       |c| Arc::new(Dismissible::new(c)) as BoxedWidget),
        ("PullToRefresh",     |c| Arc::new(PullToRefresh::new(c)) as BoxedWidget),
        ("InteractiveViewer", |c| Arc::new(InteractiveViewer::new(c)) as BoxedWidget),
        ("Responsive",        |_| Arc::new(Responsive::new(|_| {
            Arc::new(Row::new().child(Leaf).child(Leaf)) as BoxedWidget
        })) as BoxedWidget),
        ("Stateful",          |_| Arc::new(Panel.stateful()) as BoxedWidget),
    ]
}

/// Holds a BUILDER, not a widget. `build` runs again on every rebuild, and a
/// structural frame is one of the cases under test, so the widget has to be
/// constructible repeatedly.
struct One(Arc<dyn Fn() -> BoxedWidget + Send + Sync>);
impl Component for One {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let w = (self.0)();
        // Bounded by a fixed-size Container. An outer ScrollView hands down an
        // unbounded height, which used to make a nested scrollable run away
        // (`ScrollView x Card` OOM-killed the process). That is fixed —
        // `ScrollView` shrink-wraps on an unbounded axis now — but this shallow
        // host is kept as the fast baseline. `deep_host` below covers the real
        // shape.
        Container::new().size(300.0, 220.0).child(
            Column::new().children(vec![w])
        ).boxed()
    }
}

/// The shape a real app actually builds: several levels deep, inside a
/// composited scrollable, under app chrome.
///
/// **This is the gap that let two regressions through.** The matrix above builds
/// shallow, non-composited trees; the showcase builds deep composited ones, and
/// both a click failure and a stuck-hover lived precisely in the difference. A
/// widget inside a GPU-composited `ScrollView` has its rects in CONTENT space,
/// and a widget several levels down is behind ancestors that may replay — two
/// conditions the shallow host never creates.
struct Deep(Arc<dyn Fn() -> BoxedWidget + Send + Sync>);
impl Component for Deep {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let w = (self.0)();
        let mut col = Column::new().children(vec![w]);
        // Enough rows to overflow the viewport, so the ScrollView actually
        // composites rather than falling back to the base path.
        for _ in 0..10 {
            col = col.child(Container::new().size(200.0, 40.0));
        }
        Scaffold::new(
            Container::new().child(
                Column::new().child(ScrollView::new(col))
            )
        ).boxed()
    }
}

/// Paint one wrapper/content pair through the real engine for a few frames.
fn exercise(build: impl Fn() -> BoxedWidget + Send + Sync + 'static) {
    run(FrameEngine::new(Box::new(One(Arc::new(build))), FontCache::embedded()));
}

/// The same passes, but through the deep composited host.
fn exercise_deep(build: impl Fn() -> BoxedWidget + Send + Sync + 'static) {
    run(FrameEngine::new(Box::new(Deep(Arc::new(build))), FontCache::embedded()));
}

fn run(mut e: FrameEngine) {
    let (mut a, mut b) = (SkiaCanvas::new(320, 240), SkiaCanvas::new(320, 240));

    // First frame builds everything; the second exercises the CACHED paths,
    // which is where a leaked slot actually bites.
    e.paint(&mut a, &mut b, &[]);
    e.paint(&mut a, &mut b, &[]);

    // A hover produces a targeted frame — caches live, nothing rebuilt.
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove { x: 40.0, y: 40.0 }]);
    e.paint(&mut a, &mut b, &[]);

    // A rebuild makes the frame structural — caches ignored, everything new.
    rosace_state::dirty_set::reset_to_global_dirty();
    e.paint(&mut a, &mut b, &[]);

    // And a scroll, which moves content under everything below.
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::Scroll {
        x: 100.0, y: 100.0, delta_x: 0.0, delta_y: -60.0,
    }]);
    e.paint(&mut a, &mut b, &[]);
}

#[test]
fn every_wrapper_around_every_content_shape_paints_without_slot_misalignment() {
    let _guard = exclusive();
    let mut checked = 0usize;

    for (wname, wrap) in wrappers() {
        for (cname, content) in contents() {
            exercise(move || wrap(content()));
            checked += 1;
            // Printed so a panic's preceding line names the pair — the panic
            // itself reports `paint_child`, not the widget at fault.
            eprintln!("ok: {wname} × {cname}");
        }
    }

    assert!(checked >= 50, "expected the full product to run, got {checked}");
}

/// Every wrapper again, this time deep inside a composited scrollable under app
/// chrome — the shape the showcase actually builds, and the one the shallow
/// matrix misses.
#[test]
fn every_wrapper_survives_a_deep_composited_host() {
    let _guard = exclusive();
    let mut checked = 0usize;

    for (wname, wrap) in wrappers() {
        for (cname, content) in contents() {
            exercise_deep(move || wrap(content()));
            checked += 1;
            eprintln!("deep ok: {wname} × {cname}");
        }
    }

    assert!(checked >= 50, "expected the full product to run, got {checked}");
}
