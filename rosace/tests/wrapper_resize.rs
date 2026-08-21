//! A widget under a default-`layout` wrapper must still be able to RESIZE.
//!
//! `Pressable`, `Tooltip`, `RepaintBoundary`, `WithFocus`, `Semantics` and a
//! dozen others inherit the `Widget` trait's default `layout`, which measures
//! its one child through a DETACHED context. A detached measure consumes no
//! layout slot, so the wrapper's `layout_cursor` stays at 0 — and the
//! relayout-boundary rule reads a zero cursor as "measured no children, so
//! nothing below can change my size".
//!
//! That inference is exactly inverted here: the wrapper did measure a child.
//! With the boundary standing, `mark_dirty_with_ancestors` stopped at the
//! wrapper, its parent hit the layout cache and returned the STALE size, and
//! the child repainted its new content at its old height. Plausible pixels,
//! silently wrong.

use rosace::prelude::*;
use rosace::widgets::tree::{refresh_state, Children, LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A wrapper that inherits the trait's DEFAULT `layout` and overrides only
/// `paint` — the shape of Semantics<W>, Pressable<W>, IgnorePointer, etc.
struct DefaultLayoutWrapper {
    child: Arc<dyn Widget>,
}
impl Widget for DefaultLayoutWrapper {
    fn children(&self) -> Children<'_> {
        Children::One(&*self.child)
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        ctx.paint_child(r, &*self.child);
    }
}

struct Grower(Arc<AtomicUsize>);
impl Widget for Grower {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        Size { width: 50.0, height: self.0.load(Ordering::SeqCst) as f32 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(3, 4, 5));
        ctx.register_hit(Arc::new(|| refresh_state()));
    }
}

struct App {
    height: Arc<AtomicUsize>,
    wrapped: bool,
}

impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let grower = Grower(self.height.clone());
        let child: Arc<dyn Widget> = if self.wrapped {
            Arc::new(DefaultLayoutWrapper { child: Arc::new(grower) })
        } else {
            Arc::new(grower)
        };
        Column::new()
            .child(child)
            .child(Container::new().width(10.0).height(10.0))
            .boxed()
    }
}

fn run(wrapped: bool) -> (f32, f32) {
    let height = Arc::new(AtomicUsize::new(20));
    let mut e = FrameEngine::new(
        Box::new(App { height: height.clone(), wrapped }),
        FontCache::embedded(),
    );
    let (mut a, mut b) = (SkiaCanvas::new(300, 300), SkiaCanvas::new(300, 300));
    e.paint(&mut a, &mut b, &[]);

    let find = |e: &FrameEngine| {
        e.inspect_tree()
            .iter()
            .find(|n| n.tag.ends_with("::Grower"))
            .and_then(|n| n.rect)
            .map(|r| r.size.height)
            .expect("grower painted")
    };
    let h0 = find(&e);

    let rect = e
        .inspect_tree()
        .iter()
        .find(|n| n.tag.ends_with("::Grower"))
        .and_then(|n| n.rect)
        .unwrap();
    height.store(120, Ordering::SeqCst);
    let (cx, cy) = (rect.origin.x + 1.0, rect.origin.y + 1.0);
    e.paint(
        &mut a,
        &mut b,
        &[
            rosace_platform::InputEvent::MouseDown { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
        ],
    );
    e.paint(&mut a, &mut b, &[]);
    let h1 = find(&e);
    (h0, h1)
}

/// Baseline — no wrapper, so no boundary is inferred either way.
#[test]
fn a_bare_widget_resizes_on_refresh() {
    let (h0, h1) = run(false);
    assert_eq!(h0, 20.0);
    assert_eq!(h1, 120.0, "a bare widget kept its old height after refresh_state()");
}

/// The regression.
#[test]
fn a_widget_under_a_default_layout_wrapper_resizes_on_refresh() {
    let (h0, h1) = run(true);
    assert_eq!(h0, 20.0);
    assert_eq!(
        h1, 120.0,
        "the wrapped widget grew to 120 but painted at {h1} — its wrapper was \
         treated as a relayout boundary, so the size change never propagated"
    );
}
