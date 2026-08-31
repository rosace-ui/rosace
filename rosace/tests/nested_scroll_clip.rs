//! A nested scroll view scrolled off the top of its page must not paint over
//! the chrome above it.
//!
//! Reported live, and it used to be structural. `ScrollView`, `TransformLayer`
//! and `InteractiveViewer` did not record into their parent's picture: they
//! attached a transform entry the engine harvested at end of frame and handed
//! to the compositor as an independent texture. By then the ancestors' clips
//! were gone — they were `PushClip`/`PopClip` commands in a picture the layer
//! was not part of — so a placed layer's `dest` rect was the only scissor it
//! had, and a nested host scrolled up painted straight over the AppBar.
//!
//! There is one scroll path now and it records into its parent's picture, so
//! the enclosing clips apply by construction rather than by a rule someone has
//! to remember to re-derive.
//!
//! This replaces `layer_clip.rs`, which asserted the same guarantee on
//! published `ScrollLayer` geometry. That mechanism no longer exists; the
//! guarantee still does, so it is asserted on PIXELS — the only description
//! that survives a change of mechanism.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const BAR_H: f32 = 60.0;
const WIN_W: u32 = 300;
const WIN_H: u32 = 400;
const BAR: (u8, u8, u8) = (10, 10, 10);
const INNER: (u8, u8, u8) = (220, 40, 40);

/// Opaque chrome the page must never paint over.
struct Bar;
impl Widget for Bar {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: BAR_H }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(BAR.0, BAR.1, BAR.2));
    }
}

/// The nested scroller's content, in a colour nothing else uses.
struct Inner;
impl Widget for Inner {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 400.0, height: 400.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(INNER.0, INNER.1, INNER.2));
    }
}

struct App;
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let mut col = Column::new();
        // Start the nested scroller well below the bar, so scrolling can
        // carry it up underneath.
        col = col.child(Container::new().width(200.0).height(200.0));
        col = col.child(
            Container::new().width(200.0).height(150.0)
                .child(ScrollView::new(Inner)),
        );
        for _ in 0..8 {
            col = col.child(Container::new().width(200.0).height(60.0));
        }
        Column::new().child(Bar).child(ScrollView::new(col)).boxed()
    }
}

struct H { e: FrameEngine, a: SkiaCanvas, b: SkiaCanvas }
impl H {
    fn new() -> Self {
        H { e: FrameEngine::new(Box::new(App), FontCache::embedded()),
            a: SkiaCanvas::new(WIN_W, WIN_H), b: SkiaCanvas::new(WIN_W, WIN_H) }
    }
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    /// Scroll at the right edge, clear of the nested scroller — a wheel event
    /// over it would be consumed to scroll ITS content and the page would
    /// never move.
    fn scroll(&mut self, dy: f32) {
        self.e.paint(&mut self.a, &mut self.b, &[rosace_platform::InputEvent::Scroll {
            x: 290.0, y: 250.0, delta_x: 0.0, delta_y: dy,
        }]);
        self.frame();
    }
    /// Does `colour` appear anywhere in the bar's band, in either buffer?
    fn colour_in_bar(&self, colour: (u8, u8, u8)) -> bool {
        [&self.a, &self.b].iter().any(|c| {
            let px = c.pixels();
            let w = c.width();
            (0..BAR_H as u32).any(|y| (0..w).any(|x| {
                let i = ((y * w + x) * 4) as usize;
                i + 2 < px.len()
                    && px[i].abs_diff(colour.0) < 6
                    && px[i + 1].abs_diff(colour.1) < 6
                    && px[i + 2].abs_diff(colour.2) < 6
            }))
        })
    }
}

#[test]
fn a_nested_scroll_view_never_paints_over_the_bar_above_it() {
    let _guard = exclusive();
    let mut h = H::new();
    for _ in 0..3 { h.frame(); }

    assert!(h.colour_in_bar(BAR), "control: the bar did not paint at all");
    assert!(
        !h.colour_in_bar(INNER),
        "control: the nested scroller is already overlapping the bar before \
         any scrolling — the fixture does not test what it claims"
    );

    // Drag the whole page up so the nested scroller travels under the bar.
    for _ in 0..8 { h.scroll(-40.0); }
    for _ in 0..30 { h.frame(); }

    assert!(
        !h.colour_in_bar(INNER),
        "the nested scroll view painted over the bar after the page scrolled \
         it underneath — its content escaped the enclosing clip"
    );
    assert!(h.colour_in_bar(BAR), "the bar stopped painting entirely");
}
