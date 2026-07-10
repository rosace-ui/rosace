//! Global safe-area inset provider: `use_safe_area()` and `set_safe_area()`.
//!
//! Some platforms reserve screen regions the app shouldn't draw into (iOS
//! status bar / Dynamic Island / home indicator; Android status/nav bars).
//! Rather than have every widget branch on platform, the platform layer
//! measures the inset once and publishes it here; widgets (starting with
//! `Scaffold`) read it as ordinary padding. Desktop/web never set it, so it
//! defaults to zero and nothing changes for them.

use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;

/// Reserved atom ID for the safe-area atom (must not collide with other
/// reserved IDs — see `rosace_theme::provider::THEME_ATOM_ID` at 0xFFFF).
const SAFE_AREA_ATOM_ID: AtomId = AtomId(0xFFFE);

/// Inset amounts on each edge, in logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SafeArea {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

static CURRENT_SAFE_AREA: GlobalAtom<SafeArea> = GlobalAtom::new(SAFE_AREA_ATOM_ID, SafeArea::default);

/// Returns the currently active safe-area insets (zero on platforms that
/// don't have any — desktop, web).
pub fn use_safe_area() -> SafeArea {
    CURRENT_SAFE_AREA.get()
}

/// Replaces the active safe-area insets. Called by the platform layer on
/// startup and on resize/rotation; app code should not normally call this.
pub fn set_safe_area(insets: SafeArea) {
    CURRENT_SAFE_AREA.set(insets);
}
