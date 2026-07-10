//! D107 Phase 25 Step 4 — runtime shadow-DOM sync. Keeps the `#rsc-seo`
//! shadow root (Step 3's build-time `<template shadowrootmode="open">`, or
//! a fresh one attached on the fly if that step never ran — e.g. `rsc dev`)
//! up to date with the live semantic tree as app state changes AFTER
//! hydration, so a JS-executing crawler or assistive tech sees current
//! content, not just the build-time snapshot.
//!
//! Called from `rosace`'s `App::launch` closure (Layer 7 — has access to
//! `FrameEngine::semantics()`), only when `FrameEngine::paint()` reports
//! `content_changed` — this module does its own SECOND, finer check (a
//! plain string diff against the previous frame's HTML) before touching
//! the DOM at all, since a re-render can legitimately produce identical
//! output (e.g. state that changed and changed back). Two cheap gates:
//! the caller skips calling this at all on a clean frame; this skips the
//! DOM write on an unchanged-content frame. Neither does real per-element
//! diffing — deliberately: `set_inner_html` on a small text-only fragment
//! is fast, and building a real DOM diff engine is scope this phase
//! doesn't need (see `.steering/PHASE_25.md`'s Step 4 "mirror the render
//! tree's existing dirty-tracking rather than diff-from-scratch" — read as
//! "don't rebuild unconditionally every frame," which the two gates above
//! already satisfy).

use std::cell::RefCell;

use rosace_core::SemanticNode;

const HOST_ELEMENT_ID: &str = "rsc-seo";

thread_local! {
    static SHADOW_ROOT: RefCell<Option<web_sys::ShadowRoot>> = const { RefCell::new(None) };
    static PREV_HTML: RefCell<String> = RefCell::new(String::new());
}

/// Syncs the live shadow DOM to `tree`. No-op (cheaply) if the resulting
/// HTML is unchanged since the last call.
pub fn sync(tree: &SemanticNode) {
    let html = rosace_web_seo::render_html(tree);

    let changed = PREV_HTML.with(|prev| {
        let mut prev = prev.borrow_mut();
        if *prev == html {
            false
        } else {
            *prev = html.clone();
            true
        }
    });
    if !changed {
        return;
    }

    SHADOW_ROOT.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = find_or_attach_shadow_root();
        }
        if let Some(root) = cell.as_ref() {
            root.set_inner_html(&html);
        }
        // If there's still no shadow root (e.g. #rsc-seo isn't in this
        // page's HTML at all — an app that isn't using rsc new/rsc build's
        // scaffolding), silently do nothing rather than panic: this sync
        // is additive infrastructure, not something a missing host element
        // should crash the app over.
    });
}

/// Reuses Step 3's Declarative Shadow DOM root if the browser already
/// attached one from the build-time `<template shadowrootmode="open">`;
/// otherwise attaches a fresh one (e.g. `rsc dev`, which doesn't run the
/// build-time export at all).
fn find_or_attach_shadow_root() -> Option<web_sys::ShadowRoot> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let host = document.get_element_by_id(HOST_ELEMENT_ID)?;
    if let Some(existing) = host.shadow_root() {
        return Some(existing);
    }
    let init = web_sys::ShadowRootInit::new(web_sys::ShadowRootMode::Open);
    host.attach_shadow(&init).ok()
}
