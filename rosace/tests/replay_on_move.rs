//! A widget that only MOVED should be re-blitted, not re-recorded.
//!
//! `paint_child` refuses to replay a cached picture when the rect changed:
//!
//! ```ignore
//! && n.cached_rect == Some(rect)   // moved → re-record
//! ```
//!
//! So anything above a widget growing forces every widget below it to re-run
//! `paint`, even though their content is identical and only their position
//! moved. `PaintCtx::replay_offset` already exists and already translates every
//! command — `RepaintBoundary` uses it.
//!
//! # The catch, which is the whole reason this is not a one-liner
//!
//! Hit regions, scroll viewports and `cached_rect` are all WORLD-SPACE and are
//! declared DURING paint. Re-blitting the commands without re-running paint
//! moves the pixels and leaves the clickable region behind — a widget that
//! looks moved and responds at its old position. So the second test here is not
//! a nicety; it is the thing that makes the optimisation legal.

use rosace::prelude::*;
use rosace::widgets::tree::{refresh_state, LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Grows when refreshed, pushing everything below it down.
struct Grower(Arc<AtomicUsize>);
impl Widget for Grower {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        Size { width: 80.0, height: self.0.load(Ordering::SeqCst) as f32 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(9, 1, 1));
        ctx.register_hit(Arc::new(|| refresh_state()));
    }
}

/// Sits below the grower. Its content never changes — only its position.
struct Rider {
    paints: Arc<AtomicUsize>,
    clicks: Arc<AtomicUsize>,
}
impl Widget for Rider {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 80.0, height: 30.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.paints.fetch_add(1, Ordering::SeqCst);
        ctx.fill_rect(ctx.rect, Color::rgb(1, 9, 1));
        let c = Arc::clone(&self.clicks);
        ctx.register_hit(Arc::new(move || { c.fetch_add(1, Ordering::SeqCst); }));
    }
}

struct App {
    height: Arc<AtomicUsize>,
    paints: Arc<AtomicUsize>,
    clicks: Arc<AtomicUsize>,
}

impl Component for App {
    fn build(&self, _ctx: &mut Context) -> Element {
        Column::new()
            .child(Grower(self.height.clone()))
            .child(Rider { paints: self.paints.clone(), clicks: self.clicks.clone() })
            .into_element()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    height: Arc<AtomicUsize>,
    paints: Arc<AtomicUsize>,
    clicks: Arc<AtomicUsize>,
}

fn harness() -> H {
    let height = Arc::new(AtomicUsize::new(20));
    let paints = Arc::new(AtomicUsize::new(0));
    let clicks = Arc::new(AtomicUsize::new(0));
    let e = FrameEngine::new(
        Box::new(App { height: height.clone(), paints: paints.clone(), clicks: clicks.clone() }),
        FontCache::embedded(),
    );
    H { e, a: SkiaCanvas::new(200, 400), b: SkiaCanvas::new(200, 400), height, paints, clicks }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn click(&mut self, x: f32, y: f32) {
        self.e.paint(&mut self.a, &mut self.b, &[
            rosace_platform::InputEvent::MouseDown { x, y, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x, y, button: rosace_platform::MouseButton::Left },
        ]);
    }
    fn rect_of(&self, tag: &str) -> rosace_core::types::Rect {
        self.e.inspect_tree().iter()
            .find(|n| n.tag.ends_with(tag)).and_then(|n| n.rect)
            .unwrap_or_else(|| panic!("{tag} not painted"))
    }
    /// Grow the grower by clicking it, then settle.
    fn grow_to(&mut self, h: usize) {
        let r = self.rect_of("::Grower");
        self.height.store(h, Ordering::SeqCst);
        self.click(r.origin.x + 1.0, r.origin.y + 1.0);
        self.frame();
    }
}

/// The rider's content did not change — only where it sits. It should not have
/// re-run `paint`.
#[test]
fn a_widget_that_only_moved_is_not_repainted() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();

    let before_y = h.rect_of("::Rider").origin.y;
    let paints_before = h.paints.load(Ordering::SeqCst);

    h.grow_to(120);

    let after_y = h.rect_of("::Rider").origin.y;
    assert!(after_y > before_y, "the rider should have been pushed down; {before_y} -> {after_y}");
    assert_eq!(h.paints.load(Ordering::SeqCst), paints_before,
        "the rider re-ran paint although only its position changed — its recorded \
         commands are identical and could have been re-blitted");
}

/// And the half that makes it legal: the hit region must move WITH the pixels.
///
/// Re-blitting without re-running paint leaves world-space declarations behind,
/// giving a widget that looks moved and responds at its old position.
#[test]
fn a_moved_widget_is_clickable_at_its_new_position() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();

    h.grow_to(120);

    let r = h.rect_of("::Rider");
    let before = h.clicks.load(Ordering::SeqCst);
    h.click(r.origin.x + 4.0, r.origin.y + 4.0);

    assert_eq!(h.clicks.load(Ordering::SeqCst), before + 1,
        "clicking the rider where it now IS did not hit it — the pixels moved and \
         the hit region stayed behind");
}

/// The inverse, so the optimisation cannot be "achieved" by never moving
/// anything: a widget whose CONTENT changed must still re-record.
#[test]
fn a_widget_whose_content_changed_still_repaints() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    h.frame();

    let paints_before = h.paints.load(Ordering::SeqCst);
    rosace_state::dirty_set::reset_to_global_dirty();
    h.frame();

    assert!(h.paints.load(Ordering::SeqCst) > paints_before,
        "a rebuild must re-record: every widget object is new and none of them \
         can be compared against what was cached");
}
