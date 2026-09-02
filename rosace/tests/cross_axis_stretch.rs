//! `CrossAxisAlignment::Stretch` makes children fill the cross axis.
//!
//! It was declared in the enum and honoured NOWHERE: `layout_row` sized the
//! container for it, but no child was ever measured to fill, so
//! `.cross_axis_alignment(Stretch)` silently did nothing.
//!
//! Silently doing nothing is the worst of the three options, and it had a
//! real cost. The showcase put a `Divider::vertical()` in a stretched `Row`
//! inside a vertical `ScrollView`, expecting stretch to bound its height. It
//! did not, so the divider was measured against the unbounded incoming height,
//! returned `Size { width: 1.0, height: inf }` and tripped the non-finite-size
//! assertion — taking the app down on that page.
//!
//! Stretch only means something with a BOUNDED cross axis: "fill the available
//! height" has no answer inside a vertical `ScrollView`. Flutter raises on
//! exactly that. Here it warns in dev and leaves the sizes alone, which keeps
//! the failure the app author's to fix while making it visible instead of
//! silent.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Arc, Mutex};

/// Reports the rect it was painted at.
struct Probe(Arc<Mutex<Option<Rect>>>, f32, f32);
impl Widget for Probe {
    fn layout(&self, c: &LayoutCtx) -> Size {
        // Natural size, but never larger than offered.
        Size {
            width: self.1.min(c.constraints.max_width_f32()),
            height: self.2.min(c.constraints.max_height_f32()),
        }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        *self.0.lock().unwrap() = Some(ctx.rect);
        ctx.fill_rect(ctx.rect, Color::rgb(90, 90, 90));
    }
}

fn run(root: BoxedWidget) {
    struct A(Mutex<Option<BoxedWidget>>);
    impl Component for A {
        fn build(&self, _c: &mut Context) -> BoxedWidget {
            self.0.lock().unwrap().clone().unwrap()
        }
    }
    let mut e = FrameEngine::new(Box::new(A(Mutex::new(Some(root)))), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(400, 400), SkiaCanvas::new(400, 400));
    for _ in 0..3 { e.paint(&mut a, &mut b, &[]); }
}

/// With a bounded cross axis, every child fills it.
#[test]
fn a_stretched_row_makes_its_children_fill_the_available_height() {
    let short = Arc::new(Mutex::new(None));
    let tall = Arc::new(Mutex::new(None));
    run(Row::new()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(Probe(Arc::clone(&short), 40.0, 20.0))
        .child(Probe(Arc::clone(&tall), 40.0, 80.0))
        .boxed());

    let s = short.lock().unwrap().expect("short child painted");
    let t = tall.lock().unwrap().expect("tall child painted");
    assert_eq!(
        (s.size.height, t.size.height), (400.0, 400.0),
        "both children should fill the 400px window height, got {} and {}. \
         Stretch sizes the container but a child only becomes that tall if it \
         is MEASURED that way.",
        s.size.height, t.size.height
    );
}

#[test]
fn without_stretch_children_keep_their_own_height() {
    let short = Arc::new(Mutex::new(None));
    run(Row::new()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .child(Probe(Arc::clone(&short), 40.0, 20.0))
        .child(Probe(Arc::new(Mutex::new(None)), 40.0, 80.0))
        .boxed());
    assert_eq!(
        short.lock().unwrap().unwrap().size.height, 20.0,
        "Start must not resize anything"
    );
}

/// Unbounded cross axis: there is nothing to fill, so children keep their own
/// sizes. Asserted so the no-op is a DECISION with a warning behind it rather
/// than the accident it used to be.
///
/// Measured directly rather than through a `ScrollView`: a vertical
/// `ScrollView` turns out to hand its child a BOUNDED height here, so routing
/// through one tested the bounded path while claiming to test this one.
#[test]
fn stretch_is_inert_when_the_cross_axis_is_unbounded() {
    let a = Arc::new(Mutex::new(None));
    let b = Arc::new(Mutex::new(None));
    let row = Row::new()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(Probe(Arc::clone(&a), 40.0, 30.0))
        .child(Probe(Arc::clone(&b), 40.0, 50.0));

    let font = FontCache::embedded();
    let theme = rosace::theme::built_in::dark_theme();
    let sizes = row.layout_sizes_for_test(&LayoutCtx::new(
        Constraints::loose(400.0, f32::INFINITY),
        &font,
        &theme,
    ));

    assert_eq!(
        (sizes[0].height, sizes[1].height),
        (30.0, 50.0),
        "inside an unbounded height there is no available height to fill, so \
         each child keeps its own"
    );
}

#[test]
fn a_stretched_column_makes_its_children_fill_the_available_width() {
    let narrow = Arc::new(Mutex::new(None));
    run(Column::new()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(Probe(Arc::clone(&narrow), 30.0, 20.0))
        .child(Probe(Arc::new(Mutex::new(None)), 90.0, 20.0))
        .boxed());
    assert_eq!(
        narrow.lock().unwrap().unwrap().size.width, 400.0,
        "the narrow child should fill the 400px window width"
    );
}
