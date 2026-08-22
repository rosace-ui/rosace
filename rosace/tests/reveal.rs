//! Scrolling a child into view without the app doing arithmetic.
//!
//! Every other toolkit makes you measure item heights and compute an offset.
//! The framework already knows where every painted child is — that data has
//! been on the node all along, there was just no way to ask for it.
//!
//! Two mechanisms, because there are genuinely two cases: a non-virtualized
//! child has a node and a rect, so it is a lookup; a virtualized row does not
//! exist until it is on screen, so it is arithmetic from a fixed extent.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::scroll::{ScrollAlign, ScrollController};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const WIN_W: u32 = 200;
const WIN_H: u32 = 300;
const ROW_H: f32 = 40.0;
const ROWS: usize = 30;

struct Row(usize);
impl Widget for Row {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: ROW_H }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb((self.0 % 200) as u8, 40, 40));
    }
}

struct App {
    ctrl: Arc<OnceLock<ScrollController>>,
}
impl Component for App {
    fn build(&self, ctx: &mut Context) -> BoxedWidget {
        let ctrl = ctx.state(ScrollController::new()).get();
        let _ = self.ctrl.set(ctrl.clone());
        let mut col = Column::new();
        for i in 0..ROWS {
            col = col.child(Row(i));
        }
        ScrollView::controlled(col, ctrl).boxed()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    ctrl: Arc<OnceLock<ScrollController>>,
}

fn harness() -> H {
    let ctrl = Arc::new(OnceLock::new());
    let e = FrameEngine::new(Box::new(App { ctrl: ctrl.clone() }), FontCache::embedded());
    H { e, a: SkiaCanvas::new(WIN_W, WIN_H), b: SkiaCanvas::new(WIN_W, WIN_H), ctrl }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn ctrl(&self) -> ScrollController { self.ctrl.get().unwrap().clone() }
    /// The nth Row's node id, as painted.
    fn row_node(&self, n: usize) -> Option<usize> {
        self.e.inspect_tree().iter()
            .filter(|x| x.tag.ends_with("::Row"))
            .nth(n)
            .map(|x| x.id)
    }
}

/// A row below the fold is brought just into view — the minimum move.
#[test]
fn nearest_scrolls_the_minimum_distance_to_reveal_a_row() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    h.frame();

    let ctrl = h.ctrl();
    assert_eq!(ctrl.offset()[1], 0.0, "starts at the top");
    let vp_h = ctrl.viewport_size()[1];
    assert!(vp_h > 0.0, "the viewport must be measured before revealing into it");

    // A row past the bottom edge.
    let target = ((vp_h / ROW_H).ceil() as usize) + 3;
    let node = h.row_node(target).expect("the target row painted");
    assert!(h.e.reveal(node, ScrollAlign::Nearest), "a scrollable ancestor must be found");

    let want = (target as f32 + 1.0) * ROW_H - vp_h;
    assert!(
        (ctrl.offset()[1] - want).abs() < 0.5,
        "Nearest should align the row's trailing edge with the viewport's, \
         wanted {want}, got {}",
        ctrl.offset()[1]
    );
}

/// A row already fully visible must not move the list at all.
#[test]
fn nearest_does_nothing_when_the_row_is_already_visible() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    h.frame();

    let ctrl = h.ctrl();
    let node = h.row_node(1).expect("row 1 painted");
    assert!(h.e.reveal(node, ScrollAlign::Nearest));
    assert_eq!(
        ctrl.offset()[1], 0.0,
        "a row already on screen must not scroll the list"
    );
}

#[test]
fn center_puts_the_row_in_the_middle() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    h.frame();

    let ctrl = h.ctrl();
    let vp_h = ctrl.viewport_size()[1];
    let node = h.row_node(20).expect("row 20 painted");
    assert!(h.e.reveal(node, ScrollAlign::Center));

    let want = 20.0 * ROW_H + (ROW_H - vp_h) / 2.0;
    assert!(
        (ctrl.offset()[1] - want).abs() < 0.5,
        "wanted {want}, got {}", ctrl.offset()[1]
    );
}

/// Revealing must report failure rather than silently scrolling to zero when
/// there is no geometry yet — the caller can then fall back to an index.
#[test]
fn revealing_before_anything_has_painted_reports_failure() {
    let _guard = exclusive();
    let ctrl = ScrollController::new();
    assert_eq!(
        ctrl.reveal(
            Rect { origin: Point { x: 0.0, y: 400.0 }, size: Size { width: 10.0, height: 40.0 } },
            ScrollAlign::Nearest,
        ),
        None,
        "an unmeasured viewport has nothing to reveal into"
    );
    assert_eq!(ctrl.offset(), [0.0, 0.0], "and it must not have moved anything");
}

/// The virtualized case: rows off screen have no node at all, so the position
/// comes from the fixed extent instead. Exact, not approximate.
#[test]
fn a_virtualized_row_is_reached_by_index() {
    let _guard = exclusive();
    let ctrl = ScrollController::new();
    ctrl.set_viewport_size([200.0, 300.0]);
    ctrl.set_content_size([200.0, ROWS as f32 * ROW_H]);

    assert!(rosace::widgets::tree::ListView::scroll_to_index(
        &ctrl, ROWS, ROW_H, 10, ScrollAlign::Start,
    ));
    assert_eq!(ctrl.offset()[1], 10.0 * ROW_H, "Start puts the row's top at the viewport's top");

    // The last rows cannot reach the top — there is not enough content below
    // them to scroll past. Clamped to the real maximum rather than refusing,
    // which is what a user asking for the last row means.
    assert!(rosace::widgets::tree::ListView::scroll_to_index(
        &ctrl, ROWS, ROW_H, ROWS - 1, ScrollAlign::Start,
    ));
    let max = ROWS as f32 * ROW_H - 300.0;
    assert_eq!(ctrl.offset()[1], max, "clamped to the end of the content");

    assert!(
        !rosace::widgets::tree::ListView::scroll_to_index(
            &ctrl, ROWS, ROW_H, ROWS + 5, ScrollAlign::Start,
        ),
        "an out-of-range index must report failure, not clamp silently"
    );
}
