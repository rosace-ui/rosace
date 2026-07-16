use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::bus::TraceSubscriber;
use crate::event::RosaceTrace;

/// Predicate deciding whether an event is recorded. See
/// [`RingBufferSubscriber::filtered`].
type EventFilter = Arc<dyn Fn(&RosaceTrace) -> bool + Send + Sync>;

/// Retains the last `capacity` trace events in a circular buffer.
///
/// Used for time-travel debugging — the dev tools can read the buffer to replay
/// the sequence of events leading up to the current state or a crash.
///
/// Default capacity: 1000 events.
pub struct RingBufferSubscriber {
    buffer: Arc<Mutex<VecDeque<RosaceTrace>>>,
    capacity: usize,
    /// When set, only events for which this returns true are retained
    /// (D123/O1 — the flight recorder excludes high-frequency events).
    filter: Option<EventFilter>,
}

impl RingBufferSubscriber {
    /// Creates a new ring buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
            filter: None,
        }
    }

    /// A ring buffer that only records events passing `filter` — the basis
    /// of the always-on flight recorder, which excludes high-frequency
    /// events so it never becomes a per-frame firehose (D123/O1).
    pub fn filtered(
        capacity: usize,
        filter: impl Fn(&RosaceTrace) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
            filter: Some(Arc::new(filter)),
        }
    }

    /// Returns the number of events currently held in the buffer.
    pub fn len(&self) -> usize {
        self.buffer
            .lock()
            .expect("RingBufferSubscriber lock poisoned")
            .len()
    }

    /// Returns true if the buffer contains no events.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drains all buffered events into a `Vec`, oldest first.
    pub fn drain(&self) -> Vec<RosaceTrace> {
        self.buffer
            .lock()
            .expect("RingBufferSubscriber lock poisoned")
            .drain(..)
            .collect()
    }

    /// Returns a snapshot of all buffered events, oldest first, without clearing.
    pub fn snapshot(&self) -> Vec<RosaceTrace> {
        self.buffer
            .lock()
            .expect("RingBufferSubscriber lock poisoned")
            .iter()
            .cloned()
            .collect()
    }
}

impl TraceSubscriber for RingBufferSubscriber {
    fn on_trace(&self, event: &RosaceTrace) {
        if let Some(f) = &self.filter {
            if !f(event) {
                return;
            }
        }
        let mut buf = self
            .buffer
            .lock()
            .expect("RingBufferSubscriber lock poisoned");
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(event.clone());
    }
}
