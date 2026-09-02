//! A hero-tagged widget must never simply disappear.
//!
//! Reported live: after a push, the destination screen rendered its title and
//! its Back button but the hero image was missing — intermittently.
//!
//! An endpoint stands aside while its element is in the air, because the
//! promoted copy is the one on screen. Deciding that from "a transition is
//! running" is wrong twice over:
//!
//!   * a tag with no counterpart on the other side can never pair, so nothing
//!     flies and the widget is simply erased for the whole transition;
//!   * the frame it was erased on gets CACHED, so an ancestor replaying that
//!     picture keeps it erased long after the transition ends.
//!
//! An endpoint is now told when its tag is actually flying (`is_flying`), and
//! marks itself dirty while hidden so the frame the flight ends re-records it
//! instead of replaying an empty picture. That is Flutter's arrangement: the
//! navigator starts the flight and notifies the endpoints, rather than each
//! endpoint inferring it.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Arc, Mutex, MutexGuard};

static L: Mutex<()> = Mutex::new(());
fn ex() -> MutexGuard<'static, ()> { L.lock().unwrap_or_else(|e| e.into_inner()) }

const MARK: (u8, u8, u8) = (0, 0, 255);

struct Blue;
impl Widget for Blue {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32().min(400.0), height: c.constraints.max_height_f32().min(400.0) }
    }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(MARK.0, MARK.1, MARK.2)); }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum S { A, B }

struct App;
impl Component for App {
    fn build(&self, ctx: &mut Context) -> BoxedWidget {
        let nav = ScreenNav::new(ctx, S::A);
        let build = move |s: S| -> BoxedWidget {
            match s {
                S::A => Arc::new(Column::new().child(
                    Container::new().width(40.0).height(40.0).child(Blue).hero_tag("h"))),
                // Destination tags it DIFFERENTLY: no counterpart, so no
                // flight can ever form for either tag.
                S::B => Arc::new(Column::new().child(
                    Container::new().width(150.0).height(150.0).child(Blue).hero_tag("other"))),
            }
        };
        let screen = nav.current().unwrap_or(S::A);
        let body = build(screen);
        let outgoing = nav.previous().map(&build);
        if screen == S::A { nav.push(S::B); }
        rosace::widgets::tree::ScreenTransitionView::new(
            body, nav.current_key(), outgoing, nav.previous_key(),
            nav.transition_handle(), nav.stack_keys(),
        ).boxed()
    }
}

fn blue(a: &SkiaCanvas, b: &SkiaCanvas) -> usize {
    [a, b].iter().map(|c| c.pixels().chunks_exact(4)
        .filter(|p| p[0] == MARK.0 && p[1] == MARK.1 && p[2] == MARK.2 && p[3] == 255)
        .count()).sum()
}

#[test]
fn an_unpaired_hero_tag_still_renders() {
    let _g = ex();
    rosace_animate::set_frame_dt(1.0 / 60.0);
    let mut e = FrameEngine::new(Box::new(App), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(300, 300), SkiaCanvas::new(300, 300));

    // Sample every frame from the very start — the transition is running
    // for most of these, which is exactly when an endpoint might hide itself.
    let mut worst = usize::MAX;
    let mut worst_frame = 0;
    for i in 0..60 {
        e.paint(&mut a, &mut b, &[]);
        let n = blue(&a, &b);
        if n < worst { worst = n; worst_frame = i; }
    }
    eprintln!("minimum blue across the transition: {worst} px (frame {worst_frame})");

    assert!(
        worst > 500,
        "an UNPAIRED hero tag vanished mid-transition: only {worst} blue px on \
         frame {worst_frame}. Nothing can fly when one side carries the tag \
         and the other does not, so the widget must simply paint itself — \
         hiding it because 'a transition is running' erases it."
    );
}
