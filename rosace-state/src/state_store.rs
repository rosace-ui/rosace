use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;

use rosace_trace::event::ComponentId;

use crate::{use_atom, Atom};

thread_local! {
    static STORE: RefCell<HashMap<(u64, usize), Box<dyn Any>>> =
        RefCell::new(HashMap::new());
}

/// Returns a persistent [`Atom<T>`] scoped to `(component_id, hook_index)`.
///
/// On the first call for a given key the atom is seeded with `default`.
/// Subsequent calls with the same key return the existing atom — preserving
/// state across frame rebuilds. This is the hook model: call order within
/// a single `Component::build()` must be stable.
/// Remove all hook slots for `component_id` from the persistent store.
///
/// Called by the reconciler when a component unmounts so that the next mount
/// of a component at the same tree position starts with fresh state.
pub fn clear_component(component_id: ComponentId) {
    STORE.with(|store| {
        store.borrow_mut().retain(|(cid, _), _| *cid != component_id.0);
    });
}

pub fn hook_state<T: Clone + Send + Sync + 'static>(
    component_id: ComponentId,
    hook_index: usize,
    default: T,
) -> Atom<T> {
    STORE.with(|store| {
        let key = (component_id.0, hook_index);
        let mut map = store.borrow_mut();

        if let Some(existing) = map.get(&key) {
            if let Some(atom) = existing.downcast_ref::<Atom<T>>() {
                // Re-subscribe each frame so the subscriber list stays current.
                atom.subscribe(component_id);
                return atom.clone();
            }
        }

        let atom = use_atom(default);
        // Register this component as a subscriber so atom.set() can mark it dirty.
        atom.subscribe(component_id);
        map.insert(key, Box::new(atom.clone()));
        atom
    })
}
