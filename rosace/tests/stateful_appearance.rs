//! A `Stateful` rebuild that changes only APPEARANCE must still reach the screen.
//!
//! `refresh_state()` marks the `Stateful`'s own node and its ancestors — not its
//! children. So the built subtree occupies a child node with `needs_paint ==
//! false` and an unchanged rect, which is exactly the condition `paint_child`
//! replays on. If the rebuild produced a different COLOUR at the same size and
//! place, the cached picture is stale and the change never appears.
//!
//! Nothing catches this: the frame is TARGETED (a node mark dirties no
//! component), so `is_structural_frame()` is false; the widget type is
//! unchanged, so `adopt_tag` sees nothing; and the size is unchanged, so the
//! move path does not apply either.

use rosace::prelude::*;
use rosace::widgets::tree::{refresh_state, BoxedWidget, LayoutCtx, PaintCtx, StatefulExt, StatefulWidget};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Same size and position every time; only its colour depends on the state.
struct Swatch {
    shade: u8,
    drawn: Arc<Mutex<Vec<u8>>>,
}
impl Widget for Swatch {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 60.0, height: 30.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        self.drawn.lock().unwrap().push(self.shade);
        ctx.fill_rect(ctx.rect, Color::rgb(self.shade, 0, 0));
        ctx.register_hit(Arc::new(|| refresh_state()));
    }
}

struct Panel {
    shade: Arc<AtomicUsize>,
    drawn: Arc<Mutex<Vec<u8>>>,
}
impl StatefulWidget for Panel {
    fn build(&self) -> BoxedWidget {
        Arc::new(Swatch {
            shade: self.shade.load(Ordering::SeqCst) as u8,
            drawn: self.drawn.clone(),
        })
    }
}

struct App {
    shade: Arc<AtomicUsize>,
    drawn: Arc<Mutex<Vec<u8>>>,
}
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        Column::new()
            .child(Panel { shade: self.shade.clone(), drawn: self.drawn.clone() }.stateful())
            .boxed()
    }
}

#[test]
fn a_rebuild_that_changes_only_colour_still_repaints() {
    let _guard = exclusive();
    let shade = Arc::new(AtomicUsize::new(10));
    let drawn: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

    let mut e = FrameEngine::new(
        Box::new(App { shade: shade.clone(), drawn: drawn.clone() }),
        FontCache::embedded(),
    );
    let (mut a, mut b) = (SkiaCanvas::new(200, 200), SkiaCanvas::new(200, 200));

    e.paint(&mut a, &mut b, &[]);
    assert_eq!(drawn.lock().unwrap().last().copied(), Some(10));

    // Change the colour, then refresh through the real click path.
    let r = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Swatch")).and_then(|n| n.rect)
        .expect("the swatch painted");
    shade.store(200, Ordering::SeqCst);
    let (cx, cy) = (r.origin.x + 2.0, r.origin.y + 2.0);
    e.paint(&mut a, &mut b, &[
        rosace_platform::InputEvent::MouseDown { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
        rosace_platform::InputEvent::MouseUp   { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
    ]);
    e.paint(&mut a, &mut b, &[]);

    let seen = drawn.lock().unwrap().clone();
    assert!(seen.contains(&200),
        "the rebuilt subtree never painted — its node was unmarked, its rect \
         unchanged and its type the same, so the stale picture was replayed and \
         the colour change never reached the screen. Saw shades {seen:?}");
}
