//! A leaf changing size must not re-measure the whole chrome above it.
//!
//! Written to FAIL before relayout boundaries exist.
//!
//! `mark_dirty_with_ancestors` marks `needs_layout` on EVERY ancestor up to the
//! arena root — past `Scaffold`, past the navigator. Not because those need
//! re-measuring (their size is the window and cannot change) but because layout
//! is one top-down recursion and an unmarked node returns its cache WITHOUT
//! descending. The marks are a breadcrumb trail to reach the dirty node.
//!
//! A node whose `layout` measured no children is a boundary: its size is a
//! function of (constraints, font, theme) alone, so nothing beneath it can
//! change it. Layout can restart there and everything above is left alone.

use rosace::prelude::*;
use rosace::widgets::tree::{refresh_state, LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Counts its own `layout` calls. Stands in for the app chrome — `Scaffold`,
/// the navigator — that sits between the root and the content.
///
/// Sized by its constraints and NOT by its child, which is exactly what makes
/// it a boundary: it measures the child during `paint`, never during `layout`.
struct Chrome {
    layouts: Arc<AtomicUsize>,
    child: Arc<dyn Widget>,
}

impl Widget for Chrome {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        self.layouts.fetch_add(1, Ordering::SeqCst);
        // Deliberately measures NO children — fills what it is given.
        Size {
            width: ctx.constraints.max_width_f32(),
            height: ctx.constraints.max_height_f32(),
        }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        ctx.paint_child(r, &*self.child);
    }
}

/// The content whose size changes.
struct Grower(Arc<AtomicUsize>, Arc<AtomicUsize>);
impl Widget for Grower {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        self.1.fetch_add(1, Ordering::SeqCst);
        Size { width: 50.0, height: self.0.load(Ordering::SeqCst) as f32 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(3, 4, 5));
        ctx.register_hit(Arc::new(|| refresh_state()));
    }
}

struct App {
    chrome_layouts: Arc<AtomicUsize>,
    grower_layouts: Arc<AtomicUsize>,
    height: Arc<AtomicUsize>,
}

impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        // Column(above) → Chrome(boundary) → Column(below) → Grower
        //
        // The boundary sits BETWEEN the containers and the changing leaf, which
        // is where it can actually save anything. `Chrome` is sized by its
        // constraints alone, so when the leaf grows, `Column(below)` must
        // re-measure and `Column(above)` must NOT — the boundary's own size did
        // not move, so nothing above it can be affected.
        let below = Column::new().child(Grower(
            self.height.clone(),
            self.grower_layouts.clone(),
        ));
        let boundary = Chrome { layouts: self.chrome_layouts.clone(), child: Arc::new(below) };
        Column::new()
            .child(Counted(self.chrome_layouts.clone()))
            .child(boundary)
            .boxed()
    }
}

/// A sibling above the boundary whose `layout` calls are counted — it stands in
/// for the chrome that should be left alone.
struct Counted(Arc<AtomicUsize>);
impl Widget for Counted {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        self.0.fetch_add(1, Ordering::SeqCst);
        Size { width: 10.0, height: 10.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(1, 1, 1)); }
}

/// The claim: a leaf changing size re-measures the leaf and its flex parent,
/// and leaves the sized-by-constraints chrome above it untouched.
#[test]
fn a_leaf_resizing_does_not_re_measure_the_chrome_above_it() {
    let _guard = exclusive();
    let chrome = Arc::new(AtomicUsize::new(0));
    let grower = Arc::new(AtomicUsize::new(0));
    let height = Arc::new(AtomicUsize::new(20));

    let mut e = FrameEngine::new(
        Box::new(App {
            chrome_layouts: chrome.clone(),
            grower_layouts: grower.clone(),
            height: height.clone(),
        }),
        FontCache::embedded(),
    );
    let (mut a, mut b) = (SkiaCanvas::new(300, 300), SkiaCanvas::new(300, 300));

    e.paint(&mut a, &mut b, &[]);
    let chrome_after_first = chrome.load(Ordering::SeqCst);
    assert!(chrome_after_first >= 1, "the boundary and its sibling measured on the first frame");

    // The leaf grows, and asks for a targeted update.
    let rect = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Grower")).and_then(|n| n.rect)
        .expect("the grower painted");
    height.store(120, Ordering::SeqCst);
    let (cx, cy) = (rect.origin.x + 1.0, rect.origin.y + 1.0);
    e.paint(&mut a, &mut b, &[
        rosace_platform::InputEvent::MouseDown { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
        rosace_platform::InputEvent::MouseUp   { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
    ]);

    let grower_before = grower.load(Ordering::SeqCst);
    let chrome_before = chrome.load(Ordering::SeqCst);
    e.paint(&mut a, &mut b, &[]);

    assert!(grower.load(Ordering::SeqCst) > grower_before,
        "the leaf itself must re-measure — it is the thing that changed");
    assert_eq!(chrome.load(Ordering::SeqCst), chrome_before,
        "work above the boundary re-ran; the boundary is sized by its constraints \
         alone, so the leaf growing cannot change its size and the walk should \
         never have gone past it");
}

/// The safety half: the leaf must still actually grow. A boundary that stops
/// the walk too early would leave the old size in place — which is the failure
/// mode this whole mechanism risks.
#[test]
fn the_leaf_still_grows_through_the_boundary() {
    let _guard = exclusive();
    let chrome = Arc::new(AtomicUsize::new(0));
    let grower = Arc::new(AtomicUsize::new(0));
    let height = Arc::new(AtomicUsize::new(20));

    let mut e = FrameEngine::new(
        Box::new(App {
            chrome_layouts: chrome.clone(),
            grower_layouts: grower.clone(),
            height: height.clone(),
        }),
        FontCache::embedded(),
    );
    let (mut a, mut b) = (SkiaCanvas::new(300, 300), SkiaCanvas::new(300, 300));

    e.paint(&mut a, &mut b, &[]);
    let h0 = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Grower")).and_then(|n| n.rect)
        .map(|r| r.size.height).expect("painted");
    assert_eq!(h0, 20.0);

    let rect = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Grower")).and_then(|n| n.rect).unwrap();
    height.store(120, Ordering::SeqCst);
    let (cx, cy) = (rect.origin.x + 1.0, rect.origin.y + 1.0);
    e.paint(&mut a, &mut b, &[
        rosace_platform::InputEvent::MouseDown { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
        rosace_platform::InputEvent::MouseUp   { x: cx, y: cy, button: rosace_platform::MouseButton::Left },
    ]);
    e.paint(&mut a, &mut b, &[]);

    let h1 = e.inspect_tree().iter()
        .find(|n| n.tag.ends_with("::Grower")).and_then(|n| n.rect)
        .map(|r| r.size.height).expect("still painted");
    assert_eq!(h1, 120.0, "the leaf did not grow — the boundary stopped the walk too early");
}
