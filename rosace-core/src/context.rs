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

    /// D008's `permanent` tier, on the hook model (D114/D121): like
    /// [`Context::state`], but the FIRST initialization reads `key` from
    /// the installed persist backend (falling back to `default` if the
    /// key is absent or its bytes are stale), and every later `set`
    /// writes through — the value survives a full app restart.
    ///
    /// `key` is app-global: two components using the same key share the
    /// same stored value (by design — it's a storage key, not a hook
    /// slot). With no backend installed (headless tests, or before
    /// `App::launch`) this behaves exactly like plain [`Context::state`].
    ///
    /// Uses the atom's single `on_change` slot for the write-through —
    /// see `Atom::set_on_change`'s doc.
    pub fn state_permanent<T>(&mut self, key: &str, default: T) -> Atom<T>
    where
        T: crate::persist::PersistValue + Clone + Send + Sync + 'static,
    {
        let wired = self.state(false);
        let initial = if wired.get() {
            default // atom already exists; hook_state ignores this value
        } else {
            match crate::persist::persist_backend().and_then(|b| b.get(key).ok().flatten()) {
                Some(bytes) => T::from_persist_bytes(&bytes).unwrap_or(default),
                None => default,
            }
        };
        let atom = self.state(initial);
        if !wired.get() {
            wired.set(true);
            if crate::persist::persist_backend().is_some() {
                let key = key.to_string();
                let value_atom = atom.clone();
                atom.set_on_change(move |_, _| {
                    if let Some(backend) = crate::persist::persist_backend() {
                        let _ = backend.set(&key, &value_atom.get().to_persist_bytes());
                    }
                });
            }
        }
        atom
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
