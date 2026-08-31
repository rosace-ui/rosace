//! Rows scrolled into view must be clickable, and rows scrolled out must not
//! keep their hit regions.
//!
//! `register_hit` used to intersect the rect it declared with the clip in
//! force at declaration time, and to declare NOTHING for a widget entirely
//! outside it. Replay-on-move re-blits a cached picture and translates what
//! the subtree declared — it never re-declares. Those two together lose
//! information a translation cannot restore: a row below the viewport
//! declared nothing, so scrolling it into view translated that nothing and
//! the row rendered perfectly and could not be clicked, while the rows that
//! had been on screen kept regions that rode off the top.
//!
//! The list ended up clickable only where it stood at the last FULL repaint,
//! which is why toggling the theme appeared to fix it — a theme change forces
//! a structural frame and re-records everything.
//!
//! The clip is applied by the pointer walk now (`child_coords` prunes any
//! subtree whose clipping ancestor does not contain the point), so
//! declarations are full and survive translation exactly.
//!
//! Two things this test does that the older click-after-scroll tests did not,
//! each of which independently hides the bug:
//!
//!   * it attaches an explicit `ScrollController`. Without one this ScrollView
//!     composites, children stay in content space, and nothing translates.
//!   * it reads the offset off the controller. Row rects are content-space
//!     under a composited ScrollView and never move, so measuring the scroll
//!     from them reads zero forever and every assertion passes vacuously.

use rosace::prelude::*;
use rosace::widgets::scroll::ScrollController;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_platform::{InputEvent, MouseButton};
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Arc, Mutex};

const W: u32 = 420;
const H: u32 = 820;
const ROW_H: f32 = 51.0;
const ROWS: usize = 54;

struct Cell(usize, Arc<Mutex<Vec<usize>>>);
impl Widget for Cell {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: ROW_H }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(((self.0 * 7) % 200) as u8, 60, 60));
        let (i, log) = (self.0, Arc::clone(&self.1));
        ctx.register_hit(Arc::new(move || log.lock().unwrap().push(i)));
    }
}

struct App(Arc<Mutex<Vec<usize>>>, ScrollController);
impl Component for App {
    fn build(&self, _c: &mut Context) -> BoxedWidget {
        let mut col = Column::new();
        for i in 0..ROWS { col = col.child(Cell(i, Arc::clone(&self.0))); }
        ScrollView::new(col).controller(self.1.clone()).boxed()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    log: Arc<Mutex<Vec<usize>>>,
    ctrl: ScrollController,
}

impl H {
    fn new() -> Self {
        let log = Arc::new(Mutex::new(Vec::new()));
        let ctrl = ScrollController::new();
        let e = FrameEngine::new(Box::new(App(Arc::clone(&log), ctrl.clone())), FontCache::embedded());
        let mut h = H { e, a: SkiaCanvas::new(W, H), b: SkiaCanvas::new(W, H), log, ctrl };
        for _ in 0..4 { h.frame(); }
        h
    }
    fn ev(&mut self, v: &[InputEvent]) { self.e.paint(&mut self.a, &mut self.b, v); }
    fn frame(&mut self) { self.ev(&[]); }

    fn scroll(&mut self, dy: f32, times: usize) {
        for _ in 0..times {
            self.ev(&[InputEvent::Scroll { x: 200.0, y: 400.0, delta_x: 0.0, delta_y: dy }]);
            self.frame();
        }
        for _ in 0..60 { self.frame(); }
    }
    /// A real click: held down across frames, the way a pointer actually
    /// behaves. A MouseDown and MouseUp in one batch never lets `pressed()`
    /// become observable, and misses a whole class of defect.
    fn click(&mut self, y: f32) -> Vec<usize> {
        self.log.lock().unwrap().clear();
        self.ev(&[InputEvent::MouseDown { x: 200.0, y, button: MouseButton::Left }]);
        for _ in 0..4 { self.frame(); }
        self.ev(&[InputEvent::MouseUp { x: 200.0, y, button: MouseButton::Left }]);
        for _ in 0..4 { self.frame(); }
        let r = self.log.lock().unwrap().clone();
        r
    }
}

/// The reported bug: scroll, then click. Nothing fired, and only a full
/// repaint brought it back.
#[test]
fn a_row_scrolled_into_view_is_clickable() {
    let mut h = H::new();
    let before = h.click(400.0);
    assert!(!before.is_empty(), "control: clicking before any scroll fired nothing");

    h.scroll(-50.0, 10);
    let offset = h.ctrl.offset()[1];
    assert!(offset > 100.0, "the wheel scroll did not move the list (offset {offset})");

    let after = h.click(400.0);
    assert!(
        !after.is_empty(),
        "after scrolling {offset}px, clicking a row that is plainly on screen fired \
         nothing. Its hit region was never declared — it was off screen when the \
         subtree was last recorded, and replay translated that absence."
    );
    assert_ne!(before, after, "the click hit the same row as before scrolling");
}

/// The other half: a row scrolled OUT of view must stop being clickable, or
/// translated regions pile up over the content above.
#[test]
fn a_row_scrolled_out_of_view_is_not_clickable() {
    let mut h = H::new();
    let top = h.click(20.0);
    assert_eq!(top, vec![0], "control: the first row should be at the top");

    h.scroll(-50.0, 10);

    // Row 0 is now far above the viewport. Clicking where it used to be must
    // hit whatever is there NOW, never row 0.
    let after = h.click(20.0);
    assert!(
        !after.contains(&0),
        "row 0 is scrolled out of sight but still answered a click at the top of \
         the viewport: {after:?}"
    );
}

/// Scrolling back must restore the original rows, not leave a dead band.
#[test]
fn scrolling_back_restores_the_original_rows() {
    let mut h = H::new();
    let first = h.click(400.0);

    h.scroll(-50.0, 10);
    h.scroll(50.0, 10);
    assert!(h.ctrl.offset()[1] < 1.0, "the list did not return to the top");

    assert_eq!(
        h.click(400.0), first,
        "after scrolling away and back, the same point hits a different row"
    );
}
