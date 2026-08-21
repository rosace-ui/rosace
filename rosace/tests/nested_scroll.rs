//! A scrollable inside a scrollable must not run away.
//!
//! Found by `wrapper_nesting.rs`: `ScrollView x Card` inside an outer
//! `ScrollView` OOM-killed the test process (SIGKILL). A horizontal list inside
//! a vertical one is an ordinary pattern, so this is a real crash, not a
//! pathological construction.
//!
//! The suspected mechanism is unbounded constraints compounding: an outer
//! `ScrollView` measures its content against an unbounded scroll axis, and an
//! inner scrollable given an unbounded extent tries to materialise something
//! proportional to it.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Fills the space it is offered — the ordinary "expand to fit" widget — but
/// falls back to an intrinsic size on an axis nobody bounded, which is what a
/// well-behaved widget must do. Returning the unbounded value verbatim is a bug
/// in the WIDGET, and `layout_child`'s finite-size assertion now names it.
struct Filler;
impl Widget for Filler {
    fn layout(&self, c: &LayoutCtx) -> Size {
        let w = c.constraints.max_width_f32();
        let h = c.constraints.max_height_f32();
        Size {
            width:  if w.is_finite() { w } else { 100.0 },
            height: if h.is_finite() { h } else { 40.0 },
        }
    }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(2, 2, 2)); }
}

struct App;
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        ScrollView::new(
            Column::new()
                .child(ScrollView::new(Column::new().child(Filler)))
        ).boxed()
    }
}

/// The size a nested scrollable reports must stay finite. An infinite or
/// enormous value is what the compositor then tries to allocate a texture for.
#[test]
fn a_scrollable_inside_a_scrollable_reports_a_finite_size() {
    let _guard = exclusive();
    let mut e = FrameEngine::new(Box::new(App), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(200, 200), SkiaCanvas::new(200, 200));

    e.paint(&mut a, &mut b, &[]);

    for n in e.inspect_tree() {
        if let Some(r) = n.rect {
            assert!(r.size.width.is_finite() && r.size.height.is_finite(),
                "`{}` reported a non-finite size {:?} — an unbounded constraint \
                 reached a widget that passed it straight through", n.tag, r.size);
            assert!(r.size.height < 1.0e6 && r.size.width < 1.0e6,
                "`{}` reported {:?}, which is not a size anything can allocate",
                n.tag, r.size);
        }
    }
}
