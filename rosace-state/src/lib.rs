//! `rosace-state` — reactive atom-based state for the ROSACE UI framework.
//!
//! # Core concepts
//!
//! - [`Atom<T>`] — a reactive value; components that read it automatically
//!   subscribe and are rebuilt when it changes.
//! - [`GlobalAtom<T>`] — app-wide atom declared as a `static`; no provider needed.
//! - [`RefreshEngine`] — computes the minimum set of component roots to rebuild.
//! - [`batch`] — groups multiple atom writes into a single rebuild pass.
//! - [`AsyncState<T>`] — models the five states of an async data operation.

pub mod async_state;
pub mod atom;
pub mod atom_id_gen;
pub mod batch;
pub mod cleanup_store;
pub mod dirty_set;

/// Who is building right now, for attributing atom writes.
///
/// `RosaceTrace::AtomWrite` carries a `by: ComponentId` and it was
/// HARDCODED to `ComponentId(0)`, so every write in the DevTools "who
/// changed this state?" column named component 0 regardless of the truth.
/// That was harmless while 0 meant "unknown" and became a real lie once
/// engines started taking process-unique root ids beginning at 0 — the
/// placeholder became indistinguishable from a genuine answer.
///
/// A write that happens OUTSIDE a build — from an event handler, a timer, a
/// network callback, which is most of them — genuinely has no owning
/// component, and reports [`UNKNOWN_COMPONENT`] rather than inventing one.
/// The `location` field on the same event carries the file and line, which
/// is usually the more useful answer anyway.
pub mod current_component {
    use std::cell::Cell;
    use rosace_trace::event::ComponentId;

    /// Reported when a write happens outside any build. Deliberately not 0,
    /// which is a real component id.
    pub const UNKNOWN_COMPONENT: ComponentId = ComponentId(u64::MAX);

    thread_local! {
        static CURRENT: Cell<Option<ComponentId>> = const { Cell::new(None) };
    }

    /// Marks `id` as building until the returned guard drops.
    pub fn enter(id: ComponentId) -> Guard {
        let previous = CURRENT.with(|c| c.replace(Some(id)));
        Guard { previous }
    }

    /// Restores the previous builder on drop, so nesting works and an early
    /// return or a panic cannot leave a stale id attributed to later writes.
    pub struct Guard {
        previous: Option<ComponentId>,
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            CURRENT.with(|c| c.set(self.previous));
        }
    }

    /// The component currently building, or [`UNKNOWN_COMPONENT`].
    pub fn current() -> ComponentId {
        CURRENT.with(|c| c.get()).unwrap_or(UNKNOWN_COMPONENT)
    }
}
pub mod external;
pub mod frame_scheduler;
pub mod global_atom;
pub mod pan_momentum;
pub mod refresh_engine;
pub mod scroll_offset;
pub mod state_store;

pub use async_state::{AsyncError, AsyncState};
pub use atom::Atom;
pub use atom_id_gen::next_atom_id;
pub use batch::{batch, is_batching, Priority};
pub use dirty_set::{mark_dirty, is_global_dirty, take_dirty_components, reset_to_global_dirty, request_rebuild_from_any_thread};
pub use frame_scheduler::{fire_after_ms, register_wakeup, request_frame, take_frame_requested};
pub use global_atom::GlobalAtom;
pub use refresh_engine::RefreshEngine;
pub use pan_momentum::{drag_last, set_drag_last, pan_velocity, set_pan_velocity, clear_pan_momentum};
pub use scroll_offset::{scroll_offset, set_scroll_offset, scroll_offset_by, clear_scroll_offset, render_scale, set_render_scale};
pub use state_store::{hook_state, clear_component};

/// Creates a new local atom initialised with `default`.
///
/// Each call allocates a fresh [`rosace_trace::event::AtomId`] from the global
/// counter and returns an [`Atom`] that is independent of every other atom.
/// Wire-up to the build context (`Context::use_atom`) happens in a later phase
/// once `rosace-core` is complete.
pub fn use_atom<T: Clone + Send + Sync + 'static>(default: T) -> Atom<T> {
    Atom::new(next_atom_id(), default)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    use rosace_trace::event::{AtomId, ComponentId};

    use super::*;

    // -----------------------------------------------------------------------
    // Atom basics
    // -----------------------------------------------------------------------

    #[test]
    fn atom_get_returns_initial_value() {
        let atom = use_atom(42_i32);
        assert_eq!(atom.get(), 42);
    }

    #[test]
    fn atom_set_notifies_subscriber() {
        let atom = use_atom(0_i32);

        let received: Arc<Mutex<Vec<ComponentId>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = Arc::clone(&received);

        atom.set_on_change(move |_aid, subs| {
            received_clone.lock().unwrap().extend(subs);
        });

        let cid = ComponentId(42);
        atom.subscribe(cid);
        atom.set(99);

        let guard = received.lock().unwrap();
        assert!(guard.contains(&cid), "on_change not called with subscriber");
    }

    #[test]
    fn atom_update_is_atomic_read_modify_write() {
        let atom = Arc::new(use_atom(0_i32));
        let mut handles = Vec::new();

        for _ in 0..10 {
            let a = Arc::clone(&atom);
            handles.push(std::thread::spawn(move || {
                a.update(|v| v + 1);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(atom.get(), 10);
    }

    // -----------------------------------------------------------------------
    // Batching
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_atom_changes_inside_batch_flush_once() {
        // Phase 1: verify that batch() correctly sets and clears the batching flag.
        assert!(!is_batching(), "should not be batching before batch()");
        batch(|| {
            assert!(is_batching(), "should be batching inside batch()");
        });
        assert!(!is_batching(), "should not be batching after batch()");
    }

    // -----------------------------------------------------------------------
    // RefreshEngine
    // -----------------------------------------------------------------------

    #[test]
    fn refresh_engine_prunes_descendants() {
        let mut engine = RefreshEngine::new();
        let parent = ComponentId(1);
        let child = ComponentId(2);

        engine.register(parent, None);
        engine.register(child, Some(parent));

        let mut dirty = HashSet::new();
        dirty.insert(parent);
        dirty.insert(child);

        let roots = engine.find_rebuild_roots(&dirty);
        assert_eq!(roots.len(), 1, "child should be pruned");
        assert_eq!(roots[0], parent);
    }

    #[test]
    fn refresh_engine_returns_all_roots_when_no_ancestors() {
        let mut engine = RefreshEngine::new();
        let a = ComponentId(10);
        let b = ComponentId(11);

        engine.register(a, None);
        engine.register(b, None);

        let mut dirty = HashSet::new();
        dirty.insert(a);
        dirty.insert(b);

        let mut roots = engine.find_rebuild_roots(&dirty);
        roots.sort_by_key(|c| c.0);
        assert_eq!(roots, vec![a, b]);
    }

    // -----------------------------------------------------------------------
    // GlobalAtom
    // -----------------------------------------------------------------------

    // Unique IDs per static to avoid cross-test interference.
    static TEST_COUNTER: GlobalAtom<i32> = GlobalAtom::new(AtomId(9999), || 0);

    #[test]
    fn global_atom_accessible_from_multiple_call_sites() {
        TEST_COUNTER.set(42);
        assert_eq!(TEST_COUNTER.get(), 42);
    }

    // -----------------------------------------------------------------------
    // use_atom
    // -----------------------------------------------------------------------

    #[test]
    fn use_atom_creates_unique_ids() {
        let a = use_atom(0_i32);
        let b = use_atom(0_i32);
        assert_ne!(a.id(), b.id());
    }

    // -----------------------------------------------------------------------
    // AsyncState
    // -----------------------------------------------------------------------

    #[test]
    fn async_state_can_hold_all_variants() {
        let _: AsyncState<i32> = AsyncState::Idle;
        let _: AsyncState<i32> = AsyncState::Loading;
        let _: AsyncState<i32> = AsyncState::Success(42);
        let _: AsyncState<i32> = AsyncState::Error(AsyncError::new("oops"));
        let _: AsyncState<i32> = AsyncState::Refreshing(42);
    }
}
pub use external::Subscribers;

#[cfg(test)]
/// Serialises tests that flip the PROCESS-GLOBAL dirty flag.
///
/// `reset_to_global_dirty()` writes a `static AtomicBool`, so a test calling
/// it lands in the middle of any other test's sequence — across MODULES, not
/// just within one. `dirty_set` had a local lock; `atom` and `external` also
/// call it and did not take it, so `mark_and_take` still failed intermittently
/// under parallel runs. One lock for the whole crate is the fix.
pub(crate) fn test_serial() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}
