use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread::ThreadId;
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

/// Marks that arrived from ANOTHER thread, addressed to the thread that owns
/// the atom.
///
/// A thread-local dirty set alone is wrong in the other direction: a
/// background worker finishing an HTTP request writes the result atom from
/// ITS thread, and `mark_dirty` would then dirty a set no engine ever reads.
/// The UI thread wakes on `request_frame`, finds nothing dirty, and never
/// rebuilds — every async hook (`use_query`, and anything spawning a thread)
/// silently stops updating. That regression was real and shipped for exactly
/// as long as it took to write a test for it.
///
/// Keyed by the OWNING thread rather than broadcast, so two engines on two
/// threads still cannot consume each other's marks — which is the isolation
/// the thread-local gave us in the first place.
static INBOX: Mutex<Option<HashMap<ThreadId, Vec<ComponentId>>>> = Mutex::new(None);

thread_local! {
    /// Which atom most recently dirtied each component.
    ///
    /// A side channel rather than a change to the dirty set's own type, so
    /// nothing on the hot path pays for it. Exists purely so the tracing
    /// layer can answer "what made this rebuild?" with the atom's identity
    /// instead of a shrug — `RebuildCause::AtomChanged(id)` needs the id,
    /// and `mark_dirty` was throwing it away at the one point that knew it.
    ///
    /// Debug-only: `trace!` compiles out in release, so recording causes
    /// there would be pure cost.
    static LAST_CAUSE: RefCell<HashMap<ComponentId, rosace_trace::event::AtomId>> =
        RefCell::new(HashMap::new());
}

/// Record that `atom` dirtied `ids`, then mark them.
///
/// Callers that know the source should prefer this over [`mark_dirty`]; the
/// extra argument is what makes a rebuild traceable back to a write.
pub fn mark_dirty_from(atom: rosace_trace::event::AtomId, ids: &[ComponentId]) {
    #[cfg(debug_assertions)]
    LAST_CAUSE.with(|m| {
        let mut m = m.borrow_mut();
        for &id in ids {
            m.insert(id, atom);
        }
    });
    let _ = atom;
    mark_dirty(ids);
}

/// The atom that most recently dirtied `id`, if one did.
///
/// Consumed by the tracing layer; always `None` in release.
pub fn last_cause(id: ComponentId) -> Option<rosace_trace::event::AtomId> {
    #[cfg(debug_assertions)]
    { return LAST_CAUSE.with(|m| m.borrow().get(&id).copied()); }
    #[allow(unreachable_code)]
    { let _ = id; None }
}

/// Mark components dirty on behalf of `owner`, from a different thread.
///
/// Callers should use [`mark_dirty`] and let `Atom` decide; this is the
/// explicit route for a foreign reactive source (a BLoC, a stream adapter)
/// that knows which thread its subscribers live on.
pub fn mark_dirty_for_thread(owner: ThreadId, ids: &[ComponentId]) {
    if ids.is_empty() { return; }
    if owner == std::thread::current().id() {
        mark_dirty(ids);
        return;
    }
    if let Ok(mut guard) = INBOX.lock() {
        guard.get_or_insert_with(HashMap::new)
            .entry(owner)
            .or_default()
            .extend_from_slice(ids);
    }
    // Wake the owning loop; without this the marks sit until something else
    // happens to request a frame.
    crate::frame_scheduler::request_frame();
}

/// Mark each subscriber on the thread it subscribed from.
///
/// `threads[i]` is where `ids[i]` lives. Grouping matters: a `GlobalAtom`
/// can legitimately have subscribers on several threads, and marking them
/// all on one would leave the rest never rebuilding.
pub fn mark_dirty_per_subscriber_from(
    atom: rosace_trace::event::AtomId,
    ids: &[ComponentId],
    threads: &[ThreadId],
) {
    #[cfg(debug_assertions)]
    LAST_CAUSE.with(|m| {
        let mut m = m.borrow_mut();
        for &id in ids { m.insert(id, atom); }
    });
    let _ = atom;
    mark_dirty_per_subscriber(ids, threads);
}

pub fn mark_dirty_per_subscriber(ids: &[ComponentId], threads: &[ThreadId]) {
    let here = std::thread::current().id();
    let mut local: Vec<ComponentId> = Vec::new();
    for (i, &id) in ids.iter().enumerate() {
        match threads.get(i) {
            Some(&t) if t != here => mark_dirty_for_thread(t, &[id]),
            // No recorded thread (a subscriber added before this bookkeeping
            // existed) is treated as local, which is the old behaviour.
            _ => local.push(id),
        }
    }
    if !local.is_empty() {
        mark_dirty(&local);
    }
}

/// Move this thread's inbox into its local set.
fn drain_inbox() {
    let me = std::thread::current().id();
    let taken = INBOX.lock().ok().and_then(|mut g| {
        g.as_mut().and_then(|m| m.remove(&me))
    });
    if let Some(ids) = taken {
        DIRTY.with(|d| {
            let mut guard = d.borrow_mut();
            let set = guard.get_or_insert_with(HashSet::new);
            set.extend(ids);
        });
    }
}

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
    drain_inbox();
    DIRTY.with(|d| d.borrow().is_none()) || FORCE_GLOBAL.load(Ordering::Relaxed)
}

/// Drain and return the current dirty set, replacing it with an empty set.
///
/// After this call `is_global_dirty()` returns `false` until the next call
/// to `reset_to_global_dirty()`. An empty returned set means "nothing is
/// dirty this frame" — new atom writes after this call will call `mark_dirty`
/// and populate the set for the NEXT frame.
pub fn take_dirty_components() -> HashSet<ComponentId> {
    // Anything a worker thread addressed to us becomes part of THIS frame.
    drain_inbox();
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
