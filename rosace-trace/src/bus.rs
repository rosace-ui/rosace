use std::sync::{Arc, Mutex};

use crate::event::RosaceTrace;

/// Receives `RosaceTrace` events from the `TracingBus`.
///
/// Implement this trait to create a custom subscriber (console output, ring
/// buffer, file dump, IDE bridge, etc.).
///
/// Implementations must be `Send + Sync` — the bus may be called from any thread.
/// Implementations must not call `TRACING_BUS.emit()` from within `on_trace` to
/// avoid re-entrant locking.
pub trait TraceSubscriber: Send + Sync {
    /// Called for every emitted `RosaceTrace` event.
    fn on_trace(&self, event: &RosaceTrace);
}

/// Central hub that receives `RosaceTrace` events and dispatches to all
/// registered `TraceSubscriber` implementations.
///
/// Access via the `TRACING_BUS` global singleton. The bus is zero-cost in
/// production — all `trace!()` call sites are stripped by `#[cfg(debug_assertions)]`.
pub struct TracingBus {
    subscribers: Mutex<Vec<Arc<dyn TraceSubscriber + Send + Sync>>>,
}

impl Default for TracingBus {
    fn default() -> Self {
        Self::new()
    }
}

impl TracingBus {
    /// Creates a new bus with no subscribers.
    pub const fn new() -> Self {
        Self {
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Registers a subscriber to receive all future trace events.
    pub fn add_subscriber(&self, subscriber: Arc<dyn TraceSubscriber + Send + Sync>) {
        self.subscribers
            .lock()
            .expect("TracingBus subscriber lock poisoned")
            .push(subscriber);
    }

    /// Removes all registered subscribers.
    pub fn clear_subscribers(&self) {
        self.subscribers
            .lock()
            .expect("TracingBus subscriber lock poisoned")
            .clear();
    }

    /// Emits a trace event to all registered subscribers.
    ///
    /// The subscriber list lock is released before calling any subscriber so that
    /// subscribers can safely call `add_subscriber` without deadlocking.
    pub fn emit(&self, event: RosaceTrace) {
        let subs: Vec<Arc<dyn TraceSubscriber + Send + Sync>> = self
            .subscribers
            .lock()
            .expect("TracingBus subscriber lock poisoned")
            .clone();

        for sub in &subs {
            sub.on_trace(&event);
        }
    }
}

/// The global `TracingBus` singleton.
///
/// All ROSACE systems emit events through this bus. Access it directly only
/// when adding subscribers at startup. For emitting events, prefer the `trace!()`
/// macro which gates emission behind `#[cfg(debug_assertions)]`.
pub static TRACING_BUS: TracingBus = TracingBus::new();

/// Emits a `RosaceTrace` event — zero cost in production.
///
/// In debug builds, forwards the event to `TRACING_BUS`. In release builds,
/// the entire call is compiled away with no overhead.
///
/// # Example
/// ```rust
/// use rosace_trace::{trace, event::{RosaceTrace, ComponentId}, location};
///
/// trace!(RosaceTrace::ComponentUnmount {
///     id: ComponentId(1),
///     name: "MyComponent",
/// });
/// ```
#[macro_export]
macro_rules! trace {
    ($event:expr) => {
        #[cfg(debug_assertions)]
        $crate::TRACING_BUS.emit($event);
    };
}

/// Captures the current source location as a `Location`.
///
/// # Example
/// ```rust
/// use rosace_trace::location;
/// let loc = location!();
/// ```
#[macro_export]
macro_rules! location {
    () => {
        $crate::event::Location {
            file: file!(),
            line: line!(),
        }
    };
}
