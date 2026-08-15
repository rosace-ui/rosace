//! A > B > C. C changes state and GROWS. Does A find out?
//!
//! Marking a node dirty propagates `needs_paint` to the ancestors so they
//! re-assemble their display lists, but `needs_layout` deliberately stops at
//! the node — the plan's rule is that whether anything above must re-measure
//! is decided by COMPARING the new size against the cached one, not assumed.
//!
//! This pins whether that comparison actually happens.

use rosace::prelude::*;
use rosace::widgets::tree::{refresh_state, BoxedWidget, LayoutCtx, PaintCtx, StatefulExt, StatefulWidget};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// C — grows when its state changes.
struct C(Arc<AtomicU32>);
impl Widget for C {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        Size { width: 50.0, height: self.0.load(Ordering::SeqCst) as f32 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(0, 120, 0));
        ctx.register_hit(Arc::new(|| refresh_state()));
    }
}

/// The stateful wrapper around C.
struct CHost(Arc<AtomicU32>);
impl StatefulWidget for CHost {
    fn build(&self) -> BoxedWidget { Box::new(C(self.0.clone())) }
}

struct App(Arc<AtomicU32>);
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> Element {
        // A (outer Column) > B (inner Column) > C
        Column::new()
            .child(Column::new().child(CHost(self.0.clone()).stateful()))
            .into_element()
    }
}

fn height_of(e: &FrameEngine, tag_suffix: &str) -> Option<f32> {
    e.inspect_tree().iter()
        .find(|n| n.tag.ends_with(tag_suffix))
        .and_then(|n| n.rect)
        .map(|r| r.size.height)
}

#[test]
fn a_grandchild_growing_resizes_its_ancestors() {
    let h = Arc::new(AtomicU32::new(20));
    let mut e = FrameEngine::new(Box::new(App(h.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(400, 600), SkiaCanvas::new(400, 600));

    e.paint(&mut a, &mut b, &[]);
    let c0 = height_of(&e, "::C").expect("C painted");
    assert_eq!(c0, 20.0, "starting height");

    // C's state changes: it now wants to be 3x taller.
    let rect = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::C")).and_then(|n| n.rect).unwrap();
    h.store(60, Ordering::SeqCst);
    e.paint(&mut a, &mut b, &[
        rosace_platform::InputEvent::MouseDown {
            x: rect.origin.x + 1.0, y: rect.origin.y + 1.0,
            button: rosace_platform::MouseButton::Left },
        rosace_platform::InputEvent::MouseUp {
            x: rect.origin.x + 1.0, y: rect.origin.y + 1.0,
            button: rosace_platform::MouseButton::Left },
    ]);
    e.paint(&mut a, &mut b, &[]);

    let c1 = height_of(&e, "::C").expect("C still painted");
    assert_eq!(c1, 60.0,
        "C itself did not grow — its own re-layout did not happen");
}
