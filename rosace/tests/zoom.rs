//! `InteractiveViewer` zooms by replaying its child MORPHED, not by
//! rasterizing it into a texture.
//!
//! The texture approach rendered the child into an offscreen canvas at
//! `dpi_scale * zoom` and composited that, which forced a 4096px cap on
//! content size, could never work for a virtualized child, and put everything
//! beneath the viewer into a second coordinate space permanently — the source
//! of the nested-clip and screen-vs-content defects.
//!
//! Flutter applies the transform when the commands are drawn instead, so
//! vectors and glyphs come out at the zoomed size. `DrawCommand::morph` scales
//! `px` on text, so the same is true here.
//!
//! The thing that can silently break is the half that is not pixels: a
//! widget drawn twice as large has to be CLICKABLE twice as large, in the
//! right place.

use rosace::prelude::*;
use rosace::widgets::tree::{InteractiveViewer, LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const WIN_W: u32 = 300;
const WIN_H: u32 = 300;

/// A 100x100 target at the child's origin.
struct Target(Arc<AtomicUsize>);
impl Widget for Target {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 100.0, height: 100.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(200, 60, 60));
        let hits = Arc::clone(&self.0);
        ctx.register_hit(Arc::new(move || { hits.fetch_add(1, Ordering::SeqCst); }));
    }
}

struct App(Arc<AtomicUsize>);
impl Component for App {
    fn build(&self, _c: &mut Context) -> BoxedWidget {
        InteractiveViewer::new(Target(Arc::clone(&self.0))).boxed()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    hits: Arc<AtomicUsize>,
}

fn harness() -> H {
    let hits = Arc::new(AtomicUsize::new(0));
    let e = FrameEngine::new(Box::new(App(Arc::clone(&hits))), FontCache::embedded());
    H { e, a: SkiaCanvas::new(WIN_W, WIN_H), b: SkiaCanvas::new(WIN_W, WIN_H), hits }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn click(&mut self, x: f32, y: f32) {
        self.e.paint(&mut self.a, &mut self.b, &[
            rosace_platform::InputEvent::MouseDown { x, y, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x, y, button: rosace_platform::MouseButton::Left },
        ]);
    }
    fn pinch(&mut self, delta: f32) {
        self.e.paint(&mut self.a, &mut self.b, &[
            rosace_platform::InputEvent::Pinch { x: 150.0, y: 150.0, delta },
        ]);
    }
}

/// The viewer must no longer be a compositing layer at all. That is the whole
/// point: no texture, no cap, and nothing beneath it living in content space.
#[test]
fn the_viewer_is_not_a_compositing_layer() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    h.frame();

    // A transform layer is unrepresentable now — `LayerKind` has only
    // `Promoted` — so this asserts the surviving form of the property: the
    // viewer composites nothing of its own, and the only layers present are
    // portals (the engine's chrome).
    assert!(
        h.e.inspect_layers().iter().all(|l| l.kind == rosace::widgets::tree::LayerKind::Promoted),
        "InteractiveViewer is compositing separately — it should replay its \
         child morphed instead: {:?}",
        h.e.inspect_layers().iter().map(|l| l.kind).collect::<Vec<_>>(),
    );
}

/// At 1x the target occupies its natural rect and is clickable there.
#[test]
fn unzoomed_clicks_land() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    h.frame();

    h.click(50.0, 50.0);
    assert_eq!(h.hits.load(Ordering::SeqCst), 1, "a click inside the target must land");

    h.click(250.0, 250.0);
    assert_eq!(h.hits.load(Ordering::SeqCst), 1, "a click outside it must not");
}

/// The half that breaks silently: zoomed IN, the target is drawn twice as
/// large, so a point that was outside it must now be inside.
///
/// If the declarations are not morphed alongside the pixels, this is a widget
/// you can see and cannot click — and nothing about the rendering looks wrong.
#[test]
fn zoomed_in_the_target_is_clickable_at_its_new_size() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    h.frame();

    // (140, 140) is outside the natural 100x100 target.
    h.click(140.0, 140.0);
    assert_eq!(h.hits.load(Ordering::SeqCst), 0, "not inside the target at 1x");

    // Zoom in. The target grows from its top-left, so (140,140) falls inside.
    for _ in 0..6 { h.pinch(1.15); h.frame(); }
    h.frame();

    let before = h.hits.load(Ordering::SeqCst);
    h.click(140.0, 140.0);
    assert_eq!(
        h.hits.load(Ordering::SeqCst),
        before + 1,
        "zoomed in, the target covers this point visually but was not clickable — \
         its declared regions did not follow the morph"
    );
}
