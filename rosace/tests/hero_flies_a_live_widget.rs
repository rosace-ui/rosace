//! A hero in flight is the real widget, not a photograph of one.
//!
//! The old mechanism captured a `Picture` from each side and replayed it
//! morphed. That froze anything moving inside a hero for the whole flight — a
//! spinner, a video, a progress bar — and re-captured BOTH screens every
//! frame to do it.
//!
//! The flight is a promoted LIVE widget now: it paints each frame, reflows at
//! each interpolated size, and keeps animating.
//!
//! The property that actually distinguishes the two is REFLOW, and it took a
//! discarded first attempt to see that. A test built on "does the content
//! change mid-flight" does NOT discriminate: the capture mechanism
//! re-captured both screens every frame, so its content changed too. What it
//! could never do is lay the widget out at the interpolated size — it
//! captured at the natural size and SCALED the picture.
//!
//! So the hero here tiles its rect with fixed 10px cells. Laid out at each
//! interpolated size, the cell COUNT grows as it flies. Scaled from a capture,
//! the count is fixed at whatever the natural size held and only the drawn
//! size changes.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const W: u32 = 300;
const H: u32 = 240;
const MARK: (u8, u8, u8) = (0, 200, 0);

/// Tiles its rect with fixed-size cells: 6px of colour every 10px across.
/// The CELL COUNT is a direct readout of the width it was laid out at.
#[derive(Clone)]
struct Tiled(Arc<AtomicUsize>);
impl Widget for Tiled {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size {
            width: c.constraints.max_width_f32().min(400.0),
            height: c.constraints.max_height_f32().min(400.0),
        }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.0.fetch_add(1, Ordering::SeqCst);
        let r = ctx.rect;
        let h = r.size.height.max(1.0);
        let mut x = 0.0;
        while x + 6.0 <= r.size.width {
            ctx.fill_rect(
                Rect {
                    origin: Point { x: r.origin.x + x, y: r.origin.y },
                    size: Size { width: 6.0, height: h },
                },
                Color::rgb(MARK.0, MARK.1, MARK.2),
            );
            x += 10.0;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Screen { Small, Large }

struct App {
    paints: Arc<AtomicUsize>,
}
impl Component for App {
    fn build(&self, ctx: &mut Context) -> BoxedWidget {
        let nav = ScreenNav::new(ctx, Screen::Small);
        let paints = Arc::clone(&self.paints);
        let build_screen = move |s: Screen| -> BoxedWidget {
            let t = Tiled(Arc::clone(&paints));
            match s {
                Screen::Small => Arc::new(Column::new().child(
                    Container::new().width(30.0).height(30.0).child(t).hero_tag("live"),
                )),
                Screen::Large => Arc::new(Column::new().child(
                    Container::new().width(160.0).height(160.0).child(t).hero_tag("live"),
                )),
            }
        };
        let screen = nav.current().unwrap_or(Screen::Small);
        let body = build_screen(screen);
        let outgoing = nav.previous().map(&build_screen);
        // Kick the navigation on the first build so a transition is running.
        if screen == Screen::Small {
            nav.push(Screen::Large);
        }
        rosace::widgets::tree::ScreenTransitionView::new(
            body, nav.current_key(), outgoing, nav.previous_key(),
            nav.transition_handle(), nav.stack_keys(),
        ).boxed()
    }
}

/// How many separate cells appear on the busiest row, across both canvases.
fn cell_count(a: &SkiaCanvas, b: &SkiaCanvas) -> u32 {
    [a, b].iter().map(|c| {
        let px = c.pixels();
        let (w, h) = (c.width(), c.height());
        let mut best = 0u32;
        for y in 0..h {
            let (mut cells, mut prev) = (0u32, false);
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let hit = i + 2 < px.len()
                    && px[i].abs_diff(MARK.0) < 8
                    && px[i + 1].abs_diff(MARK.1) < 8
                    && px[i + 2].abs_diff(MARK.2) < 8;
                if hit && !prev { cells += 1; }
                prev = hit;
            }
            best = best.max(cells);
        }
        best
    }).max().unwrap_or(0)
}

#[test]
fn a_hero_keeps_painting_while_it_is_in_flight() {
    let _guard = exclusive();
    rosace_animate::set_frame_dt(1.0 / 60.0);
    let paints = Arc::new(AtomicUsize::new(0));
    let mut e = FrameEngine::new(
        Box::new(App { paints: Arc::clone(&paints) }),
        FontCache::embedded(),
    );
    let (mut a, mut b) = (SkiaCanvas::new(W, H), SkiaCanvas::new(W, H));

    // Fly it, counting cells every frame.
    let mut counts = std::collections::BTreeSet::new();
    for _ in 0..30 {
        e.paint(&mut a, &mut b, &[]);
        let n = cell_count(&a, &b);
        if n > 0 { counts.insert(n); }
    }

    assert!(!counts.is_empty(), "the hero never rendered at all during the flight");
    assert!(
        counts.len() > 2,
        "the hero tiled the same {} cell count(s) {counts:?} for the whole \
         flight. Its cell count is a direct readout of the width it was laid \
         out at, so a constant count means it was captured at one size and \
         SCALED — not laid out at each interpolated size.",
        counts.len()
    );
}
