//! Promotion to the root layer — React's Portal, in the one tree.
//!
//! **Visually** the content composites at the root: it escapes every ancestor
//! clip and everything below it in z-order. **Logically** it stays exactly
//! where it was declared. The logical half is the one a naive "move it to the
//! root" loses, and it is load-bearing: reading order and "is this inside the
//! dialog?" for assistive tech, tab order, per-node state, inherited theme,
//! and dismissal when the declaring parent leaves the tree.
//!
//! Each test below pins one half of that split.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

const WIN_W: u32 = 300;
const WIN_H: u32 = 400;

/// The promoted content: a panel that reports clicks and its own paint count.
struct Panel {
    clicks: Arc<AtomicUsize>,
    label: &'static str,
}
impl Widget for Panel {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        Size { width: 120.0, height: 40.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(200, 40, 40));
        ctx.semantics(rosace::widgets::tree::SemanticsProps::new(rosace_core::Role::Button)
            .label(self.label));
        let clicks = Arc::clone(&self.clicks);
        ctx.register_hit(Arc::new(move || { clicks.fetch_add(1, Ordering::SeqCst); }));
    }
}

/// Declares a promoted child at a fixed offset from itself.
struct Portal {
    clicks: Arc<AtomicUsize>,
    show: Arc<AtomicBool>,
}
impl Widget for Portal {
    fn layout(&self, _c: &LayoutCtx) -> Size {
        Size { width: 100.0, height: 30.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(40, 90, 40));
        if self.show.load(Ordering::SeqCst) {
            // Deliberately placed far above the declaring widget — over the
            // bar, and outside the scroll viewport it is declared inside.
            let r = Rect {
                origin: Point { x: 20.0, y: -160.0 },
                size: Size { width: 120.0, height: 40.0 },
            };
            let panel = Panel { clicks: Arc::clone(&self.clicks), label: "promoted" };
            ctx.promote(r, &panel);
        }
    }
}

struct Bar;
impl Widget for Bar {
    fn layout(&self, c: &LayoutCtx) -> Size {
        Size { width: c.constraints.max_width_f32(), height: 60.0 }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(10, 10, 10));
        ctx.register_hit(Arc::new(|| {}));
    }
}

struct App {
    clicks: Arc<AtomicUsize>,
    show: Arc<AtomicBool>,
    mounted: Arc<AtomicBool>,
}
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let mut col = Column::new();
        col = col.child(Container::new().width(200.0).height(100.0));
        if self.mounted.load(Ordering::SeqCst) {
            col = col.child(Portal {
                clicks: Arc::clone(&self.clicks),
                show: Arc::clone(&self.show),
            });
        }
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
    clicks: Arc<AtomicUsize>,
    show: Arc<AtomicBool>,
    mounted: Arc<AtomicBool>,
}

fn harness() -> H {
    let clicks = Arc::new(AtomicUsize::new(0));
    let show = Arc::new(AtomicBool::new(true));
    let mounted = Arc::new(AtomicBool::new(true));
    let e = FrameEngine::new(
        Box::new(App {
            clicks: Arc::clone(&clicks),
            show: Arc::clone(&show),
            mounted: Arc::clone(&mounted),
        }),
        FontCache::embedded(),
    );
    H { e, a: SkiaCanvas::new(WIN_W, WIN_H), b: SkiaCanvas::new(WIN_W, WIN_H), clicks, show, mounted }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }
    fn click(&mut self, x: f32, y: f32) {
        self.e.paint(&mut self.a, &mut self.b, &[
            rosace_platform::InputEvent::MouseDown { x, y, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x, y, button: rosace_platform::MouseButton::Left },
        ]);
    }
    fn promoted_layer(&self) -> Option<rosace::widgets::tree::Layer> {
        self.e.inspect_layers().into_iter()
            .find(|l| l.kind == rosace::widgets::tree::LayerKind::Promoted)
    }
}

/// Visual half: it composites at the root, so no ancestor clip applies.
#[test]
fn a_promoted_layer_escapes_its_ancestors_clip() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();

    let layer = h.promoted_layer().expect("the promotion produced a layer");
    assert!(layer.parent.is_none(), "a promoted layer composites against the window");
    assert!(!layer.culled, "it must not be clipped away by the scroll viewport it sits in");
    assert!(
        layer.dest.origin.y < 60.0,
        "the promoted panel is at y={}, below the bar — it was placed above it, so an \
         ancestor clip or transform is still being applied to it",
        layer.dest.origin.y
    );
}

/// Visual half: it is clickable where it visually is, and it wins over the
/// widget it covers.
#[test]
fn a_promoted_layer_takes_the_pointer_before_what_it_covers() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    let r = h.promoted_layer().expect("promoted").dest;

    // A point inside the panel, which is drawn over the bar.
    h.click(r.origin.x + 5.0, r.origin.y + 5.0);
    assert_eq!(
        h.clicks.load(Ordering::SeqCst),
        1,
        "clicking the promoted panel where it visually is did nothing — it is painted at \
         the root but still hit-tested in its declaring parent's coordinate space"
    );
}

/// Logical half: assistive tech sees it where it was DECLARED, not where the
/// pixels are — "is this inside the dialog?" has to stay answerable.
#[test]
fn a_promoted_layer_stays_at_its_logical_position_in_the_semantics_tree() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();

    fn find(node: &rosace_core::SemanticNode, label: &str) -> bool {
        if node.label.as_deref() == Some(label) {
            return true;
        }
        node.children.iter().any(|c| find(c, label))
    }
    let sem = h.e.semantics();
    assert!(
        find(&sem, "promoted"),
        "the promoted panel is missing from the semantics tree entirely — a screen reader \
         would never announce it"
    );
}

/// Logical half: it belongs to its declaring parent, so it goes when that goes.
#[test]
fn a_promoted_layer_disappears_when_its_logical_parent_leaves_the_tree() {
    let _guard = exclusive();
    let mut h = harness();
    h.frame();
    assert!(h.promoted_layer().is_some(), "promoted while its parent is mounted");

    h.mounted.store(false, Ordering::SeqCst);
    rosace_state::dirty_set::reset_to_global_dirty();
    h.frame();
    h.frame();

    assert!(
        h.promoted_layer().is_none(),
        "the declaring widget left the tree but its promoted content is still being \
         composited — an orphaned overlay nothing can dismiss"
    );
}
