use std::sync::{Arc, Mutex};

use tezzera_trace::event::{AtomId, ComponentId};

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
    use tezzera_trace::event::TraceValue;

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
    /// [`AtomRead`]: tezzera_trace::event::TezzeraTrace::AtomRead
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let val = guard.value.clone();
        #[cfg(debug_assertions)]
        let atom_id = guard.id;
        drop(guard);

        tezzera_trace::trace!(tezzera_trace::event::TezzeraTrace::AtomRead {
            atom: atom_id,
            component: tezzera_trace::event::ComponentId(0),
        });

        val
    }

    /// Replaces the current value, notifies subscribers, and emits an
    /// [`AtomWrite`] trace.
    ///
    /// If a [`crate::batch`] is active the dirty notification is queued and
    /// dispatched when the batch closes.
    ///
    /// [`AtomWrite`]: tezzera_trace::event::TezzeraTrace::AtomWrite
    pub fn set(&self, value: T) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let atom_id = guard.id;

        #[cfg(debug_assertions)]
        let (old_trace, new_trace) = {
            use trace_value::{TraceVal as _, Wrap};
            (Wrap(&guard.value).to_trace_val(), Wrap(&value).to_trace_val())
        };

        guard.value = value;
        let subscribers = guard.subscribers.clone();
        let on_change = guard.on_change.clone();
        drop(guard);

        #[cfg(debug_assertions)]
        tezzera_trace::trace!(tezzera_trace::event::TezzeraTrace::AtomWrite {
            atom: atom_id,
            old: old_trace,
            new: new_trace,
            by: tezzera_trace::event::ComponentId(0),
            location: tezzera_trace::location!(),
        });

        if crate::batch::is_batching() {
            crate::batch::queue_dirty(atom_id, subscribers);
        } else {
            crate::dirty_set::mark_dirty(&subscribers);
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
    pub fn update(&self, f: impl FnOnce(&T) -> T) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let new_value = f(&guard.value);
        let atom_id = guard.id;

        #[cfg(debug_assertions)]
        let (old_trace, new_trace) = {
            use trace_value::{TraceVal as _, Wrap};
            (Wrap(&guard.value).to_trace_val(), Wrap(&new_value).to_trace_val())
        };

        guard.value = new_value;
        let subscribers = guard.subscribers.clone();
        let on_change = guard.on_change.clone();
        drop(guard);

        #[cfg(debug_assertions)]
        tezzera_trace::trace!(tezzera_trace::event::TezzeraTrace::AtomWrite {
            atom: atom_id,
            old: old_trace,
            new: new_trace,
            by: tezzera_trace::event::ComponentId(0),
            location: tezzera_trace::location!(),
        });

        if crate::batch::is_batching() {
            crate::batch::queue_dirty(atom_id, subscribers);
        } else {
            crate::dirty_set::mark_dirty(&subscribers);
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
        if !guard.subscribers.contains(&component_id) {
            guard.subscribers.push(component_id);
        }
    }

    /// Removes `component_id` from the subscriber list.
    pub fn unsubscribe(&self, component_id: ComponentId) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.subscribers.retain(|&id| id != component_id);
    }

    /// Sets the callback invoked after each value change.
    ///
    /// Only one callback can be registered at a time; calling this again
    /// replaces the previous one.  Used internally by the refresh engine;
    /// currently exercised only from tests until the engine integration lands.
    #[allow(dead_code)]
    pub(crate) fn set_on_change(
        &self,
        f: impl Fn(AtomId, Vec<ComponentId>) + Send + Sync + 'static,
    ) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_change = Some(Arc::new(f));
    }
}
