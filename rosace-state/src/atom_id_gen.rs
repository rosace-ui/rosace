use std::sync::atomic::{AtomicU64, Ordering};

use rosace_trace::event::AtomId;

static NEXT_ATOM_ID: AtomicU64 = AtomicU64::new(1);

/// Allocates a fresh, globally unique [`AtomId`].
///
/// Uses a relaxed atomic counter — IDs are unique but not sequentially ordered
/// across threads. Starts at 1 so that `AtomId(0)` can serve as a sentinel.
pub fn next_atom_id() -> AtomId {
    AtomId(NEXT_ATOM_ID.fetch_add(1, Ordering::Relaxed))
}
