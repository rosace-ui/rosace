//! `WillPopScope` — intercept a screen leaving, so unsaved work survives.
//!
//! Wrap a screen whose exit needs a question first:
//!
//! ```rust,ignore
//! WillPopScope::new(editor_body())
//!     .on_will_pop({
//!         let dirty = dirty.clone();
//!         let confirm = confirm_open.clone();
//!         move || {
//!             if !dirty.get() { return true; }   // nothing to lose, go
//!             confirm.set(true);                  // ask, and block this pop
//!             false
//!         }
//!     })
//! ```
//!
//! Return `true` to let the pop through, `false` to stop it. Blocking is not
//! the end of the story: the callback normally opens a dialog, and when the
//! user chooses "discard" the app clears whatever made it dirty and pops
//! again — this time the guard returns `true`. There is no "force pop" API
//! because there does not need to be; the guard reads app state, so changing
//! that state is what unblocks it.
//!
//! ## What it covers
//!
//! Everything, which is the point. The gate lives inside `ScreenNav::pop`,
//! so it applies to:
//!
//! * the system back button and back gesture (Android) and the left-edge
//!   swipe (iOS),
//! * `AppBar::back_button`, which calls `pop` like anything else,
//! * any `nav.pop()` the app makes itself.
//!
//! Guarding only the system intent would be worse than not guarding at all:
//! the same screen would protect your work or lose it depending on which
//! control you happened to press.
//!
//! ## Lifetime
//!
//! The guard registers during PAINT, and the registry is cleared only when
//! the widget tree is about to re-paint and repopulate it. So a scope applies
//! exactly while its screen is on screen — walk away and it stops applying,
//! with nothing to unregister and no way to leak a guard belonging to a
//! screen the user already left.
//!
//! The "only when re-painting" part is load-bearing. Clearing on EVERY frame
//! looks equivalent and is not: on a cache-hit frame the engine replays
//! cached pictures and no widget `paint` runs, so the guards would be wiped
//! and never re-registered — a screen would protect unsaved work only on
//! frames that happened to be dirty. That was a real bug during development,
//! caught by a test that popped straight through a guard meant to block it.
//!
//! ## Known edge
//!
//! During a screen transition both screens paint, so both guards register for
//! those frames, and blocking wins. A pop landing inside that window can be
//! blocked by the guard of the screen being left. Transitions are short and a
//! second pop mid-transition is not a real interaction, so this is recorded
//! rather than solved — solving it needs the transition to expose which
//! screen is incoming.

use std::sync::Arc;

use super::{BoxedWidget, Children, PaintCtx, Widget};

/// Intercepts this screen being popped. See the module docs.
pub struct WillPopScope {
    child: BoxedWidget,
    on_will_pop: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}

impl WillPopScope {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { child: Box::new(child), on_will_pop: None }
    }

    /// Return `true` to allow the pop, `false` to block it.
    pub fn on_will_pop(mut self, f: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        self.on_will_pop = Some(Arc::new(f));
        self
    }
}

impl Widget for WillPopScope {
    fn children(&self) -> Children<'_> { Children::One(&*self.child) }

    fn paint(&self, ctx: &mut PaintCtx) {
        if let Some(f) = &self.on_will_pop {
            rosace_core::nav_back::register_will_pop(f.clone());
        }
        let r = ctx.rect;
        self.child.paint(&mut ctx.child(r));
    }
    // layout: the protocol default delegates to the child — this adds no box.
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::nav_back::{clear_will_pop, has_will_pop, may_pop};
    use rosace_core::types::{Point, Rect, Size};
    use rosace_render::{FontCache, PictureRecorder};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::{cell::RefCell, rc::Rc};

    fn paint(w: &dyn Widget) {
        let font = FontCache::embedded();
        let mut rec = PictureRecorder::new();
        let mut ctx = PaintCtx::root(
            &mut rec,
            Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 100.0, height: 100.0 } },
            &font,
            rosace_theme::built_in::dark_theme(),
            Rc::new(RefCell::new(super::super::RenderTree::new())),
        );
        w.paint(&mut ctx);
    }

    #[test]
    fn a_guard_applies_only_while_its_screen_paints() {
        clear_will_pop();
        assert!(!has_will_pop(), "nothing registered yet");

        let scope = WillPopScope::new(super::super::Spacer::new(1.0)).on_will_pop(|| false);
        paint(&scope);
        assert!(has_will_pop(), "painting registers the guard");
        assert!(!may_pop(), "the guard blocks");

        // The next frame starts clean; a screen that stopped painting stops
        // guarding, with nothing to unregister.
        clear_will_pop();
        assert!(!has_will_pop());
        assert!(may_pop(), "no guard means no obstacle");
    }

    #[test]
    fn a_scope_without_a_callback_registers_nothing() {
        clear_will_pop();
        paint(&WillPopScope::new(super::super::Spacer::new(1.0)));
        assert!(!has_will_pop());
        clear_will_pop();
    }

    /// Blocking wins. A guard exists to protect something, so losing unsaved
    /// work because one of two scopes agreed is not a trade worth making.
    #[test]
    fn any_blocking_guard_blocks_the_pop() {
        clear_will_pop();
        paint(&WillPopScope::new(super::super::Spacer::new(1.0)).on_will_pop(|| true));
        paint(&WillPopScope::new(super::super::Spacer::new(1.0)).on_will_pop(|| false));
        assert!(!may_pop(), "one blocking guard is enough");
        clear_will_pop();
    }

    /// The guard typically opens a dialog, which sets an atom, which can
    /// rebuild and re-register while `may_pop` is still running.
    #[test]
    fn a_guard_may_register_another_while_running() {
        clear_will_pop();
        let ran = Arc::new(AtomicBool::new(false));
        let r = ran.clone();
        paint(&WillPopScope::new(super::super::Spacer::new(1.0)).on_will_pop(move || {
            rosace_core::nav_back::register_will_pop(Arc::new(|| true));
            r.store(true, Ordering::SeqCst);
            true
        }));
        assert!(may_pop(), "must not panic on a re-entrant borrow");
        assert!(ran.load(Ordering::SeqCst));
        clear_will_pop();
    }
}
