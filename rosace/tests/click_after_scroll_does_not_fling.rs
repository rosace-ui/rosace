//! Clicking a list that was scrolled by wheel must not fling it.
//!
//! `track_velocity` measures the offset delta since its own last call, and
//! `ScrollView` only calls it while pressed. Nothing else updates the
//! baseline — so a wheel scroll, a `scroll_to` or a `reveal` moves the offset
//! with the baseline left behind. The first press frame afterwards measured
//! that entire movement as if it had happened in one frame: a 500px wheel
//! scroll became ~31000 px/s, clamped to `MAX_VELOCITY`, and release handed
//! it to `coast`.
//!
//! Live, clicking a stationary list flung it to the far end — in whichever
//! direction it had last been scrolled, which is what identified the cause.
//!
//! The press has to span SEVERAL FRAMES to reproduce it. `ctx.pressed()` lags
//! its event by a frame, so a MouseDown and MouseUp delivered in one batch
//! never sees `pressed` true on a painted frame and `track_velocity` never
//! runs. Every existing click-after-scroll test does exactly that, which is
//! why the whole suite passed while the app was unusable.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_platform::{InputEvent, MouseButton};
use rosace_render::{FontCache, SkiaCanvas};
use rosace::widgets::scroll::ScrollController;
use std::sync::Arc;

const W: u32 = 400;
const H: u32 = 600;
const ROWS: usize = 60;
const ROW_H: f32 = 44.0;

struct Cell(usize);
impl Widget for Cell {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: ROW_H }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(((self.0 * 7) % 200) as u8, 60, 60));
        ctx.register_hit(Arc::new(|| {}));
    }
}

struct App(ScrollController);
impl Component for App {
    fn build(&self, _c: &mut Context) -> BoxedWidget {
        let mut col = Column::new();
        for i in 0..ROWS { col = col.child(Cell(i)); }
        ScrollView::new(col).controller(self.0.clone()).boxed()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    ctrl: ScrollController,
}

impl H {
    fn new() -> Self {
        let ctrl = ScrollController::new();
        H {
            e: FrameEngine::new(Box::new(App(ctrl.clone())), FontCache::embedded()),
            a: SkiaCanvas::new(W, H), b: SkiaCanvas::new(W, H),
            ctrl,
        }
    }
    fn ev(&mut self, v: &[InputEvent]) { self.e.paint(&mut self.a, &mut self.b, v); }
    fn frame(&mut self) { self.ev(&[]); }

    /// Read straight off the controller. Row rects are content-space under a
    /// composited ScrollView, so they do NOT move when it scrolls — measuring
    /// the offset from them reads zero forever and every assertion below
    /// passes without testing anything.
    fn offset(&self) -> f32 { self.ctrl.offset()[1] }
    fn wheel(&mut self, dy: f32, times: usize) {
        for _ in 0..times {
            self.ev(&[InputEvent::Scroll { x: 200.0, y: 300.0, delta_x: 0.0, delta_y: dy }]);
            self.frame();
        }
        for _ in 0..90 { self.frame(); }   // let any momentum die out
    }
    /// A real click: the button is held down across several frames, which is
    /// what makes `pressed()` observable to the widget.
    fn click(&mut self, x: f32, y: f32, hold: usize) {
        self.ev(&[InputEvent::MouseDown { x, y, button: MouseButton::Left }]);
        for _ in 0..hold { self.frame(); }
        self.ev(&[InputEvent::MouseUp { x, y, button: MouseButton::Left }]);
        for _ in 0..90 { self.frame(); }   // give any fling time to run
    }
}

#[test]
fn clicking_after_a_wheel_scroll_down_does_not_fling_the_list() {
    let mut h = H::new();
    for _ in 0..4 { h.frame(); }
    h.wheel(-40.0, 8);

    let settled = h.offset();
    assert!(settled > 50.0, "the wheel scroll did not move the list (offset {settled})");

    h.click(200.0, 300.0, 4);
    let after = h.offset();

    assert!(
        (after - settled).abs() < 2.0,
        "clicking a stationary list moved it from {settled} to {after}. The press \
         measured the whole preceding wheel scroll as this gesture's velocity and \
         released it into `coast`, flinging the list the way it had last been \
         scrolled."
    );
}

/// The direction half of the report: scrolled UP, the fling went UP. Asserted
/// separately because a symmetric bug is easy to half-fix.
#[test]
fn clicking_after_a_wheel_scroll_up_does_not_fling_the_list() {
    let mut h = H::new();
    for _ in 0..4 { h.frame(); }
    h.wheel(-40.0, 16);          // go down first, so there is room to come back
    h.wheel(40.0, 6);            // then back up

    let settled = h.offset();
    h.click(200.0, 300.0, 4);
    let after = h.offset();

    assert!(
        (after - settled).abs() < 2.0,
        "clicking after scrolling UP moved the list from {settled} to {after}"
    );
}

/// Pressing a list that is still coasting should stop it, not accelerate it.
#[test]
fn pressing_a_coasting_list_stops_it() {
    let mut h = H::new();
    for _ in 0..4 { h.frame(); }

    // Flick, then press only a few frames later, while momentum is live.
    for _ in 0..8 {
        h.ev(&[InputEvent::Scroll { x: 200.0, y: 300.0, delta_x: 0.0, delta_y: -40.0 }]);
        h.frame();
    }
    for _ in 0..3 { h.frame(); }

    h.ev(&[InputEvent::MouseDown { x: 200.0, y: 300.0, button: MouseButton::Left }]);
    for _ in 0..4 { h.frame(); }
    let held = h.offset();
    for _ in 0..20 { h.frame(); }

    assert!(
        (h.offset() - held).abs() < 2.0,
        "the list kept moving under a held finger: {held} -> {}. Pressing must \
         stop the momentum.",
        h.offset()
    );
}
