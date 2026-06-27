//! Global theme provider: `use_theme()` and `set_theme()`.

use tezzera_state::GlobalAtom;
use tezzera_trace::event::AtomId;

use crate::built_in;
use crate::theme::ThemeData;

/// Stable atom ID reserved for the current-theme atom.
///
/// Must not collide with any dynamically generated atom IDs used elsewhere.
/// Using a high fixed value (0xFFFF) leaves the low range free for runtime atoms.
const THEME_ATOM_ID: AtomId = AtomId(0xFFFF);

/// App-wide theme atom. Defaults to the built-in light theme.
///
/// Changing this atom triggers a full re-render of all subscribed components.
static CURRENT_THEME: GlobalAtom<ThemeData> =
    GlobalAtom::new(THEME_ATOM_ID, built_in::light_theme);

/// Returns a clone of the currently active `ThemeData`.
///
/// Components should call this during their `build()` method to access design
/// tokens without any manual subscription setup.
pub fn use_theme() -> ThemeData {
    CURRENT_THEME.get()
}

/// Replaces the active theme with `theme` and notifies all subscribers.
///
/// Typically called at app startup or in response to a user preference change.
pub fn set_theme(theme: ThemeData) {
    CURRENT_THEME.set(theme);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_theme_returns_valid_theme() {
        let theme = use_theme();
        // The default theme is the light theme, so is_dark should be false.
        // However, if another test already called set_theme(), we just check
        // that the returned value is structurally valid (spacing > 0).
        assert!(theme.spacing.md > 0.0, "spacing.md should be positive");
        assert!(theme.radius.md >= 0.0, "radius.md should be non-negative");
    }

    #[test]
    fn set_theme_updates_the_global() {
        use crate::built_in::dark_theme;

        let dark = dark_theme();
        set_theme(dark);

        let current = use_theme();
        assert!(current.is_dark, "theme should now be dark after set_theme");

        // Restore light theme so other tests are not affected.
        set_theme(crate::built_in::light_theme());
    }

    #[test]
    fn use_theme_typography_is_consistent() {
        let theme = use_theme();
        assert!(
            theme.typography.display_large.size > theme.typography.body_large.size,
            "display_large should be larger than body_large in the active theme"
        );
    }
}
