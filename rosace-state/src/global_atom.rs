use std::sync::OnceLock;

use rosace_trace::event::AtomId;

use crate::atom::Atom;

/// An atom with app-wide scope.
///
/// No provider is needed — a [`GlobalAtom`] can be declared as a `static` and
/// accessed from any module. The inner [`Atom`] is lazily initialised on first
/// access using the provided `default` factory function.
///
/// # Example
/// ```rust
/// use rosace_state::GlobalAtom;
/// use rosace_trace::event::AtomId;
///
/// static THEME: GlobalAtom<&'static str> =
///     GlobalAtom::new(AtomId(1), || "light");
///
/// fn toggle_theme() {
///     THEME.set("dark");
/// }
/// ```
pub struct GlobalAtom<T: 'static> {
    id: AtomId,
    cell: OnceLock<Atom<T>>,
    default: fn() -> T,
}

impl<T: Clone + Send + Sync + 'static> GlobalAtom<T> {
    /// Creates a new [`GlobalAtom`] with the given stable `id` and `default`
    /// factory.  Const-evaluable so it can be placed in a `static`.
    pub const fn new(id: AtomId, default: fn() -> T) -> Self {
        Self {
            id,
            cell: OnceLock::new(),
            default,
        }
    }

    /// Returns a reference to the underlying [`Atom`], initialising it on
    /// first call.
    pub fn get_or_init(&self) -> &Atom<T> {
        self.cell.get_or_init(|| Atom::new(self.id, (self.default)()))
    }

    /// Writes a new value to the atom, notifying all subscribers.
    pub fn set(&self, value: T) {
        self.get_or_init().set(value);
    }

    /// Returns a clone of the atom's current value.
    pub fn get(&self) -> T {
        self.get_or_init().get()
    }
}
