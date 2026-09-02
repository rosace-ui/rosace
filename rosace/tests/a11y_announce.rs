//! `a11y::announce` puts a spoken message into the semantic tree.
//!
//! The semantic tree describes what is on screen. Some things a user needs to
//! hear are not on screen, or are not where they are looking: "Copied to
//! clipboard", "3 results", "Upload failed". Before this there was no way to
//! make a screen reader say anything except by drawing a widget and hoping
//! the reader's cursor was on it.
//!
//! An announcement is delivered as a LIVE REGION child of the root:
//! assistive technology speaks a live region when it appears. The macOS,
//! Windows and Linux bridges all consume that through AccessKit's `Live`, so
//! one call reaches VoiceOver, Narrator and Orca.
//!
//! These assert on the tree the platform bridge is handed, which is the last
//! point still under our control — what VoiceOver does with a correct
//! `Live::Polite` node is AccessKit's business and the OS's.

use rosace::a11y::{announce, Politeness};
use rosace::prelude::*;
use rosace::FrameEngine;
use rosace_core::SemanticNode;
use rosace_render::{FontCache, SkiaCanvas};
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());
fn exclusive() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

struct App;
impl Component for App {
    fn build(&self, _c: &mut Context) -> BoxedWidget {
        Column::new().child(Text::new("hello")).boxed()
    }
}

fn harness() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
    (
        FrameEngine::new(Box::new(App), FontCache::embedded()),
        SkiaCanvas::new(200, 200),
        SkiaCanvas::new(200, 200),
    )
}

/// Every live-region node in the tree, as (label, politeness).
fn spoken(root: &SemanticNode) -> Vec<(String, Politeness)> {
    fn walk(n: &SemanticNode, out: &mut Vec<(String, Politeness)>) {
        if let Some(p) = n.live {
            out.push((n.label.clone().unwrap_or_default(), p));
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

#[test]
fn an_announcement_reaches_the_semantic_tree_as_a_live_region() {
    let _g = exclusive();
    let (mut e, mut a, mut b) = harness();
    e.paint(&mut a, &mut b, &[]);
    let _ = e.semantics(); // drain anything left by another test

    announce("Copied to clipboard", Politeness::Polite);
    let tree = e.semantics();

    assert_eq!(
        spoken(&tree),
        vec![("Copied to clipboard".to_string(), Politeness::Polite)],
        "the announcement should appear as a live region in the tree"
    );
}

/// Spoken once. Left queued, every past announcement would be re-spoken on
/// every later publish — and the tree is republished on any frame that
/// changed something, so that is a lot of repeats.
#[test]
fn an_announcement_is_delivered_exactly_once() {
    let _g = exclusive();
    let (mut e, mut a, mut b) = harness();
    e.paint(&mut a, &mut b, &[]);
    let _ = e.semantics();

    announce("Message sent", Politeness::Polite);
    let first = e.semantics();
    assert_eq!(spoken(&first).len(), 1, "delivered on the next publish");

    let second = e.semantics();
    assert!(
        spoken(&second).is_empty(),
        "still queued on the following publish — it would be spoken again \
         every time the tree is republished"
    );
}

#[test]
fn politeness_is_carried_through() {
    let _g = exclusive();
    let (mut e, mut a, mut b) = harness();
    e.paint(&mut a, &mut b, &[]);
    let _ = e.semantics();

    announce("Upload failed", Politeness::Assertive);
    assert_eq!(
        spoken(&e.semantics()),
        vec![("Upload failed".to_string(), Politeness::Assertive)],
        "Assertive must not be quietly downgraded — it is the difference \
         between interrupting and waiting"
    );
}

/// An empty message would publish a live region with nothing in it, which
/// some readers announce as a meaningless blip.
#[test]
fn empty_announcements_are_dropped() {
    let _g = exclusive();
    let (mut e, mut a, mut b) = harness();
    e.paint(&mut a, &mut b, &[]);
    let _ = e.semantics();

    announce("", Politeness::Polite);
    announce("   ", Politeness::Polite);
    assert!(spoken(&e.semantics()).is_empty(), "blank announcements are dropped");
}

/// Ordinary widgets must NOT be live regions — everything on screen would be
/// read aloud on every repaint.
#[test]
fn ordinary_nodes_are_not_live_regions() {
    let _g = exclusive();
    let (mut e, mut a, mut b) = harness();
    e.paint(&mut a, &mut b, &[]);
    let _ = e.semantics();
    assert!(
        spoken(&e.semantics()).is_empty(),
        "a plain tree should contain no live regions at all"
    );
}
