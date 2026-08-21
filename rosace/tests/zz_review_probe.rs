//! TEMPORARY review probe — delete after use.

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
    fn build(&self, _ctx: &mut Context) -> Element {
        let grower = Grower(self.height.clone());
        let child: Arc<dyn Widget> = if self.wrapped {
            Arc::new(DefaultLayoutWrapper { child: Arc::new(grower) })
        } else {
            Arc::new(grower)
        };
        Column::new()
            .child(child)
            .child(Container::new().width(10.0).height(10.0))
            .into_element()
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

#[test]
fn probe_bare() {
    let (h0, h1) = run(false);
    println!("BARE:    h0={h0} h1={h1}");
    assert_eq!(h1, 120.0, "bare grower");
}

#[test]
fn probe_wrapped() {
    let (h0, h1) = run(true);
    println!("WRAPPED: h0={h0} h1={h1}");
    assert_eq!(h1, 120.0, "wrapped grower");
}
