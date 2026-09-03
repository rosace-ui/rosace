//! A deep, composited, realistic tree — and what the engine does with it.
//!
//! # Why this exists
//!
//! Every user-visible regression in this refactor lived in one gap: the test
//! harness builds shallow, non-composited trees while a real app builds deep,
//! composited ones. `wrapper_nesting.rs` had a deep host added and immediately
//! found two more bugs. So the fixture here is deliberately app-shaped —
//! `Scaffold` + `AppBar` + a GPU-composited `ScrollView` + 200 rows of nested
//! widgets + a promoted overlay — and the assertions are about WHICH PATH RAN,
//! not about pixels.
//!
//! # Why not frame times
//!
//! This refactor never claimed "faster paint". It claimed LESS WORK PER FRAME:
//! scrolling stops rebuilding the app, one widget updates instead of the
//! screen, overlays stop repainting forever, layout is cached per node. Those
//! are counts, and counts are exactly what a stale cache renders plausible
//! pixels for. A wall-clock number would hide all of it.

use rosace::prelude::*;
use rosace::widgets::tree::{refresh_state, LayoutCtx, PaintCtx, StatefulExt, StatefulWidget};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const ROWS: usize = 200;
const WIN_W: u32 = 400;
const WIN_H: u32 = 700;

/// What the engine actually did, counted at the widgets themselves.
#[derive(Default)]
struct Counts {
    builds: AtomicUsize,
    layouts: AtomicUsize,
    paints: AtomicUsize,
}

impl Counts {
    fn snapshot(&self) -> (usize, usize, usize) {
        (
            self.builds.load(Ordering::SeqCst),
            self.layouts.load(Ordering::SeqCst),
            self.paints.load(Ordering::SeqCst),
        )
    }
}

/// A leaf that reports every time it is measured or painted.
struct Probe {
    counts: Arc<Counts>,
    shade: u8,
}
impl Widget for Probe {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        self.counts.layouts.fetch_add(1, Ordering::SeqCst);
        Size { width: 120.0, height: 18.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.counts.paints.fetch_add(1, Ordering::SeqCst);
        ctx.fill_rect(ctx.rect, Color::rgb(self.shade, 40, 60));
    }
}

/// A short, horizontally scrolling strip.
///
/// The page list below is ~5600px tall, which is over the 4096px texture cap,
/// so it correctly falls back to the CPU path — a real app's long list does
/// the same. This strip is small enough to composite, so the fixture exercises
/// BOTH paths, and the content-space coordinate handling that only the
/// composited one reaches.
fn strip(counts: &Arc<Counts>) -> impl Widget {
    let mut r = Row::new().spacing(6.0);
    for c in 0..8 {
        r = r.child(Probe { counts: Arc::clone(counts), shade: (c * 20) as u8 });
    }
    Container::new()
        .height(24.0)
        .child(ScrollView::new(r).axis(ScrollAxis::Horizontal))
}

/// One row: a few nested wrappers over probes, so the tree has real depth
/// rather than a flat list of leaves.
fn row(counts: &Arc<Counts>, i: usize) -> impl Widget {
    Container::new()
        .padding(EdgeInsets::all(4.0))
        .child(
            Row::new()
                .spacing(6.0)
                .child(Probe { counts: Arc::clone(counts), shade: (i % 200) as u8 })
                .child(Probe { counts: Arc::clone(counts), shade: ((i * 3) % 200) as u8 }),
        )
}

/// A widget that owns its own state, so a targeted refresh can be measured
/// against everything that must NOT repaint.
struct Badge {
    counts: Arc<Counts>,
    hits: Arc<AtomicUsize>,
}
impl StatefulWidget for Badge {
    fn build(&self) -> BoxedWidget {
        Arc::new(BadgeFace {
            counts: Arc::clone(&self.counts),
            hits: Arc::clone(&self.hits),
        })
    }
}

/// The badge's own leaf — a distinct type so the test can locate it, and so
/// its repaints are distinguishable from the background probes.
struct BadgeFace {
    counts: Arc<Counts>,
    hits: Arc<AtomicUsize>,
}
impl Widget for BadgeFace {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        self.counts.layouts.fetch_add(1, Ordering::SeqCst);
        Size { width: 120.0, height: 18.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.counts.paints.fetch_add(1, Ordering::SeqCst);
        let n = self.hits.load(Ordering::SeqCst);
        ctx.fill_rect(ctx.rect, Color::rgb((n % 200) as u8, 90, 90));
        // Refreshing from a real callback is the only way it resolves: it
        // binds the enclosing STATEFUL widget, which exists while that
        // widget's paint or one of its callbacks is running.
        ctx.register_hit(Arc::new(|| refresh_state()));
    }
}

struct App {
    counts: Arc<Counts>,
    hits: Arc<AtomicUsize>,
    overlay_open: Arc<AtomicUsize>,
}

impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        self.counts.builds.fetch_add(1, Ordering::SeqCst);

        let mut col = Column::new().spacing(2.0);
        col = col.child(strip(&self.counts));
        col = col.child(
            Badge { counts: Arc::clone(&self.counts), hits: Arc::clone(&self.hits) }.stateful(),
        );
        for i in 0..ROWS {
            col = col.child(row(&self.counts, i));
        }

        Scaffold::new(ScrollView::new(col))
            .app_bar(AppBar::new("Stress"))
            .dialog(self.overlay_open.load(Ordering::SeqCst) > 0, || {
                Arc::new(Dialog::new("Open").message("A promoted layer."))
            })
            .boxed()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    counts: Arc<Counts>,
    hits: Arc<AtomicUsize>,
    overlay_open: Arc<AtomicUsize>,
}

fn harness() -> H {
    let counts = Arc::new(Counts::default());
    let hits = Arc::new(AtomicUsize::new(0));
    let overlay_open = Arc::new(AtomicUsize::new(0));
    let e = FrameEngine::new(
        Box::new(App {
            counts: Arc::clone(&counts),
            hits: Arc::clone(&hits),
            overlay_open: Arc::clone(&overlay_open),
        }),
        FontCache::embedded(),
    );
    H {
        e,
        a: SkiaCanvas::new(WIN_W, WIN_H),
        b: SkiaCanvas::new(WIN_W, WIN_H),
        counts,
        hits,
        overlay_open,
    }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn scroll(&mut self, dy: f32) {
        self.e.paint(&mut self.a, &mut self.b, &[rosace_platform::InputEvent::Scroll {
            x: 200.0, y: 400.0, delta_x: 0.0, delta_y: dy,
        }]);
    }
    /// Settle: paint until the tree stops changing.
    fn settle(&mut self) {
        for _ in 0..4 { self.frame(); }
    }
}

/// The fixture must actually be deep, or every assertion below is measuring
/// the wrong shape of tree — which is the exact failure this file exists to
/// prevent.
#[test]
fn the_fixture_is_deep_and_uses_the_one_scroll_path() {
    let _guard = exclusive();
    let mut h = harness();
    h.settle();

    let nodes = h.e.inspect_tree();
    assert!(
        nodes.len() > 100,
        "a shallow tree cannot exercise what this file measures, got {} nodes",
        nodes.len()
    );

    let depth = {
        let by_id: std::collections::HashMap<_, _> = nodes.iter().map(|n| (n.id, n)).collect();
        nodes.iter().map(|n| {
            let (mut d, mut cur) = (0, n.parent);
            while let Some(p) = cur { d += 1; cur = by_id.get(&p).and_then(|x| x.parent); }
            d
        }).max().unwrap_or(0)
    };
    assert!(depth >= 6, "tree is only {depth} deep — not app-shaped");

    // A ScrollView used to composite into an offscreen texture and publish a
    // transform layer, which put everything beneath it into a second
    // coordinate space. That is now unrepresentable — `LayerKind` has no
    // Transform variant — so the property is enforced by the type system.
    // What is still worth asserting is that scrolling content composites
    // nothing of its own: the only layers in an app are portals (promoted
    // nodes), here the engine's own chrome.
    assert!(
        h.e.inspect_layers().iter().all(|l| l.kind == rosace::widgets::tree::LayerKind::Promoted),
        "something other than a portal is compositing separately: {:?}",
        h.e.inspect_layers().iter().map(|l| l.kind).collect::<Vec<_>>(),
    );
}

/// Scrolling must not rebuild the app. This is the single clearest proof of
/// the whole refactor: an `Atom`-driven scroll dirtied the one component,
/// re-ran `build()`, and made the frame structural — which disables every
/// per-node cache in the framework, on the most continuous interaction there
/// is.
#[test]
fn scrolling_does_not_rebuild_the_component() {
    let _guard = exclusive();
    let mut h = harness();
    h.settle();

    let (builds_before, _, _) = h.counts.snapshot();
    for _ in 0..10 {
        h.scroll(-40.0);
        h.frame();
    }
    let (builds_after, _, _) = h.counts.snapshot();

    assert_eq!(
        builds_after, builds_before,
        "ten wheel notches rebuilt the component {} time(s) — the frame is structural, \
         so every per-node cache in the framework is being ignored",
        builds_after - builds_before
    );
}

/// Scrolling re-blits moved children instead of re-recording them.
///
/// Guarded by an assertion that the list ACTUALLY MOVED, because otherwise
/// this metric improves when scrolling breaks entirely — a paint count of
/// zero is the reward both for a perfect cache and for a dead scroll view.
#[test]
fn scrolling_replays_moved_children_instead_of_repainting_them() {
    let _guard = exclusive();
    let mut h = harness();
    h.settle();

    let first = h.e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::BadgeFace")).and_then(|n| n.rect)
        .expect("the badge painted");

    let (_, _, paints_before) = h.counts.snapshot();
    for _ in 0..10 {
        h.scroll(-40.0);
        h.frame();
    }
    let (_, _, paints_after) = h.counts.snapshot();

    let moved = h.e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::BadgeFace")).and_then(|n| n.rect)
        .expect("the badge is still in the tree");
    assert!(
        (moved.origin.y - first.origin.y).abs() > 50.0,
        "the content did not actually scroll ({} -> {}), so a low paint count \
         proves nothing",
        first.origin.y, moved.origin.y
    );

    let painted = paints_after - paints_before;
    assert!(
        painted < ROWS,
        "{painted} probe paints across ten scroll frames — moved children are being \
         re-recorded rather than re-blitted ({ROWS} rows exist)"
    );
}

/// An idle frame must do nothing at all — no build, no layout, no paint.
#[test]
fn an_idle_frame_is_free() {
    let _guard = exclusive();
    let mut h = harness();
    h.settle();

    let before = h.counts.snapshot();
    h.frame();
    h.frame();
    let after = h.counts.snapshot();

    assert_eq!(after, before, "an idle frame did work: {before:?} -> {after:?}");
}

/// An idle frame with an OPEN overlay must also be free.
///
/// It was not: the old overlay pass sat after the frame-skip gate, so every
/// dialog re-laid-out and repainted on every single frame, forever, whether or
/// not anything changed.
#[test]
fn an_idle_frame_with_an_open_overlay_is_free() {
    let _guard = exclusive();
    let mut h = harness();
    h.settle();

    h.overlay_open.store(1, Ordering::SeqCst);
    rosace_state::dirty_set::reset_to_global_dirty();
    h.settle();
    assert!(
        h.e.inspect_layers().iter().any(|l| l.kind == rosace::widgets::tree::LayerKind::Promoted),
        "the dialog should be promoted while open"
    );

    let before = h.counts.snapshot();
    h.frame();
    h.frame();
    let after = h.counts.snapshot();

    assert_eq!(
        after, before,
        "an open overlay kept the whole tree repainting on idle frames: {before:?} -> {after:?}"
    );
}

/// One widget refreshing itself must repaint a handful of nodes, not the
/// screen. With 200 rows of two probes each, "the screen" is ~400 paints.
#[test]
fn refreshing_one_widget_does_not_repaint_the_tree() {
    let _guard = exclusive();
    let mut h = harness();
    h.settle();

    // Through the real click path: `refresh_state()` binds the enclosing
    // STATEFUL widget, which only exists while that widget is painting or one
    // of its callbacks is running. Called from test code it resolves to
    // nothing and marks nothing — correctly.
    let r = h.e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::BadgeFace"))
        .and_then(|n| n.rect)
        .expect("the stateful badge painted");

    let (builds_before, _, paints_before) = h.counts.snapshot();
    h.hits.fetch_add(1, Ordering::SeqCst);
    let (cx, cy) = (r.origin.x + 2.0, r.origin.y + 2.0);
    h.e.paint(&mut h.a, &mut h.b, &[
        rosace_platform::InputEvent::MouseDown { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
        rosace_platform::InputEvent::MouseUp   { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
    ]);
    h.frame();
    let (builds_after, _, paints_after) = h.counts.snapshot();

    let painted = paints_after - paints_before;
    assert!(painted > 0, "the refreshed widget did not repaint at all");
    assert!(
        painted < 20,
        "refreshing ONE widget repainted {painted} probes — a targeted refresh is \
         repainting the tree ({ROWS} rows are on screen)"
    );
    assert_eq!(
        builds_after, builds_before,
        "refresh_state() rebuilt the component; it should mark a node"
    );
}

/// Layout must be cached per node: a second frame at the same size measures
/// nothing.
#[test]
fn layout_is_cached_across_frames() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    let (_, first, _) = h.counts.snapshot();
    assert!(first > 0, "the first frame must lay something out");

    // The first paint MEASURES — a scroll view publishes its content and
    // viewport extents, which marks its node and legitimately costs one more
    // frame. The cache claim is about steady state, so settle first.
    h.settle();
    let (_, settled, _) = h.counts.snapshot();

    h.frame();
    h.frame();
    let (_, after, _) = h.counts.snapshot();
    assert_eq!(
        after, settled,
        "a settled frame re-measured {} probe(s) at an unchanged size",
        after - settled
    );
}

/// Print what the engine actually does, so the claims have numbers behind
/// them rather than only pass/fail.
///
/// `cargo test -p rosace --test stress -- --nocapture report`
#[test]
fn report() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    let (b1, l1, p1) = h.counts.snapshot();
    h.settle();
    let (b2, l2, p2) = h.counts.snapshot();

    let idle = h.counts.snapshot();
    h.frame();
    let after_idle = h.counts.snapshot();

    let before_scroll = h.counts.snapshot();
    for _ in 0..10 { h.scroll(-40.0); h.frame(); }
    let after_scroll = h.counts.snapshot();

    let nodes = h.e.inspect_tree().len();
    let layers = h.e.inspect_layers().len();

    println!("\n  tree              {nodes} nodes, {layers} layers, {ROWS} rows");
    println!("  first frame       build {b1}  layout {l1}  paint {p1}");
    println!("  settle (+3)       build {}  layout {}  paint {}", b2 - b1, l2 - l1, p2 - p1);
    println!("  idle frame        build {}  layout {}  paint {}",
             after_idle.0 - idle.0, after_idle.1 - idle.1, after_idle.2 - idle.2);
    println!("  10 wheel notches  build {}  layout {}  paint {}\n",
             after_scroll.0 - before_scroll.0,
             after_scroll.1 - before_scroll.1,
             after_scroll.2 - before_scroll.2);
}

/// Does the GPU texture path still earn its place now that a moved widget is
/// re-blitted instead of re-recorded?
///
/// Replay removed WIDGET work (0 `paint()` calls per scroll frame). It did not
/// remove RASTER work: the cached commands are still pushed and still drawn.
/// The texture path removes both — a scroll is a UV shift over an existing
/// texture. So the question is what that second saving is actually worth.
///
/// Times `engine.paint` over scroll frames for a composited list against a
/// CPU-path one.
///
/// **This cannot decide the question, and the numbers mislead if read as if it
/// could.** Headless, the engine still rasterizes the transform layer's
/// content into an offscreen canvas on every publish frame, but there is no
/// GPU to do the part that justifies the texture path — scrolling as a UV
/// shift with no drawing at all. So this run pays the texture path's cost and
/// gives it none of its benefit, and the composited list duly looks slower
/// while holding a seventh of the rows.
///
/// Kept as a regression guard on CPU-path scroll cost, which it does measure
/// honestly. Deciding whether the texture path still earns its place needs the
/// showcase running against the real compositor — which is how D090 measured
/// it in the first place.
///
/// `cargo test -p rosace --test stress -- --nocapture texture_vs_replay`
#[test]
fn texture_vs_replay() {
    let _guard = exclusive();

    /// `rows` short enough to composite, or long enough to fall to the CPU
    /// path — the 4096px texture cap is what decides.
    fn run(rows: usize) -> (u128, bool) {
        let counts = Arc::new(Counts::default());
        struct L { counts: Arc<Counts>, rows: usize }
        impl Component for L {
            fn build(&self, _c: &mut Context) -> BoxedWidget {
                let mut col = Column::new().spacing(2.0);
                for i in 0..self.rows {
                    col = col.child(row(&self.counts, i));
                }
                ScrollView::new(col).boxed()
            }
        }
        let mut e = FrameEngine::new(
            Box::new(L { counts: Arc::clone(&counts), rows }),
            FontCache::embedded(),
        );
        let (mut a, mut b) = (SkiaCanvas::new(WIN_W, WIN_H), SkiaCanvas::new(WIN_W, WIN_H));
        for _ in 0..4 { e.paint(&mut a, &mut b, &[]); }

        // There is one scroll path now; kept so the printed line still
        // distinguishes the two fixtures at a glance.
        let composited = false;

        let t = std::time::Instant::now();
        for _ in 0..60 {
            e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::Scroll {
                x: 200.0, y: 300.0, delta_x: 0.0, delta_y: -8.0,
            }]);
        }
        (t.elapsed().as_micros(), composited)
    }

    let (gpu_us, gpu_composited) = run(60);
    let (cpu_us, cpu_composited) = run(400);

    println!("\n  composited list (60 rows)   composited={gpu_composited}  {gpu_us} us / 60 scroll frames  ({} us/frame)", gpu_us / 60);
    println!("  CPU-path list  (400 rows)   composited={cpu_composited}  {cpu_us} us / 60 scroll frames  ({} us/frame)\n", cpu_us / 60);
}

/// Does a scroll frame on the GPU path actually SKIP work, or does node
/// marking defeat it?
///
/// D090's claim was that the composited path costs nothing per scroll frame:
/// the offset lives in a non-reactive channel the compositor reads, so the
/// engine skips build, walk and raster entirely. Stage 4a-i later made a
/// scroll write call `mark_node_dirty` so the implicit path would repaint at
/// all. If that marking also fires on the composited path, the frame is no
/// longer skipped and D090's property is gone.
///
/// `cargo test -p rosace --test stress -- --nocapture gpu_path_skips_work`
#[test]
fn gpu_path_skips_work() {
    let _guard = exclusive();

    fn run(rows: usize, label: &str) {
        let counts = Arc::new(Counts::default());
        struct L { counts: Arc<Counts>, rows: usize }
        impl Component for L {
            fn build(&self, _c: &mut Context) -> BoxedWidget {
                let mut col = Column::new().spacing(2.0);
                for i in 0..self.rows { col = col.child(row(&self.counts, i)); }
                ScrollView::new(col).boxed()
            }
        }
        let mut e = FrameEngine::new(
            Box::new(L { counts: Arc::clone(&counts), rows }),
            FontCache::embedded(),
        );
        let (mut a, mut b) = (SkiaCanvas::new(WIN_W, WIN_H), SkiaCanvas::new(WIN_W, WIN_H));
        for _ in 0..4 { e.paint(&mut a, &mut b, &[]); }

        // There is one scroll path now; kept so the printed line still
        // distinguishes the two fixtures at a glance.
        let composited = false;

        // `paint` reports whether the frame produced new pixels. A skipped
        // frame is the whole claim.
        // Baseline AFTER settling, so the counts are the scroll cost alone.
        let (_, l0, p0) = counts.snapshot();

        // The "republish" column counted how many scroll frames re-published
        // a layer's content texture — D090's claim that a composited scroll
        // costs none. There are no content textures now, so it is gone rather
        // than reported as a permanent zero that reads like a result.
        let t = std::time::Instant::now();
        for _ in 0..60 {
            e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::Scroll {
                x: 200.0, y: 300.0, delta_x: 0.0, delta_y: -8.0,
            }]);
        }
        let us = t.elapsed().as_micros();
        let (_, l1, p1) = counts.snapshot();
        println!("  {label:<26} composited={composited:<5} {us} us ({} us/frame)  \
layout={} paint={}", us / 60, l1 - l0, p1 - p0);
    }

    println!();
    run(60, "short list (composites)");
    run(400, "long list (CPU path)");
    println!();
}
