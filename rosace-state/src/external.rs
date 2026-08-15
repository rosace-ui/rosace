//! Drive ROSACE rebuilds from a reactive source ROSACE knows nothing about.
//!
//! `Atom` is not the only way to hold state, and it was never meant to be the
//! only way to *trigger* a rebuild. A BLoC, a signal library, a Redux-style
//! store, an ECS world, an mpsc channel — anything with "the value changed"
//! semantics can drive the UI, and none of it needs to be an `Atom`.
//!
//! This module is the seam. It is deliberately small: three things, which
//! together are everything the built-in `Atom` uses.
//!
//! # What a rebuild actually needs
//!
//! 1. **Who is watching.** During `build`, a component has an id
//!    ([`rosace_core::Context::component_id`]). Record it while the component
//!    is reading you.
//! 2. **Tell them it changed.** [`Subscribers::notify`] marks those
//!    components dirty and wakes the frame loop.
//! 3. **Stop when they go away.** `Context::on_cleanup` fires on unmount;
//!    call [`Subscribers::remove`] there, or the list grows forever and you
//!    dirty components that no longer exist.
//!
//! # A BLoC builder, end to end
//!
//! ```rust,ignore
//! use rosace_state::external::Subscribers;
//!
//! pub struct Gloc<S> {
//!     state: Mutex<S>,
//!     watchers: Subscribers,   // <- the whole integration
//! }
//!
//! impl<S: Clone> Gloc<S> {
//!     /// Called from an event handler, a stream, a worker thread — anywhere.
//!     pub fn emit(&self, next: S) {
//!         *self.state.lock().unwrap() = next;
//!         self.watchers.notify();          // rebuilds every watching component
//!     }
//! }
//!
//! /// The hook. Read state AND subscribe in one call, the way `ctx.state`
//! /// does — a reader that forgets to subscribe is the classic bug here.
//! pub fn use_gloc<S: Clone>(ctx: &mut Context, gloc: &Arc<Gloc<S>>) -> S {
//!     gloc.watchers.add(ctx.component_id());
//!     let g = gloc.clone();
//!     let id = ctx.component_id();
//!     ctx.on_cleanup(move || g.watchers.remove(id));
//!     gloc.state.lock().unwrap().clone()
//! }
//!
//! /// Optional sugar, so it reads like the Flutter original.
//! pub fn gloc_builder<S: Clone>(
//!     ctx: &mut Context,
//!     gloc: &Arc<Gloc<S>>,
//!     build: impl FnOnce(S) -> BoxedWidget,
//! ) -> BoxedWidget {
//!     build(use_gloc(ctx, gloc))
//! }
//! ```
//!
//! That is the entire integration. No trait to implement, no registration
//! with the engine, no widget subclass.
//!
//! # Threads
//!
//! [`Subscribers::notify`] is safe to call from any thread. It records which
//! thread the subscriber list belongs to (the one that called
//! [`Subscribers::add`], which is the UI thread, since that is where `build`
//! runs) and routes the marks there.
//!
//! This matters more than it looks. Marking dirty on the *calling* thread
//! means a worker finishing an async job dirties a set no engine ever reads:
//! the UI thread wakes, finds nothing dirty, and never rebuilds. That exact
//! bug existed in `Atom` for a few hours during development and is the reason
//! this routing is built in here rather than left to each plugin.
//!
//! # What this does NOT give you
//!
//! Rebuild granularity is per COMPONENT, not per widget. Notifying rebuilds
//! the whole `build()` of every subscriber. That is the same granularity
//! `Atom` has, so a plugin is not at a disadvantage — but a "rebuild just
//! this subtree" boundary does not exist for anyone yet.

use std::sync::Mutex;
use std::thread::ThreadId;

use rosace_trace::event::ComponentId;

/// A subscriber list a foreign reactive source can embed.
///
/// Cheap to construct, safe to share, and safe to notify from any thread.
#[derive(Debug, Default)]
pub struct Subscribers {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    ids: Vec<ComponentId>,
    /// The thread `add` was last called on — where `build` runs, and so
    /// where these components must be marked dirty.
    owner: Option<ThreadId>,
}

impl Subscribers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a component as watching. Call during `build`, every build —
    /// it is idempotent, and re-adding is what keeps the list current when a
    /// component stops reading you.
    pub fn add(&self, id: ComponentId) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.owner = Some(std::thread::current().id());
        if !g.ids.contains(&id) {
            g.ids.push(id);
        }
    }

    /// Stop watching. Call from `Context::on_cleanup`; without it the list
    /// grows for the life of the app and dirties components that are gone.
    pub fn remove(&self, id: ComponentId) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.ids.retain(|&x| x != id);
    }

    /// Rebuild every watching component on the next frame, and wake the loop.
    ///
    /// Safe from any thread — see the module docs on threading.
    pub fn notify(&self) {
        let (ids, owner) = {
            let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            (g.ids.clone(), g.owner)
        };
        if ids.is_empty() {
            return;
        }
        match owner {
            Some(t) => crate::dirty_set::mark_dirty_for_thread(t, &ids),
            // Nobody has subscribed from a UI thread yet, so there is no
            // engine to address. Marking locally is the best available guess
            // and is what a same-thread caller would expect.
            None => crate::dirty_set::mark_dirty(&ids),
        }
        crate::frame_scheduler::request_frame();
    }

    /// How many components are watching. For diagnostics and tests — a count
    /// that only grows is the signature of a missing `remove`.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_is_idempotent_and_removing_takes_effect() {
        let s = Subscribers::new();
        s.add(ComponentId(1));
        s.add(ComponentId(1)); // every build re-adds
        s.add(ComponentId(2));
        assert_eq!(s.len(), 2, "re-adding must not duplicate");

        s.remove(ComponentId(1));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn notify_marks_every_watcher_dirty() {
        let _serial = crate::test_serial();
        crate::dirty_set::reset_to_global_dirty();
        let _ = crate::dirty_set::take_dirty_components();

        let s = Subscribers::new();
        s.add(ComponentId(7));
        s.add(ComponentId(9));
        s.notify();

        let dirty = crate::dirty_set::take_dirty_components();
        assert!(dirty.contains(&ComponentId(7)));
        assert!(dirty.contains(&ComponentId(9)));
    }

    /// The case a plugin author will hit first and debug last: emitting from
    /// a worker thread. The marks must land on the thread that subscribed.
    #[test]
    fn notify_from_another_thread_still_reaches_the_subscribing_thread() {
        let _serial = crate::test_serial();
        crate::dirty_set::reset_to_global_dirty();
        let _ = crate::dirty_set::take_dirty_components();

        let s = std::sync::Arc::new(Subscribers::new());
        s.add(ComponentId(11)); // subscribed on THIS thread

        let s2 = s.clone();
        std::thread::spawn(move || s2.notify()).join().unwrap();

        let dirty = crate::dirty_set::take_dirty_components();
        assert!(dirty.contains(&ComponentId(11)),
            "an off-thread emit never reached the UI thread; got {dirty:?}");
    }

    #[test]
    fn notifying_with_no_watchers_is_a_no_op() {
        let s = Subscribers::new();
        s.notify(); // must not panic or wake anything
        assert!(s.is_empty());
    }
}
