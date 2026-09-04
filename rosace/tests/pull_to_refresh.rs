//! `PullToRefresh` fires once when you drag past the trigger and release.
//!
//! The showcase's demo only ever set its `refreshing` flag to FALSE, so the
//! spinner could never appear and the widget looked broken when the demo was.
//! That also meant nothing exercised the release path end to end — and doing
//! so found that it does not work at all over a bounce-physics `ScrollView`,
//! which is what every real use wraps.
//!
//! Three defects had to be fixed before a pull could fire at all, and the
//! last one is the interesting one:
//!
//! * NESTED-SCROLL PRECEDENCE — the chain tried the innermost scrollable
//!   first, and a Bounce ScrollView always consumes a downward drag at its top
//!   by stretching, so the pull never reached the wrapper. The chain offers a
//!   delta hard-clamped first, then allows overscroll outermost-first.
//! * RESISTANCE — the pull went through Bounce's rubber-band resistance,
//!   which scales with how far out you already are, so it asymptoted: a 220px
//!   drag gave 20px of pull against a 70px trigger.
//! * RELEASE — the widget inferred "finger lifted" from `ctx.pressed()`,
//!   which reports the ONE node under the pointer. A wrapper is never that
//!   node, so it read false for the entire gesture. The engine builds the
//!   scroll chain on press and clears it on release, so it knows exactly when
//!   the gesture ends; `on_scroll_gesture_end` says so instead of making each
//!   widget guess.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_platform::{InputEvent, MouseButton};
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Row;
impl Widget for Row {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: 40.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(70, 70, 90)); }
}

struct App(Arc<AtomicUsize>);
impl Component for App {
    fn build(&self, _c: &mut Context) -> BoxedWidget {
        let mut col = Column::new();
        for _ in 0..12 { col = col.child(Row); }
        let fired = Arc::clone(&self.0);
        PullToRefresh::new(ScrollView::new(col))
            .on_refresh(move || { fired.fetch_add(1, Ordering::SeqCst); })
            .boxed()
    }
}

struct H { e: FrameEngine, a: SkiaCanvas, b: SkiaCanvas, fired: Arc<AtomicUsize> }
fn harness() -> H {
    let fired = Arc::new(AtomicUsize::new(0));
    let e = FrameEngine::new(Box::new(App(Arc::clone(&fired))), FontCache::embedded());
    H { e, a: SkiaCanvas::new(300, 500), b: SkiaCanvas::new(300, 500), fired }
}
impl H {
    fn ev(&mut self, v: &[InputEvent]) { self.e.paint(&mut self.a, &mut self.b, v); }
    fn frame(&mut self) { self.ev(&[]); }
    /// Drag from `from` down to `to`, one move per frame, then release.
    fn pull(&mut self, from: f32, to: f32) {
        self.ev(&[InputEvent::MouseDown { x: 150.0, y: from, button: MouseButton::Left }]);
        let mut y = from;
        while y < to { y += 15.0; self.ev(&[InputEvent::MouseMove { x: 150.0, y }]); }
        self.ev(&[InputEvent::MouseUp { x: 150.0, y, button: MouseButton::Left }]);
        for _ in 0..30 { self.frame(); }
    }
}

#[test]
fn a_long_pull_fires_refresh_once() {
    let mut h = harness();
    for _ in 0..4 { h.frame(); }
    h.pull(40.0, 260.0);
    assert_eq!(
        h.fired.load(Ordering::SeqCst), 1,
        "one pull past the trigger distance should fire on_refresh exactly once"
    );
}

#[test]
fn a_short_pull_does_not_fire() {
    let mut h = harness();
    for _ in 0..4 { h.frame(); }
    h.pull(40.0, 70.0);
    assert_eq!(
        h.fired.load(Ordering::SeqCst), 0,
        "a pull short of the trigger distance must not refresh — otherwise \
         any small downward drag at the top of a list reloads it"
    );
}

/// While `refreshing` is set the widget must keep ASKING for frames, or the
/// spinner stops turning as soon as the app goes idle.
///
/// Asserted on the frame REQUEST, not on pixels changing. The spinner's angle
/// comes from elapsed time, and a headless test advances almost none, so
/// consecutive frames are legitimately identical — an earlier version of this
/// counted repaints, saw 0/10 and read it as the widget being broken when it
/// was the measurement that was wrong.
#[test]
fn a_refreshing_widget_keeps_requesting_frames() {
    struct Spin;
    impl Component for Spin {
        fn build(&self, _c: &mut Context) -> BoxedWidget {
            PullToRefresh::new(ScrollView::new(Column::new().child(Row)))
                .refreshing(true)
                .on_refresh(|| {})
                .boxed()
        }
    }
    let mut e = FrameEngine::new(Box::new(Spin), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(300, 300), SkiaCanvas::new(300, 300));
    for _ in 0..3 { e.paint(&mut a, &mut b, &[]); }

    let mut asked = 0;
    for _ in 0..10 {
        let _ = rosace::state::take_frame_requested();
        e.paint(&mut a, &mut b, &[]);
        if rosace::state::take_frame_requested() { asked += 1; }
    }
    assert!(
        asked >= 8,
        "only {asked}/10 refreshing frames asked for another — the spinner \
         needs the app kept awake or it freezes the moment nothing else \
         happens"
    );
}
