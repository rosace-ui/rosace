//! `ctx.widget_state()` — node-owned state, the framework's `remember`.
//!
//! Compose's model rather than Flutter's: the declaration site IS the creation
//! site, identity is positional, and the storage lives on the persistent
//! structure (the arena node) rather than on the widget object, which is
//! rebuilt and discarded constantly.
//!
//! Written to FAIL before the method exists.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// `dirty_set`'s global-dirty flag is process-wide; serialise as the other
/// frame-classification tests do.
static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Holds a counter in NODE state. The widget object itself is rebuilt on every
/// structural frame and carries nothing.
struct Counter {
    /// What the widget saw in its own state on its last paint.
    observed: Arc<AtomicUsize>,
}

impl Widget for Counter {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 80.0, height: 20.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        let n = ctx.widget_state(|| 0usize);
        self.observed.store(n.get(), Ordering::SeqCst);
        ctx.fill_rect(ctx.rect, Color::rgb(1, 2, 3));
        let h = n.clone();
        ctx.register_hit(Arc::new(move || h.update(|v| *v += 1)));
    }
}

/// A different widget type, to prove a slot type change discards the state.
struct Other;
impl Widget for Other {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 80.0, height: 20.0 } }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(9, 9, 9)); }
}

struct App {
    observed: Arc<AtomicUsize>,
    show_counter: Arc<AtomicBool>,
}

impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let col = Column::new();
        if self.show_counter.load(Ordering::SeqCst) {
            col.child(Counter { observed: self.observed.clone() }).boxed()
        } else {
            col.child(Other).boxed()
        }
    }
}

struct H {
    e: FrameEngine,
    a: SkiaCanvas,
    b: SkiaCanvas,
    observed: Arc<AtomicUsize>,
    show_counter: Arc<AtomicBool>,
}

fn harness() -> H {
    let observed = Arc::new(AtomicUsize::new(0));
    let show_counter = Arc::new(AtomicBool::new(true));
    let e = FrameEngine::new(
        Box::new(App { observed: observed.clone(), show_counter: show_counter.clone() }),
        FontCache::embedded(),
    );
    H {
        e,
        a: SkiaCanvas::new(300, 200),
        b: SkiaCanvas::new(300, 200),
        observed,
        show_counter,
    }
}

impl H {
    fn frame(&mut self) { self.e.paint(&mut self.a, &mut self.b, &[]); }

    fn click_counter(&mut self) {
        let r = self.e.inspect_tree().iter()
            .find(|n| n.tag.ends_with("::Counter"))
            .and_then(|n| n.rect)
            .expect("the counter painted");
        let (x, y) = (r.origin.x + 2.0, r.origin.y + 2.0);
        self.e.paint(&mut self.a, &mut self.b, &[
            rosace_platform::InputEvent::MouseDown { x, y, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x, y, button: rosace_platform::MouseButton::Left },
        ]);
    }
}

/// The core promise: state lives on the node, mutating it marks that node, and
/// the next frame sees the new value — with no `Atom` and nothing owned by the
/// app above the widget.
#[test]
fn state_survives_repaints_and_updating_it_refreshes_the_widget() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    assert_eq!(h.observed.load(Ordering::SeqCst), 0, "init runs once and yields 0");

    h.frame();
    assert_eq!(h.observed.load(Ordering::SeqCst), 0,
        "an idle frame must not re-run the initialiser");

    h.click_counter();
    h.frame();
    assert_eq!(h.observed.load(Ordering::SeqCst), 1,
        "updating node state must mark the node and be visible on the next paint");
}

/// State must survive a structural rebuild. This is the whole reason it lives
/// on the node rather than on the widget: the widget object is thrown away and
/// reconstructed on every rebuild.
#[test]
fn state_survives_a_structural_rebuild() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    h.click_counter();
    h.frame();
    assert_eq!(h.observed.load(Ordering::SeqCst), 1);

    // Force a full rebuild — every widget object is now new.
    rosace_state::dirty_set::reset_to_global_dirty();
    h.frame();

    assert_eq!(h.observed.load(Ordering::SeqCst), 1,
        "a rebuild discarded node state — it must live on the node, not the widget");
}

/// A slot that changes widget TYPE must not inherit the previous occupant's
/// state. `adopt_tag` already enforces this for every other kind of node state
/// (edit buffers, scroll positions); `widget_state` must be no different.
#[test]
fn changing_the_widget_type_in_a_slot_discards_the_state() {
    let _guard = exclusive();
    let mut h = harness();

    h.frame();
    h.click_counter();
    h.frame();
    assert_eq!(h.observed.load(Ordering::SeqCst), 1);

    // Counter -> Other
    h.show_counter.store(false, Ordering::SeqCst);
    rosace_state::dirty_set::reset_to_global_dirty();
    h.frame();

    // Other -> Counter: a fresh occupant of that slot starts from the initialiser.
    h.show_counter.store(true, Ordering::SeqCst);
    rosace_state::dirty_set::reset_to_global_dirty();
    h.frame();

    assert_eq!(h.observed.load(Ordering::SeqCst), 0,
        "the returning Counter inherited state left by a different widget type");
}
