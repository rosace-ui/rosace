//! `.on_size(..)` fires when a widget's size changes, and not otherwise.
//!
//! The gating is the feature, not an optimisation of it. A version that fired
//! every paint would push the cost of remembering the last value onto every
//! caller, and any caller that wrote app state from the handler would mark
//! something dirty every frame — an app that never reaches an idle frame,
//! which is invisible until someone profiles it.
//!
//! So these assert CALL COUNTS across frames, not just that the value is
//! right. A callback that fires with the correct size every single frame
//! would satisfy any assertion about its argument.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx, SizeApi};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A box whose height is read from a cell the test controls.
struct Box_(Arc<AtomicU32>);
impl Widget for Box_ {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size {
            width: c.constraints.max_width_f32().min(100.0),
            height: self.0.load(Ordering::SeqCst) as f32,
        }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(80, 80, 200));
    }
}

struct App {
    height: Arc<AtomicU32>,
    seen: Arc<Mutex<Vec<Size>>>,
}
impl Component for App {
    fn build(&self, _c: &mut Context) -> BoxedWidget {
        let seen = Arc::clone(&self.seen);
        Column::new()
            .child(
                Box_(Arc::clone(&self.height))
                    .on_size(move |s| seen.lock().unwrap().push(s)),
            )
            .boxed()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    height: Arc<AtomicU32>,
    seen: Arc<Mutex<Vec<Size>>>,
}

fn harness() -> H {
    let height = Arc::new(AtomicU32::new(40));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let e = FrameEngine::new(
        Box::new(App { height: Arc::clone(&height), seen: Arc::clone(&seen) }),
        FontCache::embedded(),
    );
    H { e, a: SkiaCanvas::new(200, 300), b: SkiaCanvas::new(200, 300), height, seen }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn calls(&self) -> usize { self.seen.lock().unwrap().len() }
    fn last(&self) -> Option<Size> { self.seen.lock().unwrap().last().copied() }
    /// Change the height and force the rebuild that a real state change would.
    fn resize(&mut self, h: u32) {
        self.height.store(h, Ordering::SeqCst);
        rosace::state::dirty_set::reset_to_global_dirty();
        self.frame();
    }
}

#[test]
fn it_reports_the_size_on_the_first_paint() {
    let _g = exclusive();
    let mut h = harness();
    h.frame();
    assert_eq!(h.calls(), 1, "the first paint must report a size");
    assert_eq!(h.last().map(|s| s.height), Some(40.0));
}

/// The frames that matter are ones where the widget really REPAINTS at an
/// unchanged size — an animating or scrolling page. Idle frames prove nothing
/// here: the node replays its cached picture, `paint` never runs, and an
/// ungated version stays silent too. This test passed against an ungated
/// implementation until it was made to force repaints.
#[test]
fn it_stays_quiet_while_the_size_does_not_change() {
    let _g = exclusive();
    let mut h = harness();
    h.frame();
    let after_first = h.calls();

    for _ in 0..30 {
        rosace::state::dirty_set::reset_to_global_dirty();
        h.frame();
    }

    assert_eq!(
        h.calls(), after_first,
        "on_size fired {} extra time(s) across 30 REPAINTS at an unchanged \
         size. A handler that writes app state would then dirty something \
         every frame and the app would never reach an idle frame.",
        h.calls() - after_first
    );
}

#[test]
fn it_fires_again_when_the_size_actually_changes() {
    let _g = exclusive();
    let mut h = harness();
    h.frame();
    for _ in 0..5 { h.frame(); }
    let before = h.calls();

    h.resize(90);
    for _ in 0..5 { h.frame(); }

    assert_eq!(h.calls(), before + 1, "exactly one report for one size change");
    assert_eq!(h.last().map(|s| s.height), Some(90.0), "and it carries the new size");

    // ...and goes quiet again at the new size.
    let after = h.calls();
    for _ in 0..20 { h.frame(); }
    assert_eq!(h.calls(), after, "it must settle again after reporting");
}

#[test]
fn two_readers_in_a_list_do_not_share_a_memory() {
    let _g = exclusive();

    struct Two {
        left: Arc<AtomicU32>,
        right: Arc<AtomicU32>,
        seen: Arc<Mutex<Vec<(u8, Size)>>>,
    }
    impl Component for Two {
        fn build(&self, _c: &mut Context) -> BoxedWidget {
            let (s1, s2) = (Arc::clone(&self.seen), Arc::clone(&self.seen));
            Column::new()
                .child(Box_(Arc::clone(&self.left)).on_size(move |s| s1.lock().unwrap().push((0, s))))
                .child(Box_(Arc::clone(&self.right)).on_size(move |s| s2.lock().unwrap().push((1, s))))
                .boxed()
        }
    }

    let left = Arc::new(AtomicU32::new(30));
    let right = Arc::new(AtomicU32::new(70));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut e = FrameEngine::new(
        Box::new(Two { left: Arc::clone(&left), right: Arc::clone(&right), seen: Arc::clone(&seen) }),
        FontCache::embedded(),
    );
    let (mut a, mut b) = (SkiaCanvas::new(200, 300), SkiaCanvas::new(200, 300));
    e.paint(&mut a, &mut b, &[]);
    for _ in 0..5 { e.paint(&mut a, &mut b, &[]); }

    let first: Vec<(u8, f32)> =
        seen.lock().unwrap().iter().map(|(i, s)| (*i, s.height)).collect();
    assert_eq!(first, vec![(0, 30.0), (1, 70.0)], "each reports its own size, once");

    // Change ONLY the left one.
    left.store(31, Ordering::SeqCst);
    rosace::state::dirty_set::reset_to_global_dirty();
    e.paint(&mut a, &mut b, &[]);
    for _ in 0..5 { e.paint(&mut a, &mut b, &[]); }

    let after: Vec<(u8, f32)> =
        seen.lock().unwrap().iter().map(|(i, s)| (*i, s.height)).collect();
    assert_eq!(
        after,
        vec![(0, 30.0), (1, 70.0), (0, 31.0)],
        "only the widget that changed should report — a shared memory would \
         make the untouched sibling fire too, or suppress the one that changed"
    );
}
