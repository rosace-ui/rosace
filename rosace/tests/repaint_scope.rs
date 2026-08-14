//! One widget changing must not repaint its siblings.
//!
//! This is the payoff of per-node picture caches, and it is asserted by
//! counting `paint` calls rather than by comparing pixels — a stale cache
//! renders plausible output, so only "which path ran" can catch it.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const N: usize = 10;

struct Probe(Arc<Vec<AtomicUsize>>, usize);
impl Widget for Probe {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 100.0, height: 20.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.0[self.1].fetch_add(1, Ordering::SeqCst);
        ctx.fill_rect(ctx.rect, Color::rgb(10, 20, 30));
        ctx.register_hit(Arc::new(|| {}));
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

/// Marking ONE node dirty on a targeted frame must repaint that node and
/// replay the other nine.
#[test]
fn marking_one_node_dirty_repaints_only_that_node() {
    let c: Arc<Vec<AtomicUsize>> = Arc::new((0..N).map(|_| AtomicUsize::new(0)).collect());
    let mut e = FrameEngine::new(Box::new(App(c.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(300, 400), SkiaCanvas::new(300, 400));

    e.paint(&mut a, &mut b, &[]);
    let first = counts(&c);
    assert!(first.iter().all(|&n| n == 1), "first frame paints everything: {first:?}");

    // Hover the 4th probe by moving the real mouse over it, so this goes
    // through the actual hover pipeline rather than a test hook.
    let rect = {
        let tree = e.inspect_tree();
        let probes: Vec<_> = tree.iter().filter(|n| n.tag.ends_with("::Probe")).collect();
        assert_eq!(probes.len(), N, "each probe should own a node");
        probes[3].rect.expect("a painted probe has a rect")
    };
    let (cx, cy) = (rect.origin.x + rect.size.width / 2.0,
                    rect.origin.y + rect.size.height / 2.0);
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove { x: cx, y: cy }]);
    e.paint(&mut a, &mut b, &[]);

    let after = counts(&c);
    let repainted: Vec<usize> = (0..N).filter(|&i| after[i] > first[i]).collect();
    assert_eq!(repainted, vec![3],
        "only the hovered node should have repainted; repainted = {repainted:?}, counts = {after:?}");
}

/// The safety half. A rebuild makes every widget object new, and a node
/// cannot tell a fresh-but-identical widget from a changed one — so a
/// structural frame must ignore the caches entirely.
#[test]
fn a_rebuild_repaints_everything_because_caches_cannot_be_trusted() {
    let c: Arc<Vec<AtomicUsize>> = Arc::new((0..N).map(|_| AtomicUsize::new(0)).collect());
    let mut e = FrameEngine::new(Box::new(App(c.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(300, 400), SkiaCanvas::new(300, 400));

    e.paint(&mut a, &mut b, &[]);
    let first = counts(&c);

    rosace_state::dirty_set::reset_to_global_dirty();
    e.paint(&mut a, &mut b, &[]);

    let after = counts(&c);
    for i in 0..N {
        assert!(after[i] > first[i],
            "probe {i} replayed a cached picture across a rebuild — that is a stale-cache bug, \
             not an optimisation: {after:?}");
    }
}
