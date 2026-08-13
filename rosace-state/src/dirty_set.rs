use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use rosace_trace::event::ComponentId;

thread_local! {
    /// Components to rebuild next frame, for THIS thread's engine.
    ///
    /// Thread-local, not process-global, and that is the whole point. It used
    /// to be a `static Mutex<Option<HashSet<..>>>` shared by every engine in
    /// the process, while the state store it partners with
    /// (`state_store::STORE`) was already thread-local. That asymmetry was a
    /// real bug, not a style inconsistency:
    ///
    ///   * `take_dirty_components()` DRAINS the set. With two engines alive,
    ///     whichever drained first consumed the other's marks, and that
    ///     engine silently skipped its rebuild.
    ///   * `FrameEngine::new()` calls `reset_to_global_dirty()`, so merely
    ///     CONSTRUCTING a second engine wiped the first one's pending work.
    ///
    /// The visible symptom was dropped keystrokes: type "hello world" and get
    /// "hello worl" — the frame that would have applied the last character
    /// never rebuilt. It showed up as engine tests failing about one run in
    /// three, a different test each time, because the suite runs tests in
    /// parallel and each test builds its own engine (WIDGET_FINDINGS L15).
    ///
    /// A real app has one engine on one UI thread, so nothing changes there.
    static DIRTY: RefCell<Option<HashSet<ComponentId>>> = const { RefCell::new(None) };
}

/// Cross-thread "rebuild everything" request.
///
/// [`reset_to_global_dirty`] can legitimately be called from a thread that
/// owns no engine — a platform callback pushing an OS setting change
/// (`set_media_query` on an iOS/Android configuration change). Marking only
/// that thread's `DIRTY` would drop the request on the floor, so the intent
/// is also recorded here where the UI thread can see it.
///
/// Erring toward an EXTRA rebuild is safe; missing one loses user input.
static FORCE_GLOBAL: AtomicBool = AtomicBool::new(true);

/// Mark the given components dirty for the next frame.
///
/// Called by `Atom::set()` for every subscriber when the atom's value changes.
/// The render loop reads this via `take_dirty_components()` once per frame.
pub fn mark_dirty(ids: &[ComponentId]) {
    if ids.is_empty() { return; }
    DIRTY.with(|d| {
        let mut guard = d.borrow_mut();
        let set = guard.get_or_insert_with(HashSet::new);
        for &id in ids {
            set.insert(id);
        }
    });
}

/// Check if `ALL` components should be rebuilt this frame (when no specific
/// dirty set is recorded — e.g. first frame, or full-refresh event).
pub fn is_global_dirty() -> bool {
    DIRTY.with(|d| d.borrow().is_none()) || FORCE_GLOBAL.load(Ordering::Relaxed)
}

/// Drain and return the current dirty set, replacing it with an empty set.
///
/// After this call `is_global_dirty()` returns `false` until the next call
/// to `reset_to_global_dirty()`. An empty returned set means "nothing is
/// dirty this frame" — new atom writes after this call will call `mark_dirty`
/// and populate the set for the NEXT frame.
pub fn take_dirty_components() -> HashSet<ComponentId> {
    FORCE_GLOBAL.store(false, Ordering::Relaxed);
    DIRTY.with(|d| {
        let mut guard = d.borrow_mut();
        match guard.as_mut() {
            Some(set) => std::mem::take(set),
            None => {
                *guard = Some(HashSet::new());
                HashSet::new()
            }
        }
    })
}

/// Reset to the "globally dirty" state (rebuild everything next frame).
///
/// Called at startup or when the tree shape changes in a way that invalidates
/// the element cache (e.g. a component type mismatch during reconciliation).
pub fn reset_to_global_dirty() {
    DIRTY.with(|d| *d.borrow_mut() = None);
    FORCE_GLOBAL.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_trace::event::ComponentId;

    /// Both tests below drive the same process-global dirty set, so they
    /// must not interleave. Resetting it at the top of each is not enough:
    /// the suite runs in parallel, so one test's `reset` lands in the middle
    /// of the other's sequence. This was a real intermittent CI failure.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn starts_globally_dirty() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset_to_global_dirty();
        assert!(is_global_dirty());
    }

    #[test]
    fn mark_and_take() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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
