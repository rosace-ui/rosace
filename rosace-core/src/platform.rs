//! Global platform provider: `use_platform()` and `set_platform()` (D105).
//!
//! Widgets never branch on platform directly — the running platform exists
//! only to drive THEME resolution (a platform-keyed `Themes` bundle picks
//! the active `ThemeData` once at startup). This mirrors the `safe_area`
//! provider's shape exactly: a detected default, overridable, read through a
//! `GlobalAtom` so it's cheap and consistent everywhere.

use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;

/// Reserved atom ID for the platform atom (must not collide with other
/// reserved IDs — see `rosace_theme::provider::THEME_ATOM_ID` at 0xFFFF and
/// `safe_area::SAFE_AREA_ATOM_ID` at 0xFFFE).
const PLATFORM_ATOM_ID: AtomId = AtomId(0xFFFD);

/// The platform ROSACE is running on.
///
/// Deliberately flat (no separate "Desktop" catch-all alongside `MacOs`/
/// `Windows`/`Linux`) — the AppBar proof (Phase 23 Step 3) needs to tell
/// macOS apart from other desktop OSes (traffic-light inset), so folding
/// them into one variant would immediately need un-folding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
    Ios,
    Android,
    Web,
}

impl Platform {
    /// Detects the platform at compile time — `cfg(target_arch = "wasm32")`
    /// for web (sufficient on its own: nothing else compiles to wasm32 in
    /// this codebase, so no runtime `navigator.platform` query is needed),
    /// `cfg(target_os)` for every native target.
    pub fn detect() -> Self {
        #[cfg(target_arch = "wasm32")]
        return Platform::Web;
        #[cfg(target_os = "macos")]
        return Platform::MacOs;
        #[cfg(target_os = "windows")]
        return Platform::Windows;
        #[cfg(target_os = "linux")]
        return Platform::Linux;
        #[cfg(target_os = "ios")]
        return Platform::Ios;
        #[cfg(target_os = "android")]
        return Platform::Android;
        // Unreachable for every target this workspace actually builds for;
        // kept so the function is total rather than panicking on a future
        // target this list hasn't been taught about yet.
        #[allow(unreachable_code)]
        Platform::Linux
    }

    pub fn is_desktop(&self) -> bool {
        matches!(self, Platform::MacOs | Platform::Windows | Platform::Linux)
    }

    pub fn is_mobile(&self) -> bool {
        matches!(self, Platform::Ios | Platform::Android)
    }
}

static CURRENT_PLATFORM: GlobalAtom<Platform> = GlobalAtom::new(PLATFORM_ATOM_ID, Platform::detect);

/// Returns the currently active platform — the real detected one unless
/// overridden via [`set_platform`] (e.g. `App::platform(Platform::Ios)` to
/// preview an iOS theme on desktop).
pub fn use_platform() -> Platform {
    CURRENT_PLATFORM.get()
}

/// Overrides the active platform. Called by `App::platform(..)` at startup;
/// app code should not normally need this directly.
pub fn set_platform(p: Platform) {
    CURRENT_PLATFORM.set(p);
}
