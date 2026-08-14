//! One measurement per child per frame, whatever the parent is.
//!
//! Layout cost is multiplicative in a tree: a parent that measures its child
//! twice makes a four-deep nesting sixteen times as expensive as it needs to
//! be. `Row` and `Column` did exactly that until 2026-08-14 — a first pass
//! summed the non-flex children's main axis for the flex pool, discarded the
//! sizes, and a second pass re-measured the same children with the *same*
//! constraints to get them back.
//!
//! Nothing caught it, because a double measure produces identical pixels. So
//! this test counts `layout` calls rather than checking output, and it covers
//! every container rather than the one that was broken — the bug is a shape
//! any multi-pass parent can grow back into.
//!
//! Flex children are the deliberate exception and are covered separately, in
//! `Row`/`Column`'s own unit tests: their tight main-axis constraint is not
//! known until the pool has been totalled, so they are measured once, in the
//! second pass only.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Default)]
struct Counts {
    layouts: AtomicUsize,
    paints: AtomicUsize,
}

struct Probe(Arc<Counts>);

impl Widget for Probe {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        self.0.layouts.fetch_add(1, Ordering::SeqCst);
        Size { width: 40.0, height: 20.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.0.paints.fetch_add(1, Ordering::SeqCst);
        ctx.fill_rect(ctx.rect, Color::rgb(1, 2, 3));
    }
}

/// Every container worth checking, keyed by an index the component can match
/// on — `Element` is not `Clone`, so the shape has to be rebuilt per case.
const CASES: [(&str, usize); 7] = [
    ("bare (no parent)", 0),
    ("Container", 1),
    ("Column", 2),
    ("Row", 3),
    ("Stack", 4),
    ("ScrollView", 5),
    ("Column > Container", 6),
];

struct App(Arc<Counts>, usize);

impl Component for App {
    fn build(&self, _ctx: &mut Context) -> Element {
        let p = || Probe(self.0.clone());
        match self.1 {
            0 => p().into_element(),
            1 => Container::new().child(p()).into_element(),
            2 => Column::new().child(p()).into_element(),
            3 => Row::new().child(p()).into_element(),
            4 => Stack::new().child(p()).into_element(),
            5 => ScrollView::new(p()).into_element(),
            _ => Column::new().child(Container::new().child(p())).into_element(),
        }
    }
}

fn one_frame(kind: usize) -> (usize, usize) {
    let counts = Arc::new(Counts::default());
    let mut engine = FrameEngine::new(
        Box::new(App(counts.clone(), kind)),
        FontCache::embedded(),
    );
    let (mut a, mut b) = (SkiaCanvas::new(300, 400), SkiaCanvas::new(300, 400));
    engine.paint(&mut a, &mut b, &[]);
    (
        counts.layouts.load(Ordering::SeqCst),
        counts.paints.load(Ordering::SeqCst),
    )
}

#[test]
fn no_container_measures_its_child_more_than_once_per_frame() {
    let mut over_measured = Vec::new();
    for (name, kind) in CASES {
        let (layouts, paints) = one_frame(kind);
        assert_eq!(paints, 1, "{name}: painted {paints} times, expected 1");
        if layouts != 1 {
            over_measured.push(format!("{name}: {layouts} layouts"));
        }
    }
    assert!(
        over_measured.is_empty(),
        "these parents measure their child more than once per frame — layout \
         cost compounds with nesting depth, so this is not cosmetic: {}",
        over_measured.join(", ")
    );
}
