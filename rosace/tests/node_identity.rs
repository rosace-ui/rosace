//! Nested nodes must know WHAT is in them, end to end.
//!
//! `walk_element` has always recorded a type tag at element boundaries. Nodes
//! created inside a component's widget tree never did — `PaintCtx::child`
//! wrote only `cached_rect` — so every node below the first native element
//! carried `tag == ""` and could not tell a `Button` from a `TextField`.
//!
//! These tests drive the real engine rather than `RenderTree` directly,
//! because the unit tests in `render_tree.rs` prove `adopt_tag` resets
//! correctly and prove nothing about whether anyone CALLS it. That gap is the
//! whole bug: the mechanism existed at one layer and was simply absent at the
//! other.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Alpha;
impl Widget for Alpha {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 20.0, height: 20.0 } }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(200, 0, 0)); }
}
struct Beta;
impl Widget for Beta {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 20.0, height: 20.0 } }
    fn paint(&self, ctx: &mut PaintCtx) { ctx.fill_rect(ctx.rect, Color::rgb(0, 0, 200)); }
}

/// One child either way — the shape where the slot is REUSED and
/// `finalize`'s truncate never fires.
struct App(Arc<AtomicBool>);
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> Element {
        let col = Column::new();
        if self.0.load(Ordering::SeqCst) {
            col.child(Alpha).into_element()
        } else {
            col.child(Beta).into_element()
        }
    }
}

fn tags_in_tree(engine: &FrameEngine) -> Vec<&'static str> {
    engine.inspect_tree().iter().map(|n| n.tag).collect()
}


/// `dirty_set`'s global-dirty flag is PROCESS-wide, so a test that forces a
/// rebuild makes any concurrently-running test's frame structural. Serialise
/// the tests in this binary that depend on frame classification.
static FRAME_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn a_nested_widget_node_records_its_type() {
    let _guard = exclusive();
    let flag = Arc::new(AtomicBool::new(true));
    let mut e = FrameEngine::new(Box::new(App(flag.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(200, 200), SkiaCanvas::new(200, 200));
    e.paint(&mut a, &mut b, &[]);

    let tags = tags_in_tree(&e);
    assert!(
        tags.iter().any(|t| t.ends_with("::Alpha")),
        "the child widget's own node has no type recorded; tags were {tags:?}"
    );
}

/// The branch swap. Before `paint_child`, the node kept `tag == \"\"` across
/// both branches, so nothing could tell that the occupant had changed.
#[test]
fn swapping_the_widget_type_in_a_slot_is_visible_to_the_tree() {
    let _guard = exclusive();
    let flag = Arc::new(AtomicBool::new(true));
    let mut e = FrameEngine::new(Box::new(App(flag.clone())), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(200, 200), SkiaCanvas::new(200, 200));

    e.paint(&mut a, &mut b, &[]);
    let before = tags_in_tree(&e);
    assert!(before.iter().any(|t| t.ends_with("::Alpha")), "{before:?}");

    // Flip the branch and force a rebuild.
    flag.store(false, Ordering::SeqCst);
    rosace_state::dirty_set::reset_to_global_dirty();
    e.paint(&mut a, &mut b, &[]);

    let after = tags_in_tree(&e);
    assert!(
        after.iter().any(|t| t.ends_with("::Beta")),
        "the slot still reports the old widget type after the branch flipped: {after:?}"
    );
    assert!(
        !after.iter().any(|t| t.ends_with("::Alpha")),
        "the replaced widget's type is still present: {after:?}"
    );
}
