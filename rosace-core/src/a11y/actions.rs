//! Actions requested by assistive technology, queued for the engine.
//!
//! A screen-reader user does not tap a button — they select it and issue an
//! *activate* action. Until this existed, ROSACE announced controls
//! correctly and then ignored every attempt to use one: roles and labels
//! shipped, activation did not.
//!
//! ## Why a queue rather than a direct call
//!
//! The platform bridges sit BELOW the engine. `rosace-platform`'s AccessKit
//! adapter, and the iOS/Android hosts calling in over FFI, cannot reach the
//! engine's dispatch path — that would invert the dependency. So they push a
//! request here and the engine drains it on its next frame, the same shape
//! the frame scheduler already uses for wakeups.
//!
//! Draining on a frame boundary is also what decouples the timing: the
//! request lands whenever assistive tech makes it, and is served at a point
//! where the render tree is consistent and the widget callbacks it will run
//! are safe to run.
//!
//! The queue is THREAD-LOCAL. See the comment on `PENDING` — briefly, a
//! request only means anything to the engine that owns the node, and a
//! global one is drained by whichever engine paints first.
//!
//! ## Identity
//!
//! A request names a node by the id published in the semantic tree, which
//! packs the render-tree node and its semantics-entry index
//! (`(node_id << 8) | entry`). The engine resolves it back to a rect and
//! dispatches through the ordinary hit path, so an activation does exactly
//! what a tap on that control does — including press state, focus changes
//! and any overlay it opens. Reimplementing activation separately would let
//! the two drift.

use std::cell::RefCell;

/// What assistive tech asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A11yAction {
    /// Activate the control: VoiceOver double-tap, TalkBack double-tap,
    /// AccessKit's `Click`. Equivalent to a press.
    Activate,
    /// Move keyboard focus to this node.
    Focus,
}

/// A queued request naming a node from the published semantic tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct A11yActionRequest {
    /// The `SemanticNode::id` the tree published for this node.
    pub node_id: u64,
    pub action: A11yAction,
}

// THREAD-LOCAL, not a process global.
//
// It was a global first, and that was wrong for the same reason the dirty
// set was: `take()` DRAINS, so any other engine painting concurrently
// consumes a request meant for this one. It showed up immediately as the
// activation test failing intermittently under a parallel run — the request
// was swallowed by an unrelated test's paint.
//
// Thread-local is also correct rather than merely convenient: a request only
// makes sense to the engine that owns the node, and platform accessibility
// callbacks are delivered on the UI thread anyway (AppKit's main thread;
// accesskit_winit dispatches inside the event loop). A request pushed from
// some other thread has no engine to serve it.
thread_local! {
    static PENDING: RefCell<Vec<A11yActionRequest>> = const { RefCell::new(Vec::new()) };
}

/// Called by a platform bridge when assistive tech requests something.
pub fn request(node_id: u64, action: A11yAction) {
    PENDING.with(|q| {
        let mut q = q.borrow_mut();
        // Bound the queue. If no engine is draining (a headless host, or a
        // frame loop that has stopped), requests would otherwise accumulate
        // for the life of the thread.
        if q.len() < 64 {
            q.push(A11yActionRequest { node_id, action });
        }
    });
}

/// Drained by the engine once per frame.
pub fn take() -> Vec<A11yActionRequest> {
    PENDING.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Split a published semantic id back into `(render-tree node, entry index)`.
///
/// The inverse of the packing in `RenderTree::collect_semantics`. It lives
/// here, beside the queue, so the two halves of the encoding cannot drift
/// apart in separate crates.
pub fn split_node_id(id: u64) -> (usize, usize) {
    ((id >> 8) as usize, (id & 0xff) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    // No serial lock needed: the queue is thread-local and each test runs
    // on its own thread, which is the isolation the global version lacked.

    #[test]
    fn requests_round_trip_in_order_and_drain_once() {
        let _ = take();

        request(0x0102, A11yAction::Activate);
        request(0x0203, A11yAction::Focus);

        let got = take();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], A11yActionRequest { node_id: 0x0102, action: A11yAction::Activate });
        assert_eq!(got[1].action, A11yAction::Focus);

        assert!(take().is_empty(), "a drained queue must not replay");
    }

    /// Without a bound, a host with no running frame loop accumulates
    /// requests forever.
    #[test]
    fn the_queue_is_bounded() {
        let _ = take();
        for _ in 0..500 {
            request(1, A11yAction::Activate);
        }
        assert_eq!(take().len(), 64);
    }

    /// Must match `RenderTree::collect_semantics`'s packing exactly.
    #[test]
    fn ids_split_back_into_node_and_entry() {
        assert_eq!(split_node_id((7 << 8) | 3), (7, 3));
        assert_eq!(split_node_id(0), (0, 0));
        // The entry index is masked to a byte on the way in, so a node id
        // large enough to matter still round-trips.
        assert_eq!(split_node_id((100_000u64 << 8) | 255), (100_000, 255));
    }
}
