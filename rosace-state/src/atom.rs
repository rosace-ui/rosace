use std::sync::{Arc, Mutex};

use rosace_trace::event::{AtomId, ComponentId};

// ---------------------------------------------------------------------------
// Trace-value helper
//
// Converts a reference to TraceValue::Debug when T: Debug, else Opaque.
// Uses the "inherent method shadows trait method" trick for stable Rust.
// NOTE: In a fully generic monomorphisation context (T unconstrained) the
// compiler always selects the trait fallback (Opaque).  Concrete Debug types
// used at monomorphisation sites will select the inherent impl.  Full
// specialisation requires nightly `feature(specialization)` and is deferred.
// ---------------------------------------------------------------------------
#[cfg(debug_assertions)]
mod trace_value {
    use rosace_trace::event::TraceValue;

    pub struct Wrap<'a, T>(pub &'a T);

    /// Fallback: returns [`TraceValue::Opaque`] for any `T`.
    pub trait TraceVal {
        fn to_trace_val(&self) -> TraceValue;
    }

    impl<T> TraceVal for Wrap<'_, T> {
        fn to_trace_val(&self) -> TraceValue {
            TraceValue::Opaque
        }
    }

    impl<T: std::fmt::Debug> Wrap<'_, T> {
        /// Inherent impl: preferred over the trait impl when `T: Debug`.
        #[allow(dead_code)]
        pub fn to_trace_val(&self) -> TraceValue {
            TraceValue::Debug(format!("{:?}", self.0))
        }
    }
}

// ---------------------------------------------------------------------------
// AtomInner
// ---------------------------------------------------------------------------

type OnChangeFn = Arc<dyn Fn(AtomId, Vec<ComponentId>) + Send + Sync>;

struct AtomInner<T> {
    id: AtomId,
    value: T,
    subscribers: Vec<ComponentId>,
    /// The thread each subscriber lives on, parallel to `subscribers`.
    ///
    /// A write from a worker thread must dirty the SUBSCRIBER's thread, not
    /// its own: an async result (`use_query` finishing an HTTP call on a
    /// spawned thread) would otherwise mark a set no engine reads — the UI
    /// wakes, finds nothing dirty, never rebuilds.
    ///
    /// Recorded at SUBSCRIBE time rather than at atom creation. A
    /// `GlobalAtom` is a `OnceLock`, so its creating thread is whichever one
    /// happened to touch it first — which is not necessarily, or even
    /// usually, where its subscribers live.
    subscriber_threads: Vec<std::thread::ThreadId>,
    /// Notified after every value change.  Stored as `Arc` so it can be
    /// cloned out of the lock before being called.
    on_change: Option<OnChangeFn>,
}

// ---------------------------------------------------------------------------
// Atom<T>
// ---------------------------------------------------------------------------

/// A reactive value. When changed, all subscriber components are scheduled for
/// rebuild by the refresh engine.
///
/// Cloning an [`Atom`] is cheap — all clones share the same inner state via
/// [`Arc`]. This mirrors how atoms are passed through a component tree.
pub struct Atom<T: 'static> {
    inner: Arc<Mutex<AtomInner<T>>>,
}

impl<T: 'static> Clone for Atom<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: 'static> Atom<T> {
    /// Creates a new atom with the given `id` and initial `value`.
    pub fn new(id: AtomId, value: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AtomInner {
                id,
                value,
                subscribers: Vec::new(),
                subscriber_threads: Vec::new(),
                on_change: None,
            })),
        }
    }

    /// Returns the [`AtomId`] that uniquely identifies this atom.
    pub fn id(&self) -> AtomId {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).id
    }

    /// Returns a clone of the current value and records an [`AtomRead`] trace.
    ///
    /// [`AtomRead`]: rosace_trace::event::RosaceTrace::AtomRead
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let val = guard.value.clone();
        #[cfg(debug_assertions)]
        let atom_id = guard.id;
        drop(guard);

        rosace_trace::trace!(rosace_trace::event::RosaceTrace::AtomRead {
            atom: atom_id,
            component: rosace_trace::event::ComponentId(0),
        });

        val
    }

    /// Replaces the current value, notifies subscribers, and emits an
    /// [`AtomWrite`] trace.
    ///
    /// If a [`crate::batch`] is active the dirty notification is queued and
    /// dispatched when the batch closes.
    ///
    /// [`AtomWrite`]: rosace_trace::event::RosaceTrace::AtomWrite
    pub fn set(&self, value: T)
    where
        T: PartialEq,
    {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        // Value-dedup: writing the SAME value is a no-op — no dirty, no frame
        // request, no on_change. Without this, any callback that fires the
        // current value every frame (e.g. a control whose gesture re-reports an
        // unchanged selection) marks its subscribers dirty forever → the UI
        // repaints every frame producing identical pixels, pegging the CPU.
        // "Dirty repaint" must mean *actually changed*.
        if guard.value == value {
            return;
        }

        let atom_id = guard.id;

        #[cfg(debug_assertions)]
        let (old_trace, new_trace) = {
            use trace_value::{TraceVal as _, Wrap};
            (Wrap(&guard.value).to_trace_val(), Wrap(&value).to_trace_val())
        };

        guard.value = value;
        let subscribers = guard.subscribers.clone();
        let threads = guard.subscriber_threads.clone();
        let on_change = guard.on_change.clone();
        drop(guard);

        #[cfg(debug_assertions)]
        rosace_trace::trace!(rosace_trace::event::RosaceTrace::AtomWrite {
            atom: atom_id,
            old: old_trace,
            new: new_trace,
            by: rosace_trace::event::ComponentId(0),
            location: rosace_trace::location!(),
        });

        if crate::batch::is_batching() {
            crate::batch::queue_dirty(atom_id, subscribers);
        } else {
            crate::dirty_set::mark_dirty_per_subscriber_from(atom_id, &subscribers, &threads);
            crate::frame_scheduler::request_frame();
            if let Some(cb) = on_change {
                cb(atom_id, subscribers);
            }
        }
    }

    /// Writes unconditionally — for value types that are not `PartialEq` (a
    /// theme bundle carrying a type-erased extension map, a controller holding
    /// callbacks). Prefer [`set`](Self::set), which dedups equal writes; only
    /// reach for this when the type genuinely cannot be compared, and only for
    /// atoms that are not written every frame.
    pub fn set_always(&self, value: T) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let atom_id = guard.id;
        guard.value = value;
        let subscribers = guard.subscribers.clone();
        let threads = guard.subscriber_threads.clone();
        let on_change = guard.on_change.clone();
        drop(guard);
        if crate::batch::is_batching() {
            crate::batch::queue_dirty(atom_id, subscribers);
        } else {
            crate::dirty_set::mark_dirty_per_subscriber_from(atom_id, &subscribers, &threads);
            crate::frame_scheduler::request_frame();
            if let Some(cb) = on_change {
                cb(atom_id, subscribers);
            }
        }
    }

    /// Atomically reads the current value, applies `f`, and writes the result.
    ///
    /// The read-modify-write is performed under a single lock acquisition so
    /// concurrent callers cannot interleave their updates.
    pub fn update(&self, f: impl FnOnce(&T) -> T)
    where
        T: PartialEq,
    {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let new_value = f(&guard.value);
        if guard.value == new_value {
            return; // unchanged → no dirty/frame (see `set`)
        }
        let atom_id = guard.id;

        #[cfg(debug_assertions)]
        let (old_trace, new_trace) = {
            use trace_value::{TraceVal as _, Wrap};
            (Wrap(&guard.value).to_trace_val(), Wrap(&new_value).to_trace_val())
        };

        guard.value = new_value;
        let subscribers = guard.subscribers.clone();
        let threads = guard.subscriber_threads.clone();
        let on_change = guard.on_change.clone();
        drop(guard);

        #[cfg(debug_assertions)]
        rosace_trace::trace!(rosace_trace::event::RosaceTrace::AtomWrite {
            atom: atom_id,
            old: old_trace,
            new: new_trace,
            by: rosace_trace::event::ComponentId(0),
            location: rosace_trace::location!(),
        });

        if crate::batch::is_batching() {
            crate::batch::queue_dirty(atom_id, subscribers);
        } else {
            crate::dirty_set::mark_dirty_per_subscriber_from(atom_id, &subscribers, &threads);
            crate::frame_scheduler::request_frame();
            if let Some(cb) = on_change {
                cb(atom_id, subscribers);
            }
        }
    }

    /// Unconditional read-modify-write for non-`PartialEq` values — the
    /// [`update`](Self::update) counterpart of [`set_always`](Self::set_always).
    pub fn update_always(&self, f: impl FnOnce(&T) -> T) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let new_value = f(&guard.value);
        let atom_id = guard.id;
        guard.value = new_value;
        let subscribers = guard.subscribers.clone();
        let threads = guard.subscriber_threads.clone();
        let on_change = guard.on_change.clone();
        drop(guard);
        if crate::batch::is_batching() {
            crate::batch::queue_dirty(atom_id, subscribers);
        } else {
            crate::dirty_set::mark_dirty_per_subscriber_from(atom_id, &subscribers, &threads);
            crate::frame_scheduler::request_frame();
            if let Some(cb) = on_change {
                cb(atom_id, subscribers);
            }
        }
    }

    /// Registers `component_id` as a subscriber of this atom.
    ///
    /// Duplicate registrations are silently ignored.
    pub fn subscribe(&self, component_id: ComponentId) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let here = std::thread::current().id();
        if let Some(i) = guard.subscribers.iter().position(|&c| c == component_id) {
            // Re-subscribing from a different thread re-homes it: a
            // component only ever builds on one thread, so the latest
            // subscribe is the truth.
            guard.subscriber_threads[i] = here;
            return;
        }
        guard.subscribers.push(component_id);
        guard.subscriber_threads.push(here);
    }

    /// Removes `component_id` from the subscriber list.
    pub fn unsubscribe(&self, component_id: ComponentId) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = guard.subscribers.iter().position(|&c| c == component_id) {
            guard.subscribers.remove(i);
            guard.subscriber_threads.remove(i);
        }
    }

    /// Sets the callback invoked after each value change (outside the
    /// value lock — reading the atom from inside the callback is safe).
    ///
    /// Only one callback can be registered at a time; calling this again
    /// replaces the previous one. Public since D121: the persistence
    /// write-through (`Context::state_permanent`) claims this slot for
    /// its atoms — don't also register on a persistent atom or you'll
    /// silently disable its persistence.
    pub fn set_on_change(
        &self,
        f: impl Fn(AtomId, Vec<ComponentId>) + Send + Sync + 'static,
    ) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_change = Some(Arc::new(f));
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    /// A background thread writing an atom must rebuild the SUBSCRIBER's
    /// component, not dirty its own thread's set.
    ///
    /// `use_query` and every async hook depend on this: the worker finishes
    /// an HTTP call and writes the result atom from its own thread. Marking
    /// locally means the UI thread wakes, finds nothing dirty, and never
    /// rebuilds — the request completes and the screen never updates. That
    /// regression was live for a few hours and is why this test exists.
    #[test]
    fn a_write_from_another_thread_dirties_the_subscribing_thread() {
        use rosace_trace::event::ComponentId;
        crate::dirty_set::reset_to_global_dirty();
        let _ = crate::dirty_set::take_dirty_components();

        let atom = Atom::new(crate::next_atom_id(), 0i32);
        atom.subscribe(ComponentId(4242)); // subscribed HERE

        let a = atom.clone();
        std::thread::spawn(move || a.set(99)).join().unwrap();

        let dirty = crate::dirty_set::take_dirty_components();
        assert!(dirty.contains(&ComponentId(4242)),
            "the subscribing thread never saw the off-thread write; got {dirty:?}");
    }
}
