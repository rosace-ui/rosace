//! A compositing layer must inherit its ancestors' clips.
//!
//! `ScrollView`, `InteractiveViewer` and `TransformLayer` do not record into
//! their parent's picture — they attach a transform entry the engine harvests
//! at end of frame and hands to the compositor as an independent texture. By
//! then the ancestors' clips are gone: they were `PushClip`/`PopClip` commands
//! in a picture this layer is not part of.
//!
//! So a nested transform host scrolled off the top of its page painted
//! straight over the AppBar (reported live). A placed layer's `dest` rect is
//! the only scissor it has, which is why the clip has to be resolved
//! structurally and applied to the placement.
//!
//! Asserted on the published layer geometry rather than pixels: cropping and
//! shifting look identical in a screenshot of a scrolling page, and only one
//! of them is correct.

use rosace::prelude::*;
use rosace::widgets::tree::{InteractiveViewer, LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stands in for an AppBar: opaque chrome the page must never paint over.
struct Bar;
impl Widget for Bar {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: BAR_H }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(10, 10, 10));
    }
}

const BAR_H: f32 = 60.0;
const WIN_W: u32 = 300;
const WIN_H: u32 = 400;

/// A page whose scrollable body holds a nested transform host.
struct App;
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let mut col = Column::new();
        // Push the viewer down so it starts well below the bar, then scroll
        // it up underneath.
        col = col.child(Container::new().width(200.0).height(200.0));
        col = col.child(
            Container::new()
                .width(200.0)
                .height(150.0)
                .child(InteractiveViewer::new(
                    Container::new().width(400.0).height(400.0),
                )),
        );
        for _ in 0..8 {
            col = col.child(Container::new().width(200.0).height(60.0));
        }
        Column::new()
            .child(Bar)
            .child(ScrollView::new(col))
            .boxed()
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
}

impl H {
    fn new() -> Self {
        H {
            e: FrameEngine::new(Box::new(App), FontCache::embedded()),
            a: SkiaCanvas::new(WIN_W, WIN_H),
            b: SkiaCanvas::new(WIN_W, WIN_H),
        }
    }
    fn frame(&mut self) {
        self.e.paint(&mut self.a, &mut self.b, &[]);
    }
    /// Scrolled at the right edge, clear of the nested viewer — a wheel event
    /// over the viewer would be consumed by it to pan its own content, and the
    /// page would never move.
    fn scroll(&mut self, dy: f32) {
        self.e.paint(&mut self.a, &mut self.b, &[rosace_platform::InputEvent::Scroll {
            x: 290.0, y: 250.0, delta_x: 0.0, delta_y: dy,
        }]);
    }
    /// The nested viewer's published layer, as `(dest, src_bias)`.
    fn nested_layer(&self) -> Option<((f32, f32, f32, f32), (f32, f32))> {
        let layers = rosace_platform::take_scroll_layers()?;
        // The page's own ScrollView is the full-width one; the nested viewer
        // is narrower, which is what distinguishes them without hardcoding ids.
        layers
            .iter()
            .find(|l| l.dest.2 > 0.0 && l.dest.2 < (WIN_W as f32) * 0.9)
            .map(|l| (l.dest, l.src_bias))
    }
}

/// Baseline: unscrolled, the nested layer is fully below the bar and uncropped.
#[test]
fn an_unclipped_nested_layer_publishes_its_whole_viewport() {
    let _guard = exclusive();
    let mut h = H::new();
    h.frame();

    let (dest, bias) = h.nested_layer().expect("the nested viewer published a layer");
    assert!(
        dest.1 >= BAR_H,
        "the nested layer starts at y={} — the fixture is wrong, it should begin below the bar",
        dest.1
    );
    assert_eq!(bias, (0.0, 0.0), "nothing was cropped, so nothing should be biased");
}

/// The regression: scrolled under the bar, the layer is cropped — not shifted.
#[test]
fn a_nested_layer_scrolled_under_the_app_bar_is_cropped_by_it() {
    let _guard = exclusive();
    let mut h = H::new();
    h.frame();
    let (before, _) = h.nested_layer().expect("the nested viewer published a layer");
    let full_h = before.3;

    // Scroll far enough that the viewer's top passes above the bar's bottom.
    for _ in 0..5 {
        h.scroll(-50.0);
        h.frame();
    }

    let (dest, bias) = h.nested_layer().expect("the layer is still published while partly visible");

    assert!(
        dest.1 >= BAR_H - 0.5,
        "the nested layer is placed at y={}, above the bar's bottom edge at {BAR_H} — it \
         paints over the AppBar",
        dest.1
    );
    assert!(
        dest.3 < full_h,
        "the layer kept its full height {full_h} after being scrolled under the bar, so it \
         was never cropped"
    );
    assert!(
        bias.1 > 0.0,
        "the layer was cropped to {dest:?} but its sample origin was not biased ({bias:?}) — \
         the content shifted down instead of being clipped at the top"
    );
}
