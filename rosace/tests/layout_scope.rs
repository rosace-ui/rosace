//! One widget changing must not re-LAYOUT its siblings.
//!
//! The mirror of `repaint_scope.rs`, which asserts the same thing for paint.
//! Written deliberately to FAIL against the pre-Stage-1 engine: today
//! `last_constraints`/`cached_size` are consulted in exactly one place —
//! `rosace/src/lib.rs:424`, the single element boundary — so every widget
//! inside a component's tree re-measures on every non-idle frame.
//!
//! Counting `layout` calls rather than comparing pixels is the point. A stale
//! or missing layout cache produces correct-looking output while doing the
//! work, so only "which path ran" can measure it.

use rosace::prelude::*;
use rosace::widgets::tree::{refresh_state, LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// `dirty_set`'s global-dirty flag is PROCESS-wide, so a test that forces a
/// rebuild makes any concurrently-running test's frame structural — and a
/// structural frame ignores the caches these tests exist to measure. Same
/// hazard, and same fix, as `repaint_scope.rs`.
static FRAME_STATE: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const N: usize = 10;

/// Counts its own `layout` calls. `paint` registers a hit that refreshes this
/// node, so a click drives the real mark-dirty pipeline rather than a test hook.
struct Probe(Arc<Vec<AtomicUsize>>, usize);

impl Widget for Probe {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        self.0[self.1].fetch_add(1, Ordering::SeqCst);
        Size { width: 100.0, height: 20.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(10, 20, 30));
        ctx.register_hit(Arc::new(|| refresh_state()));
    }
}

struct App(Arc<Vec<AtomicUsize>>);
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> Element {
        let mut col = Column::new();
        for i in 0..N {
            col = col.child(Probe(self.0.clone(), i));
        }
        col.into_element()
    }
}

fn counts(c: &Arc<Vec<AtomicUsize>>) -> Vec<usize> {
    c.iter().map(|a| a.load(Ordering::SeqCst)).collect()
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    c: Arc<Vec<AtomicUsize>>,
}

fn harness() -> H {
    let c: Arc<Vec<AtomicUsize>> = Arc::new((0..N).map(|_| AtomicUsize::new(0)).collect());
    let e = FrameEngine::new(Box::new(App(c.clone())), FontCache::embedded());
    H { e, a: SkiaCanvas::new(300, 400), b: SkiaCanvas::new(300, 400), c }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn event(&mut self, ev: &[rosace_platform::InputEvent]) {
        self.e.paint(&mut self.a, &mut self.b, ev);
    }
    /// Centre of the i-th probe's painted rect.
    fn probe_centre(&self, i: usize) -> (f32, f32) {
        let tree = self.e.inspect_tree();
        let probes: Vec<_> = tree.iter().filter(|n| n.tag.ends_with("::Probe")).collect();
        assert_eq!(probes.len(), N, "each probe should own a node");
        let r = probes[i].rect.expect("a painted probe has a rect");
        (r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0)
    }
}

/// The core claim. Marking ONE node dirty must re-measure that node and let
/// the other nine answer from `cached_size`.
///
/// Today every probe re-lays-out, because there is no per-node layout cache
/// below the element boundary.
#[test]
fn refreshing_one_node_relayouts_only_that_node() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    let first = counts(&h.c);
    assert!(first.iter().all(|&n| n >= 1), "first frame measures everything: {first:?}");

    // Click probe 3, whose hit handler calls refresh_state() — the real
    // mark-dirty path, not a test hook.
    let (cx, cy) = h.probe_centre(3);
    h.event(&[
        rosace_platform::InputEvent::MouseDown { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
        rosace_platform::InputEvent::MouseUp   { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
    ]);
    h.frame();

    let after = counts(&h.c);
    let relaid: Vec<usize> = (0..N).filter(|&i| after[i] > first[i]).collect();
    assert_eq!(relaid, vec![3],
        "only the refreshed node should have re-measured; re-laid-out = {relaid:?}, \
         counts = {after:?}");
}

/// Hover changes appearance, never size — `LayoutCtx` cannot reach `hovered()`,
/// so it is provably impossible for a hover to alter a measurement. Nothing
/// should re-measure at all.
#[test]
fn hovering_relayouts_nothing() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    let first = counts(&h.c);

    let (cx, cy) = h.probe_centre(3);
    h.event(&[rosace_platform::InputEvent::MouseMove { x: cx, y: cy }]);
    h.frame();

    let after = counts(&h.c);
    assert_eq!(after, first,
        "a hover re-measured widgets, but size cannot depend on hover state: {after:?}");
}

/// The safety half, mirroring `repaint_scope.rs`. A rebuild makes every widget
/// object new, and a node cannot tell a fresh-but-identical widget from a
/// changed one — so a structural frame must re-measure everything.
#[test]
fn a_rebuild_relayouts_everything_because_caches_cannot_be_trusted() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    let first = counts(&h.c);

    rosace_state::dirty_set::reset_to_global_dirty();
    h.frame();

    let after = counts(&h.c);
    for i in 0..N {
        assert!(after[i] > first[i],
            "probe {i} reused a cached size across a rebuild — that is a stale-cache bug, \
             not an optimisation: {after:?}");
    }
}
