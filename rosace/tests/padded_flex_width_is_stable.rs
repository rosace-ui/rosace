//! A padded `Row`/`Column` must give its children the same width on a
//! paint-only frame as on a frame that laid out first.
//!
//! `paint` shrank the rect by the padding and handed those already-shrunk
//! constraints to `measure`, which subtracts the padding itself — so paint
//! computed `width - 2 * padding` while `layout`, which passes the unshrunk
//! constraints, computed `width - padding`.
//!
//! Which one a frame got depended on whether the frame-scoped measure cache
//! had been filled by a layout pass. Interacting with a widget produces
//! exactly the alternating pattern — a repaint on the input frame, a full
//! pass on the next — so every child of a padded container oscillated between
//! the two widths at frame rate. Reported as "dragging one slider makes all
//! the sliders vibrate"; the sliders were incidental, the padding was not.
//!
//! Asserted by driving BOTH kinds of frame and comparing, because either kind
//! alone is self-consistent and looks perfectly correct.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

const PAD: f32 = 16.0;
const W: u32 = 400;

/// Fills the width it is offered and reports it.
struct Probe(Arc<Mutex<Vec<f32>>>);
impl Widget for Probe {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: 24.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.0.lock().unwrap().push(ctx.rect.size.width);
        ctx.fill_rect(ctx.rect, Color::rgb(70, 90, 160));
    }
}

struct App(Arc<Mutex<Vec<f32>>>);
impl Component for App {
    fn build(&self, _c: &mut Context) -> BoxedWidget {
        // The ScrollView matters: it is a relayout boundary, so an input
        // frame repaints the Column WITHOUT a layout pass above it. That is
        // the frame on which `measure` runs from `paint` and sees the
        // already-shrunk constraints.
        ScrollView::new(
            Column::new()
                .padding(EdgeInsets::all(PAD))
                .child(Probe(Arc::clone(&self.0)))
                .child(Pressable::new(Probe(Arc::clone(&self.0)), || {})),
        )
        .boxed()
    }
}

#[test]
fn padding_is_applied_once_on_every_kind_of_frame() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut e = FrameEngine::new(Box::new(App(Arc::clone(&seen))), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(W, 300), SkiaCanvas::new(W, 300));

    // Alternate the two frame kinds the way an interaction does: an input
    // frame (repaint, no full layout pass) and then a plain frame. This is
    // the drag pattern that made it visible, reproduced without a drag.
    e.paint(&mut a, &mut b, &[]);
    for i in 0..12 {
        if i % 2 == 0 {
            e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::MouseMove {
                x: 100.0 + i as f32, y: 30.0,
            }]);
        } else {
            e.paint(&mut a, &mut b, &[]);
        }
    }

    let widths: BTreeSet<i64> =
        seen.lock().unwrap().iter().map(|w| (*w * 100.0) as i64).collect();

    assert_eq!(
        widths.len(), 1,
        "the child was laid out at {} different widths across frame kinds: \
         {:?}. Paint and layout disagree about how many times the padding \
         applies, so anything being interacted with oscillates between them.",
        widths.len(),
        widths.iter().map(|w| *w as f32 / 100.0).collect::<Vec<_>>()
    );
    assert_eq!(
        *widths.iter().next().unwrap() as f32 / 100.0,
        W as f32 - 2.0 * PAD,
        "padding should be subtracted exactly once"
    );
}
