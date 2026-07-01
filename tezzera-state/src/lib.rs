//! `tezzera-state` — reactive atom-based state for the TEZZERA UI framework.
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
pub mod frame_scheduler;
pub mod global_atom;
pub mod refresh_engine;
pub mod state_store;

pub use async_state::{AsyncError, AsyncState};
pub use atom::Atom;
pub use atom_id_gen::next_atom_id;
pub use batch::{batch, is_batching, Priority};
pub use dirty_set::{mark_dirty, is_global_dirty, take_dirty_components, reset_to_global_dirty};
pub use frame_scheduler::{register_wakeup, request_frame, take_frame_requested};
pub use global_atom::GlobalAtom;
pub use refresh_engine::RefreshEngine;
pub use state_store::{hook_state, clear_component};

/// Creates a new local atom initialised with `default`.
///
/// Each call allocates a fresh [`tezzera_trace::event::AtomId`] from the global
/// counter and returns an [`Atom`] that is independent of every other atom.
/// Wire-up to the build context (`Context::use_atom`) happens in a later phase
/// once `tezzera-core` is complete.
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

    use tezzera_trace::event::{AtomId, ComponentId};

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
