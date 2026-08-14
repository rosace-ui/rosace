//! Stage 0 — pins TODAY's caching behaviour before the refresh work.
//!
//! See `.steering/PLAN_REFRESHABLE_WIDGETS.md`. The two files this work
//! changes most — `rosace/src/lib.rs` (the walker) and
//! `rosace-widgets/src/tree/mod.rs` (`PaintCtx`) — have **no tests at all**,
//! and nothing anywhere asserts caching behaviour: not a cache hit, not a
//! picture replay, not a skipped layout.
//!
//! That is the wrong shape for this change. A stale cache produces
//! correct-looking pixels on the frame you inspect, so a test that checks
//! output cannot catch it. These tests assert **which path ran**, by
//! counting real `layout`/`paint` calls on a probe widget.
//!
//! # Some of these are meant to INVERT
//!
//! `one_atom_change_currently_reprocesses_every_widget` documents a
//! deficiency, not a guarantee. When Stage 2 lands it MUST fail, and the fix
//! is to rewrite it to assert the new behaviour — not to make it pass again.
//! It is named and commented so nobody "repairs" it back.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

/// Counts the work a widget is actually asked to do.
#[derive(Default)]
struct Counts {
    layouts: AtomicUsize,
    paints: AtomicUsize,
}

impl Counts {
    fn read(&self) -> (usize, usize) {
        (self.layouts.load(Ordering::SeqCst), self.paints.load(Ordering::SeqCst))
    }
}

struct Probe(Arc<Counts>);

impl Widget for Probe {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        self.0.layouts.fetch_add(1, Ordering::SeqCst);
        Size { width: 40.0, height: 20.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.0.paints.fetch_add(1, Ordering::SeqCst);
        ctx.fill_rect(ctx.rect, Color::rgb(10, 20, 30));
    }
}

const PROBES: usize = 10;

struct App {
    counts: Arc<Counts>,
    tick: Arc<OnceLock<Atom<i32>>>,
}

impl Component for App {
    fn build(&self, ctx: &mut Context) -> Element {
        let t: Atom<i32> = ctx.state(0);
        let _ = self.tick.set(t);
        // Probes nested inside a Column — the shape Stage 2 targets, where
        // children are painted through `PaintCtx::child` rather than by the
        // element walker.
        let mut col = Column::new();
        for _ in 0..PROBES {
            col = col.child(Probe(self.counts.clone()));
        }
        col.into_element()
    }
}

struct Harness {
    engine: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    counts: Arc<Counts>,
    tick: Arc<OnceLock<Atom<i32>>>,
}

fn harness() -> Harness {
    let counts = Arc::new(Counts::default());
    let tick: Arc<OnceLock<Atom<i32>>> = Arc::new(OnceLock::new());
    let engine = FrameEngine::new(
        Box::new(App { counts: counts.clone(), tick: tick.clone() }),
        FontCache::embedded(),
    );
    Harness {
        engine,
        a: SkiaCanvas::new(300, 400),
        b: SkiaCanvas::new(300, 400),
        counts,
        tick,
    }
}

impl Harness {
    fn frame(&mut self) {
        self.engine.paint(&mut self.a, &mut self.b, &[]);
    }
}

/// A frame with nothing dirty must do NO widget work.
///
/// This is the one cheap path that already works, via the `needs_paint`
/// early return. Stage 2 must not regress it while adding finer skipping.
#[test]
fn a_clean_frame_does_no_widget_work_at_all() {
    let mut h = harness();
    h.frame();
    let after_first = h.counts.read();
    assert!(after_first.0 > 0 && after_first.1 > 0, "the first frame must do real work");

    h.frame();
    assert_eq!(h.counts.read(), after_first,
        "a frame with nothing dirty must not touch a single widget");

    h.frame();
    assert_eq!(h.counts.read(), after_first, "and must keep not touching them");
}

/// TODAY'S DEFICIENCY, pinned so the fix is visible.
///
/// One atom change marks the root dirty, `subtree_dirty` propagates to every
/// node, and both caches (layout and picture replay) consult `!paint_dirty`
/// — so every widget re-layouts and re-records, however far it is from the
/// thing that changed.
///
/// **This test MUST fail when Stage 2 lands.** Rewrite it then to assert
/// that only the refreshed node re-runs; do not make it pass again.
#[test]
fn one_atom_change_currently_reprocesses_every_widget() {
    let mut h = harness();
    h.frame();
    let (l0, p0) = h.counts.read();

    h.tick.get().expect("the atom is captured during build").set(1);
    h.frame();
    let (l1, p1) = h.counts.read();

    assert_eq!(p1 - p0, PROBES,
        "every probe repainted — Stage 2 should reduce this to the changed one");
    assert!(l1 > l0, "and every probe re-laid-out");
}

/// Layout runs TWICE per widget per frame: `Column::layout` measures its
/// children, then `Column::paint` measures them again to place them.
///
/// Pinned as an observation, not an endorsement. It doubles the cost of
/// every layout pass, and Stage 2's per-node `cached_size` is what would let
/// the second measure hit a cache instead of re-running.
#[test]
fn layout_runs_twice_per_widget_per_frame() {
    let mut h = harness();
    h.frame();
    let (layouts, paints) = h.counts.read();
    assert_eq!(paints, PROBES, "one paint each");
    assert_eq!(layouts, PROBES * 2,
        "measured twice each — once to size the Column, once to place them");
}

/// The atom's own dedup: writing the SAME value must not schedule a frame,
/// so it must not cause any widget work either.
#[test]
fn writing_an_unchanged_value_does_no_work() {
    let mut h = harness();
    h.frame();
    let before = h.counts.read();

    let atom = h.tick.get().expect("captured").clone();
    atom.set(0); // already 0
    h.frame();

    assert_eq!(h.counts.read(), before,
        "an unchanged write must not dirty anything");
}
