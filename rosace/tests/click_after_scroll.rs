//! Clicking must still work after scrolling.
//!
//! Scrolling moves every visible widget at once, which is the case replay-on-move
//! was built for — and the case where a mistake in translating world-space hit
//! regions shows up as "the app stopped responding to clicks".

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

struct Tap(usize, Arc<Mutex<Vec<usize>>>);
impl Widget for Tap {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 120.0, height: 40.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(20, 60, 20));
        let (i, log) = (self.0, Arc::clone(&self.1));
        ctx.register_hit(Arc::new(move || log.lock().unwrap().push(i)));
    }
}

struct App(Arc<Mutex<Vec<usize>>>);
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> Element {
        let mut col = Column::new();
        for i in 0..12 {
            col = col.child(Tap(i, self.0.clone()));
        }
        ScrollView::new(col).into_element()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    log: Arc<Mutex<Vec<usize>>>,
}

fn harness() -> H {
    let log = Arc::new(Mutex::new(Vec::new()));
    let e = FrameEngine::new(Box::new(App(log.clone())), FontCache::embedded());
    H { e, a: SkiaCanvas::new(200, 200), b: SkiaCanvas::new(200, 200), log }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn click(&mut self, x: f32, y: f32) {
        self.e.paint(&mut self.a, &mut self.b, &[
            rosace_platform::InputEvent::MouseDown { x, y, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x, y, button: rosace_platform::MouseButton::Left },
        ]);
    }
    /// Where a given Tap currently is, per the painted tree.
    fn nth_tap_rect(&self, n: usize) -> Option<rosace_core::types::Rect> {
        self.e.inspect_tree().iter()
            .filter(|x| x.tag.ends_with("::Tap"))
            .filter_map(|x| x.rect)
            .nth(n)
    }
}

/// Baseline: clicking works before any scrolling.
#[test]
fn clicks_work_before_scrolling() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    let r = h.nth_tap_rect(0).expect("first tap painted");
    h.click(r.origin.x + 5.0, r.origin.y + 5.0);
    assert!(!h.log.lock().unwrap().is_empty(), "a click before scrolling did nothing");
}

/// The regression: after a scroll, the widget now under the pointer must be the
/// one that fires.
#[test]
fn clicks_work_after_scrolling() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();

    for _ in 0..3 {
        h.e.paint(&mut h.a, &mut h.b, &[rosace_platform::InputEvent::Scroll {
            x: 100.0, y: 100.0, delta_x: 0.0, delta_y: -50.0,
        }]);
        h.frame();
    }

    // Pick a widget by where it actually IS now, and click there.
    let r = h.e.inspect_tree().iter()
        .filter(|x| x.tag.ends_with("::Tap"))
        .filter_map(|x| x.rect)
        .find(|r| r.origin.y > 20.0 && r.origin.y < 150.0)
        .expect("some tap is on screen after scrolling");

    h.log.lock().unwrap().clear();
    h.click(r.origin.x + 5.0, r.origin.y + 5.0);

    assert!(!h.log.lock().unwrap().is_empty(),
        "clicking a widget where it now IS did nothing — its hit region did not \
         move with its pixels");
}
