use std::cell::{Cell, RefCell};

use tezzera_trace::event::{AtomId, ComponentId};

thread_local! {
    static BATCHING: Cell<bool> = const { Cell::new(false) };
    static PENDING_DIRTY: RefCell<Vec<(AtomId, Vec<ComponentId>)>> =
        const { RefCell::new(Vec::new()) };
}

/// Priority level for atom updates.
///
/// Controls how the refresh engine schedules the resulting rebuild work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Bypass batching and trigger an immediate rebuild on the next microtask.
    Immediate,
    /// Default priority. Updates are batched within synchronous blocks.
    Normal,
    /// Defer the rebuild to idle time; suitable for non-visible or low-urgency data.
    Background,
}

/// Returns `true` if a [`batch`] is currently active on the calling thread.
pub fn is_batching() -> bool {
    BATCHING.with(|b| b.get())
}

/// Enqueue a dirty notification for later flush.
///
/// Called from [`crate::Atom::set`] when a batch is active so that subscriber
/// rebuilds are deferred until the batch boundary.
pub fn queue_dirty(atom_id: AtomId, subscribers: Vec<ComponentId>) {
    PENDING_DIRTY.with(|q| q.borrow_mut().push((atom_id, subscribers)));
}

/// Execute `f` as a single batch.
///
/// All [`crate::Atom::set`] calls inside `f` queue their dirty notifications
/// rather than dispatching them immediately. After `f` returns the queue is
/// flushed — in Phase 1 this drains and discards the queue; real dispatch to the
/// refresh engine is wired in the integration phase.
pub fn batch<F: FnOnce()>(f: F) {
    BATCHING.with(|b| b.set(true));
    f();
    BATCHING.with(|b| b.set(false));

    let pending: Vec<(AtomId, Vec<ComponentId>)> = PENDING_DIRTY.with(|q| {
        let mut q = q.borrow_mut();
        std::mem::take(&mut *q)
    });
    if !pending.is_empty() {
        for (_atom_id, subs) in &pending {
            crate::dirty_set::mark_dirty(subs);
        }
        crate::frame_scheduler::request_frame();
    }
}
