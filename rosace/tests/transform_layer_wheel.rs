//! A `TransformLayer` scrolls when you turn the wheel over it.
//!
//! It registers a scroll target that writes into the non-reactive scroll
//! channel. On the old GPU path the COMPOSITOR read that channel and shifted
//! the layer's texture sample origin, so the widget itself never had to know.
//!
//! Painting directly into the parent's picture removed that reader, and the
//! registration was left writing to a channel with no consumer — the widget
//! took the wheel event, updated the offset, and painted its child at exactly
//! the same place. Silent, and invisible to any test that only checks the
//! offset was recorded.
//!
//! Asserted on the child's node rect read from `inspect_tree`, NOT on a
//! rect captured inside `paint`. Replay-on-move re-blits a cached picture and
//! translates the node's rect without running `paint` at all, so a paint-time
//! probe records the first position and then goes quiet while the content
//! visibly moves — it reports "nothing moved" for a widget that is working.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx, TransformLayer};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};

struct Probe;
impl Widget for Probe {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        Size { width: 200.0, height: 800.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(60, 120, 180));
    }
}

struct App;
impl Component for App {
    fn build(&self, _c: &mut Context) -> BoxedWidget {
        TransformLayer::new(Probe, 200.0, 0.0).boxed()
    }
}

/// The probe's node rect, as the tree currently holds it.
fn probe_y(e: &FrameEngine) -> f32 {
    e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("Probe"))
        .and_then(|n| n.rect)
        .expect("the probe has a rect")
        .origin.y
}

#[test]
fn wheeling_over_a_transform_layer_moves_its_content() {
    let mut e = FrameEngine::new(Box::new(App), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(300, 300), SkiaCanvas::new(300, 300));

    for _ in 0..3 { e.paint(&mut a, &mut b, &[]); }
    let before = probe_y(&e);

    for _ in 0..4 {
        e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::Scroll {
            x: 100.0, y: 100.0, delta_x: 0.0, delta_y: -40.0,
        }]);
        e.paint(&mut a, &mut b, &[]);
    }
    let after = probe_y(&e);

    assert!(
        after < before - 10.0,
        "the content did not move: y went {before} -> {after}. The wheel \
         handler writes the offset into the scroll channel, so something has \
         to READ it — on the old GPU path that was the compositor."
    );
}
