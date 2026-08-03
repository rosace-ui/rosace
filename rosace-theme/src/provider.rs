//! Global theme provider: `use_theme()` and `set_theme()`.

use std::sync::OnceLock;

use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;

use crate::built_in;
use crate::theme::ThemeData;

/// App-registered light/dark pair, set once at startup via
/// [`register_theme_pair`]. [`sync_system_theme`]/[`set_theme_mode`] resolve
/// through this (falling back to the generic `built_in` themes if the app
/// never registered one) so a customized `theme.rs` (edited colors, a
/// platform-specific `Themes` bundle, etc.) is honored by system-brightness
/// switching too, instead of being silently discarded in favor of the
/// generic built-ins.
static THEME_PAIR: OnceLock<(ThemeData, ThemeData)> = OnceLock::new();

/// Registers the app's own light/dark `ThemeData` — call once at startup
/// (e.g. `rosace_theme::register_theme_pair(crate::theme::light(), crate::theme::dark())`)
/// so [`ThemeMode::System`]/[`set_theme_mode`] apply YOUR themes rather than
/// the generic `built_in::light_theme()`/`dark_theme()`. A second call is a
/// no-op (first registration wins, matching `OnceLock` semantics) — call it
/// exactly once, before the first frame.
pub fn register_theme_pair(light: ThemeData, dark: ThemeData) {
    let _ = THEME_PAIR.set((light, dark));
}

fn theme_pair() -> (ThemeData, ThemeData) {
    THEME_PAIR.get().cloned().unwrap_or_else(|| (built_in::light_theme(), built_in::dark_theme()))
}

/// Stable atom ID reserved for the current-theme atom.
///
/// Must not collide with any dynamically generated atom IDs used elsewhere.
/// Using a high fixed value (0xFFFF) leaves the low range free for runtime atoms.
const THEME_ATOM_ID: AtomId = AtomId(0xFFFF);

/// Reserved atom ID for the theme-mode atom (must not collide with other
/// reserved IDs — see `THEME_ATOM_ID` above at 0xFFFF, `safe_area`'s
/// `SAFE_AREA_ATOM_ID` at 0xFFFE, `platform`'s `PLATFORM_ATOM_ID` at 0xFFFD,
/// `media_query`'s `MEDIA_QUERY_ATOM_ID` at 0xFFF4).
const THEME_MODE_ATOM_ID: AtomId = AtomId(0xFFFC);

/// App-wide theme atom. Defaults to the built-in light theme.
///
/// Changing this atom triggers a full re-render of all subscribed components.
static CURRENT_THEME: GlobalAtom<ThemeData> =
    GlobalAtom::new(THEME_ATOM_ID, built_in::light_theme);

/// Whether the active theme should follow the OS light/dark setting, or is
/// pinned by the app/user. Mirrors `rosace_core::platform`'s enum+atom shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    /// Follow `rosace_core::media_query().is_dark` — the default. Every
    /// native env-push call site calls [`sync_system_theme`] after updating
    /// the media query, which swaps in `dark_theme()`/`light_theme()`.
    System,
    /// Locked to light regardless of the OS setting.
    Light,
    /// Locked to dark regardless of the OS setting.
    Dark,
}

static CURRENT_THEME_MODE: GlobalAtom<ThemeMode> = GlobalAtom::new(THEME_MODE_ATOM_ID, || ThemeMode::System);

/// Returns the active theme mode (`System` by default).
pub fn use_theme_mode() -> ThemeMode {
    CURRENT_THEME_MODE.get()
}

/// Pins the theme to `mode`. Passing `ThemeMode::System` re-enables
/// following the OS setting on the next [`sync_system_theme`] call (e.g. the
/// next native env-push, or call it directly to apply immediately).
pub fn set_theme_mode(mode: ThemeMode) {
    CURRENT_THEME_MODE.set(mode);
    if mode != ThemeMode::System {
        let (light, dark) = theme_pair();
        set_theme(if mode == ThemeMode::Dark { dark } else { light });
        // See the matching comment in `sync_system_theme` — forces the
        // already-imminent next frame to actually repaint instead of
        // waiting on an unrelated dirty trigger.
        rosace_state::reset_to_global_dirty();
    }
}

/// Applies the OS brightness (`rosace_core::media_query().is_dark`) to the
/// active theme, but only while `use_theme_mode() == ThemeMode::System` — an
/// app/user that pinned `Light`/`Dark` via [`set_theme_mode`] is left alone.
/// Native platform code calls this right after every
/// `rosace_core::set_media_query(..)` push.
pub fn sync_system_theme() {
    if use_theme_mode() != ThemeMode::System {
        return;
    }
    let is_dark = rosace_core::media_query::use_media_query().is_dark;
    let (light, dark) = theme_pair();
    set_theme(if is_dark { dark } else { light });
    // `set_theme` writes a `GlobalAtom`, which has no per-component
    // subscribers in the dirty-tracking graph (unlike a `ctx.state()` atom
    // read inside `build()`) — so `mark_dirty` is a silent no-op here and
    // the actual repaint would otherwise wait for some UNRELATED event
    // (mouse move, hover, an animation tick) to incidentally trigger the
    // next real frame. A push from OUTSIDE the input-dispatch path (native
    // OS env-change callback, not a widget click) has no such event to
    // ride along on, so the theme change appeared to "eventually" apply
    // after a random multi-second delay — reproduced and root-caused live.
    // Force the next already-imminent frame (`set_theme`'s own
    // `request_frame()` wakes the loop promptly) to actually repaint.
    rosace_state::reset_to_global_dirty();
}

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
/// Globally enable/disable animation on the LIVE theme — one call makes
/// every Switch/Checkbox/Radio (and future animated widget) ease or snap.
pub fn set_animations(enabled: bool) {
    let mut t = use_theme();
    t.animation.enabled = enabled;
    set_theme(t);
}

pub fn set_theme(theme: ThemeData) {
    CURRENT_THEME.set_always(theme); // ThemeData carries a non-PartialEq ext map
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

    #[test]
    fn sync_system_theme_follows_os_brightness_in_system_mode() {
        set_theme_mode(ThemeMode::System);

        let mut mq = rosace_core::media_query::use_media_query();
        mq.is_dark = true;
        rosace_core::set_media_query(mq);
        sync_system_theme();
        assert!(use_theme().is_dark, "System mode should follow OS dark push");

        mq.is_dark = false;
        rosace_core::set_media_query(mq);
        sync_system_theme();
        assert!(!use_theme().is_dark, "System mode should follow OS light push");
    }

    #[test]
    fn sync_system_theme_leaves_a_pinned_mode_alone() {
        set_theme_mode(ThemeMode::Dark);

        let mut mq = rosace_core::media_query::use_media_query();
        mq.is_dark = false; // OS says light...
        rosace_core::set_media_query(mq);
        sync_system_theme(); // ...but the app pinned Dark, so this must be a no-op.
        assert!(use_theme().is_dark, "pinned ThemeMode::Dark must ignore OS brightness pushes");

        set_theme_mode(ThemeMode::System); // restore for other tests
        set_theme(crate::built_in::light_theme());
    }
}
