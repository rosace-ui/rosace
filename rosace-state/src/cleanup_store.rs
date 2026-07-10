/// Per-component cleanup callback storage for the ROSACE lifecycle system.
///
/// Cleanup functions registered via `on_mount` / `ctx.on_cleanup` are stored
/// here (keyed by `ComponentId`) and fired exactly once when the reconciler
/// detects that a component has been removed from the element tree.
use std::cell::RefCell;
use std::collections::HashMap;

use rosace_trace::event::ComponentId;

type CleanupFn = Box<dyn FnOnce() + Send>;
type CleanupMap = HashMap<u64, Vec<CleanupFn>>;

thread_local! {
    static STORE: RefCell<CleanupMap> = RefCell::new(HashMap::new());
}

/// Register a cleanup callback for `id`. Called by `ctx.on_cleanup`.
pub fn register(id: ComponentId, f: Box<dyn FnOnce() + Send>) {
    STORE.with(|s| {
        s.borrow_mut().entry(id.0).or_default().push(f);
    });
}

/// Fire all callbacks for `id` and remove them from the store.
/// Called by the reconciler when a component unmounts.
pub fn fire_and_clear(id: ComponentId) {
    let callbacks = STORE.with(|s| s.borrow_mut().remove(&id.0));
    if let Some(callbacks) = callbacks {
        for cb in callbacks {
            cb();
        }
    }
}

/// Returns `true` if there is at least one callback registered for `id`.
pub fn has_callbacks(id: ComponentId) -> bool {
    STORE.with(|s| {
        s.borrow()
            .get(&id.0)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    })
}
