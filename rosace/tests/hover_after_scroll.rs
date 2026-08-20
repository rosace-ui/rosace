//! Hover must follow the POINTER, not the content.
//!
//! Hover is recomputed on `MouseMove`. Scrolling moves the content underneath a
//! stationary pointer and produces no `MouseMove`, so nothing re-evaluates:
//! the highlight stays attached to the row it was on and travels away with it,
//! while the row now under the cursor stays unlit.
//!
//! Flutter re-runs hit-testing after a frame for exactly this reason
//! (`MouseTracker`). Reported live as "when we scroll it moves with the mouse
//! and persists on the current item".

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

struct Row(usize);
impl Widget for Row {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 160.0, height: 40.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        let h = ctx.hovered();
        ctx.fill_rect(ctx.rect, if h { Color::rgb(200, 0, 0) } else { Color::rgb(20, 20, 20) });
        ctx.hoverable();
        ctx.register_hit(std::sync::Arc::new(|| {}));
    }
}

struct App;
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> Element {
        let mut col = Column::new();
        for i in 0..20 {
            col = col.child(Row(i));
        }
        ScrollView::new(col).into_element()
    }
}

/// Park the pointer, scroll underneath it, and check which row is lit.
#[test]
fn scrolling_under_a_stationary_pointer_moves_the_hover_to_the_new_row() {
    let _guard = exclusive();
    let mut e = FrameEngine::new(Box::new(App), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(200, 200), SkiaCanvas::new(200, 200));

    e.paint(&mut a, &mut b, &[]);

    // Park the pointer over whatever row sits at this point.
    let (px, py) = (80.0, 100.0);
    e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove { x: px, y: py }]);
    e.paint(&mut a, &mut b, &[]);

    let hovered_rect_before = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Row") && n.hovered)
        .and_then(|n| n.rect)
        .expect("a row should be hovered under the parked pointer");
    assert!(
        hovered_rect_before.origin.y <= py
            && py <= hovered_rect_before.origin.y + hovered_rect_before.size.height,
        "the hovered row is not the one under the pointer to begin with"
    );

    // Scroll WITHOUT moving the pointer.
    for _ in 0..3 {
        e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::Scroll {
            x: px, y: py, delta_x: 0.0, delta_y: -50.0,
        }]);
        e.paint(&mut a, &mut b, &[]);
    }

    let hovered_after = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Row") && n.hovered)
        .and_then(|n| n.rect)
        .expect("some row should still be hovered after scrolling");

    assert!(
        hovered_after.origin.y <= py
            && py <= hovered_after.origin.y + hovered_after.size.height,
        "after scrolling, the lit row is at y={}..{} but the pointer is at y={py} — \
         the highlight travelled with the content instead of staying under the \
         cursor, so the row actually beneath the pointer is unlit",
        hovered_after.origin.y,
        hovered_after.origin.y + hovered_after.size.height,
    );
}
