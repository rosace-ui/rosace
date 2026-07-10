use std::sync::{Mutex, MutexGuard};
use std::collections::HashSet;
use rosace_trace::event::ComponentId;

static DIRTY: Mutex<Option<HashSet<ComponentId>>> = Mutex::new(None);

/// Mark the given components dirty for the next frame.
///
/// Called by `Atom::set()` for every subscriber when the atom's value changes.
/// The render loop reads this via `take_dirty_components()` once per frame.
pub fn mark_dirty(ids: &[ComponentId]) {
    if ids.is_empty() { return; }
    let mut guard: MutexGuard<Option<HashSet<ComponentId>>> = DIRTY.lock().unwrap();
    let set = guard.get_or_insert_with(HashSet::new);
    for &id in ids {
        set.insert(id);
    }
}

/// Check if `ALL` components should be rebuilt this frame (when no specific
/// dirty set is recorded — e.g. first frame, or full-refresh event).
pub fn is_global_dirty() -> bool {
    DIRTY.lock().unwrap().is_none()
}

/// Drain and return the current dirty set, replacing it with an empty set.
///
/// After this call `is_global_dirty()` returns `false` until the next call
/// to `reset_to_global_dirty()`. An empty returned set means "nothing is
/// dirty this frame" — new atom writes after this call will call `mark_dirty`
/// and populate the set for the NEXT frame.
pub fn take_dirty_components() -> HashSet<ComponentId> {
    let mut guard = DIRTY.lock().unwrap();
    match guard.as_mut() {
        Some(set) => {
            // Swap the set out, leaving an empty set in place.
            std::mem::take(set)
        }
        None => {
            // Globally dirty — transition to "clean" (empty set).
            *guard = Some(HashSet::new());
            HashSet::new()
        }
    }
}

/// Reset to the "globally dirty" state (rebuild everything next frame).
///
/// Called at startup or when the tree shape changes in a way that invalidates
/// the element cache (e.g. a component type mismatch during reconciliation).
pub fn reset_to_global_dirty() {
    *DIRTY.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_trace::event::ComponentId;

    #[test]
    fn starts_globally_dirty() {
        // The static is shared across tests; reset it to None first.
        reset_to_global_dirty();
        assert!(is_global_dirty());
    }

    #[test]
    fn mark_and_take() {
        reset_to_global_dirty();
        // Seed to "not globally dirty"
        let _ = take_dirty_components();
        mark_dirty(&[ComponentId(1), ComponentId(2)]);
        assert!(!is_global_dirty());
        let dirty = take_dirty_components();
        assert!(dirty.contains(&ComponentId(1)));
        assert!(dirty.contains(&ComponentId(2)));
        // After take, empty set → not globally dirty
        assert!(!is_global_dirty());
    }
}
