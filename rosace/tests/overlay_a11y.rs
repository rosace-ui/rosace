//! An open overlay must be visible to assistive technology.
//!
//! It was not. `FrameEngine::semantics()` walks `self.render_tree`, and overlay
//! content lived in `overlay_trees` — a parallel map of retained render trees
//! painted in a separate pass. So a screen reader on an open dialog read the
//! page BEHIND it and never announced the dialog at all, and the content behind
//! a modal was never hidden from assistive tech either.
//!
//! Once an overlay is a promoted node it is simply in the tree, and the
//! semantics walk finds it with no overlay-specific code — which is the point
//! of folding the third compositing mechanism back into the first.

use rosace::prelude::*;
use rosace::widgets::tree::{LayoutCtx, OverlayApi, PaintCtx};
use rosace::FrameEngine;
use rosace_render::{FontCache, SkiaCanvas};
use rosace_state::Atom;
use std::sync::{Arc, Mutex, MutexGuard};

static FRAME_STATE: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    FRAME_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Overlay body carrying a label an assistive-tech user would need to hear.
struct Body;
impl Widget for Body {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 160.0, height: 80.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(30, 30, 40));
        ctx.semantics(
            rosace::widgets::tree::SemanticsProps::new(rosace_core::Role::Button)
                .label("Delete everything"),
        );
    }
}

struct App {
    open: Atom<bool>,
}
impl Component for App {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        Column::new()
            .child(
                Container::new()
                    .width(100.0)
                    .height(40.0)
                    .dialog(self.open.get(), || Body.boxed()),
            )
            .boxed()
    }
}

fn labels(node: &rosace_core::SemanticNode, out: &mut Vec<String>) {
    if let Some(l) = &node.label {
        out.push(l.clone());
    }
    for c in &node.children {
        labels(c, out);
    }
}

/// Paint an app whose dialog starts `open`, and collect every semantic label.
///
/// The dialog's open state is a standalone atom nothing subscribes to, so
/// flipping it mid-run would dirty no component and skip the frame. Starting
/// from the state under test keeps the test about semantics rather than about
/// invalidation.
fn labels_with_dialog(open: bool) -> Vec<String> {
    let atom = Atom::new(rosace_state::next_atom_id(), open);
    let mut e = FrameEngine::new(Box::new(App { open: atom }), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(300, 400), SkiaCanvas::new(300, 400));
    e.paint(&mut a, &mut b, &[]);
    e.paint(&mut a, &mut b, &[]);

    let mut out = Vec::new();
    labels(&e.semantics(), &mut out);
    out
}

#[test]
fn a_closed_dialog_is_not_in_the_semantics_tree() {
    let _guard = exclusive();
    let seen = labels_with_dialog(false);
    assert!(
        !seen.iter().any(|l| l == "Delete everything"),
        "the dialog is closed but its label is in the semantics tree: {seen:?}"
    );
}

#[test]
fn an_open_dialog_is_announced_to_assistive_tech() {
    let _guard = exclusive();
    let seen = labels_with_dialog(true);
    assert!(
        seen.iter().any(|l| l == "Delete everything"),
        "an OPEN dialog is missing from the semantics tree — a screen reader would read the \
         page behind it and never announce the dialog. Saw {seen:?}"
    );
}

// ── Keyboard focus ───────────────────────────────────────────────────────────

/// A modal must TRAP Tab focus.
///
/// `FocusBehavior::Trap` was set by every overlay widget since overlays
/// existed and read by nothing, so Tab walked straight out of a dialog into
/// the page it was covering — and until the overlay migration, overlay content
/// was not in the Tab cycle at all.
struct Focusable(&'static str);
impl Widget for Focusable {
    fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 80.0, height: 24.0 } }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_rect(ctx.rect, Color::rgb(60, 60, 80));
        let node = ctx.focus_node();
        ctx.register_focus(node);
    }
}

struct FocusApp {
    open: Atom<bool>,
}
impl Component for FocusApp {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        Column::new()
            .child(Focusable("page-a"))
            .child(Focusable("page-b"))
            .child(
                Container::new()
                    .width(60.0)
                    .height(20.0)
                    .dialog(self.open.get(), || {
                        Arc::new(Column::new().child(Focusable("dialog-a")))
                    }),
            )
            .boxed()
    }
}

fn focus_count(open: bool) -> usize {
    let atom = Atom::new(rosace_state::next_atom_id(), open);
    let mut e = FrameEngine::new(Box::new(FocusApp { open: atom }), FontCache::embedded());
    let (mut a, mut b) = (SkiaCanvas::new(300, 400), SkiaCanvas::new(300, 400));
    e.paint(&mut a, &mut b, &[]);
    e.paint(&mut a, &mut b, &[]);
    e.focus_node_count()
}

#[test]
fn an_open_modal_traps_tab_focus_to_itself() {
    let _guard = exclusive();
    let closed = focus_count(false);
    assert!(closed >= 2, "the page's own controls should be focusable, got {closed}");

    let open = focus_count(true);
    assert!(
        open < closed,
        "with a modal open the Tab cycle still contains {open} nodes (page had {closed}) — \
         focus can walk out of the dialog into the page behind it"
    );
}
