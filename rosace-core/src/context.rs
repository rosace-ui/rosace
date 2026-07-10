use crate::types::ComponentId;
use rosace_state::{hook_state, Atom};

/// Per-component context passed to every [`Component::build`] call.
///
/// Carries the component's identity and provides access to persistent local
/// state via [`Context::state`]. State is keyed by `(component_id, call_order)`
/// — the hook model — so call order within `build()` must be stable across frames.
pub struct Context {
    pub(crate) component_id: ComponentId,
    pub(crate) hook_index: usize,
}

impl Context {
    pub fn new(id: ComponentId) -> Self {
        Context {
            component_id: id,
            hook_index: 0,
        }
    }

    pub fn component_id(&self) -> ComponentId {
        self.component_id
    }

    /// Returns a persistent [`Atom<T>`] for local component state.
    ///
    /// On first call per slot the atom is seeded with `default`. On subsequent
    /// frames the existing atom is returned, preserving the last value.
    pub fn state<T: Clone + Send + Sync + 'static>(&mut self, default: T) -> Atom<T> {
        let idx = self.hook_index;
        self.hook_index += 1;
        hook_state(self.component_id, idx, default)
    }

    /// Registers a cleanup function that runs when this component unmounts.
    ///
    /// Stored in the persistent [`rosace_state::cleanup_store`] keyed by
    /// component ID. The reconciler fires these callbacks when the component
    /// disappears from the element tree.
    pub fn on_cleanup(&mut self, f: impl FnOnce() + Send + 'static) {
        rosace_state::cleanup_store::register(self.component_id, Box::new(f));
    }
}
