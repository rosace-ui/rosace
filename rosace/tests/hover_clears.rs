//! Hover must CLEAR when the pointer leaves, not just set when it arrives.
//!
//! `set_hover` marks the node entered and the node left, and the engine only
//! requests a frame when the target actually changed. The exit half is the one
//! that goes unnoticed: a widget that never repaints after the pointer leaves
//! keeps its hover treatment on screen forever.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Records the hover state it last PAINTED with — what is actually on screen,
/// rather than what the tree believes.
struct Probe(Arc<AtomicBool>);
impl Widget for Probe {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 100.0, height: 40.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        let h = ctx.hovered();
        self.0.store(h, Ordering::SeqCst);
        ctx.fill_rect(ctx.rect, if h { Color::rgb(200, 0, 0) } else { Color::rgb(20, 20, 20) });
        ctx.register_hit(Arc::new(|| {}));
    }
}

struct App(Arc<AtomicBool>);
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        Column::new()
            .child(Probe(self.0.clone()))
            .child(Spacer::new(200.0))
            .boxed()
    }
}

/// The same probe, but inside a SCROLLABLE — which is where the showcase's
/// list lives. A composited `ScrollView` declares its children in content
/// space, so hover testing has to remap on the way down. This is the case the
/// plain harness above does not build.
struct ScrollApp(Arc<AtomicBool>);
impl Component for ScrollApp {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let mut col = Column::new().child(Probe(self.0.clone()));
        for _ in 0..12 {
            col = col.child(Spacer::new(40.0));
        }
        ScrollView::new(col).boxed()
    }
}

#[test]
fn hover_clears_inside_a_scrollable() {
    let _guard = exclusive();
    let painted = Arc::new(AtomicBool::new(false));
    let mut e = FrameEngine::new(Box::new(ScrollApp(painted.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(200, 200), SkiaCanvas::new(200, 200));

    e.paint(&mut a, &mut b, &[]);
    let r = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Probe")).and_then(|n| n.rect)
        .expect("probe painted");

    let (cx, cy) = (r.origin.x + 5.0, r.origin.y + 5.0);
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove { x: cx, y: cy }]);
    e.paint(&mut a, &mut b, &[]);
    assert!(painted.load(Ordering::SeqCst),
        "inside a scrollable, the probe never painted as hovered");

    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove {
        x: cx, y: r.origin.y + r.size.height + 60.0 }]);
    e.paint(&mut a, &mut b, &[]);

    assert!(!painted.load(Ordering::SeqCst),
        "inside a scrollable, the hover treatment is stuck on after the pointer left");
    // NOT "nothing is hovered": the pointer is still inside the ScrollView, so
    // the ScrollView itself being hovered is correct. Only the probe must clear.
    let probe_hovered = e.inspect_tree().iter()
        .any(|n| n.tag.ends_with("::Probe") && n.hovered);
    assert!(!probe_hovered, "the probe is still marked hovered inside a scrollable");
}

/// Hover effects are ANIMATED — `animate_to` eases toward the target and asks
/// for the next frame from inside `paint`. If that chain stops being driven,
/// the widget freezes at whatever it last drew, which looks exactly like hover
/// being stuck on.
struct Fading(Arc<Mutex<f32>>);
impl Widget for Fading {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 100.0, height: 40.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        let t = ctx.animate_to(if ctx.hovered() { 1.0 } else { 0.0 }, 0.0);
        *self.0.lock().unwrap() = t;
        ctx.fill_rect(ctx.rect, Color::rgb((t * 255.0) as u8, 0, 0));
        ctx.register_hit(Arc::new(|| {}));
    }
}

struct FadeApp(Arc<Mutex<f32>>);
impl Component for FadeApp {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        Column::new().child(Fading(self.0.clone())).child(Spacer::new(200.0)).boxed()
    }
}

#[test]
fn an_animated_hover_eases_back_to_zero_after_leaving() {
    let _guard = exclusive();
    let level = Arc::new(Mutex::new(0.0_f32));
    let mut e = FrameEngine::new(Box::new(FadeApp(level.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(200, 400), SkiaCanvas::new(200, 400));

    e.paint(&mut a, &mut b, &[]);
    let r = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Fading")).and_then(|n| n.rect).unwrap();
    let (cx, cy) = (r.origin.x + 5.0, r.origin.y + 5.0);

    // Settle ON.
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove { x: cx, y: cy }]);
    for _ in 0..60 { e.paint(&mut a, &mut b, &[]); }
    assert!(*level.lock().unwrap() > 0.9, "hover never eased in: {}", level.lock().unwrap());

    // Leave, and let it settle OFF.
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove {
        x: cx, y: r.origin.y + r.size.height + 80.0 }]);
    for _ in 0..60 { e.paint(&mut a, &mut b, &[]); }

    assert!(*level.lock().unwrap() < 0.1,
        "the hover animation never eased back out — it is stuck at {}. The widget \
         stopped being driven after the pointer left, so it kept drawing its \
         hovered appearance.", level.lock().unwrap());
}

#[test]
fn hover_clears_when_the_pointer_leaves() {
    let _guard = exclusive();
    let painted_hovered = Arc::new(AtomicBool::new(false));
    let mut e = FrameEngine::new(Box::new(App(painted_hovered.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(200, 400), SkiaCanvas::new(200, 400));

    e.paint(&mut a, &mut b, &[]);
    let r = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Probe")).and_then(|n| n.rect)
        .expect("probe painted");

    // Move ONTO it.
    let (cx, cy) = (r.origin.x + 5.0, r.origin.y + 5.0);
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove { x: cx, y: cy }]);
    e.paint(&mut a, &mut b, &[]);
    assert!(painted_hovered.load(Ordering::SeqCst),
        "the probe never painted itself as hovered");

    // Move well AWAY, onto the spacer below it.
    let away_y = r.origin.y + r.size.height + 80.0;
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove { x: cx, y: away_y }]);
    e.paint(&mut a, &mut b, &[]);

    assert!(!painted_hovered.load(Ordering::SeqCst),
        "the probe is still painting itself as hovered after the pointer left — \
         its hover treatment is stuck on screen");
}

/// And the tree's own view must agree, so the two cannot drift.
#[test]
fn the_tree_reports_no_hovered_node_after_leaving() {
    let _guard = exclusive();
    let painted = Arc::new(AtomicBool::new(false));
    let mut e = FrameEngine::new(Box::new(App(painted.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(200, 400), SkiaCanvas::new(200, 400));

    e.paint(&mut a, &mut b, &[]);
    let r = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Probe")).and_then(|n| n.rect).unwrap();

    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove {
        x: r.origin.x + 5.0, y: r.origin.y + 5.0 }]);
    e.paint(&mut a, &mut b, &[]);
    assert!(e.inspect_tree().iter().any(|n| n.hovered), "nothing is hovered after moving on");

    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove {
        x: r.origin.x + 5.0, y: r.origin.y + r.size.height + 80.0 }]);
    e.paint(&mut a, &mut b, &[]);

    let still: Vec<&str> = e.inspect_tree().iter()
        .filter(|n| n.hovered).map(|n| n.tag).collect();
    assert!(still.is_empty(), "these nodes are still marked hovered: {still:?}");
}
