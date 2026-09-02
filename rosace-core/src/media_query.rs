//! Global "environment" query provider: `use_media_query()` and
//! `set_media_query()` (D126) — mirrors `safe_area.rs` exactly, same
//! reasoning: the platform layer measures an OS-level environment value
//! once and publishes it here, so widgets read it as ordinary state instead
//! of every widget branching on platform.
//!
//! # Text scale
//! iOS and Android both let the user scale system-wide text size up or
//! down for accessibility (iOS: Settings → Accessibility → Display & Text
//! Size → Larger Text, surfaced via `UIContentSizeCategory`; Android:
//! Settings → Display → Font size, surfaced via `Configuration.fontScale`).
//! `text_scale` carries that multiplier; `1.0` is the system default.
//! Desktop/web have no equivalent OS-wide accessibility text-scale concept
//! distinct from DPI (DPI/pixel density is already handled separately via
//! `rosace_state::render_scale`), so it stays `1.0` there — not a gap, an
//! intentional platform difference.
//!
//! Applied automatically wherever text size is resolved
//! (`FontCache::measure_text`/`measure_text_weighted` and
//! `PaintCtx::draw_text_at`/`text`/`text_styled`), so every existing
//! widget respects it with zero per-widget code changes — a widget that
//! asks to draw at `14.0`px actually renders (and is measured/laid out,
//! so surrounding containers size correctly around it) at `14.0 *
//! text_scale`px.
//!
//! # Other fields
//! `is_dark`, `bold_text`, `reduce_motion`, and `always_24_hour_format`
//! mirror `text_scale`'s reasoning — each is an OS accessibility/appearance
//! signal, sourced per-platform where the OS actually exposes it, and left
//! at its documented default (`false`) on platforms with no clean source
//! for it (see `rosace-platform`/`rosace-ffi` native push call sites for
//! exactly what's wired per platform). `is_dark` drives
//! `rosace_theme::sync_system_theme`; `bold_text` and `reduce_motion` are
//! applied at the same text/animation choke points `text_scale` uses;
//! `always_24_hour_format` is detection-only today — no widget consumes it
//! yet (`TimePicker` has no 24-hour dial mode).

use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;

/// Reserved atom ID for the media-query atom (must not collide with other
/// reserved IDs — see `rosace_theme::provider::THEME_ATOM_ID` at 0xFFFF).
const MEDIA_QUERY_ATOM_ID: AtomId = AtomId(0xFFF4);

/// OS/environment values a widget might want to adapt to, beyond what
/// `use_platform()`/`use_safe_area()` already cover.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaQuery {
    /// OS accessibility text-size multiplier — see this module's own doc.
    pub text_scale: f32,
    /// System-wide light/dark appearance — `true` when the OS is in dark
    /// mode. Drives `rosace_theme::sync_system_theme` unless the app has
    /// locked a `ThemeMode`.
    pub is_dark: bool,
    /// OS accessibility "bold text everywhere" toggle.
    pub bold_text: bool,
    /// OS accessibility "reduce motion" toggle — when set, ROSACE's default
    /// animations snap instead of easing (see `PaintCtx::animate_to`).
    pub reduce_motion: bool,
    /// Whether the OS locale/settings prefer a 24-hour clock over 12-hour
    /// + AM/PM. Detection-only today — see this module's own doc.
    pub always_24_hour_format: bool,
}

impl Default for MediaQuery {
    fn default() -> Self {
        Self {
            text_scale: 1.0,
            is_dark: false,
            bold_text: false,
            reduce_motion: false,
            always_24_hour_format: false,
        }
    }
}

static CURRENT_MEDIA_QUERY: GlobalAtom<MediaQuery> = GlobalAtom::new(MEDIA_QUERY_ATOM_ID, MediaQuery::default);

/// Returns the currently active environment values (`text_scale: 1.0` on
/// platforms that don't have an OS-wide accessibility text-scale setting).
pub fn use_media_query() -> MediaQuery {
    CURRENT_MEDIA_QUERY.get()
}

/// Replaces the active environment values. Called by the platform layer on
/// startup and whenever the OS setting changes (iOS:
/// `UIContentSizeCategory.didChangeNotification`; Android:
/// `onConfigurationChanged`); app code should not normally call this.
pub fn set_media_query(mq: MediaQuery) {
    CURRENT_MEDIA_QUERY.set(mq);
    // A `GlobalAtom` write has no per-component subscribers in the
    // dirty-tracking graph, so `mark_dirty` is a silent no-op here — and a
    // push from OUTSIDE input dispatch (native OS callback) has no other
    // event to ride along on to trigger a real repaint (root-caused live,
    // see `rosace_theme::provider::sync_system_theme`'s matching comment).
    // This used to work by accident because every native call site paired
    // `set_media_query` with `sync_system_theme`, which did this same
    // reset for `is_dark`; doing it here directly makes `text_scale`,
    // `bold_text`, `reduce_motion`, and `always_24_hour_format` correct on
    // their own, independent of whatever else a caller happens to invoke.
    rosace_state::request_rebuild_from_any_thread();
}

// ---------------------------------------------------------------------------
// Tests
//
// This is the one platform-agnostic choke point every native push (desktop
// `WindowEvent::ThemeChanged`, web `matchMedia` "change", iOS
// `traitCollectionDidChange`, Android `onConfigurationChanged`, all four in
// `rosace-platform`/`rosace-ffi`) funnels through — so a test here covers
// the push→cache→repaint contract for every platform's native call site at
// once, without needing a live OS on each one to prove it.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_media_query_reads_back_what_was_set() {
        let mq = MediaQuery { text_scale: 1.3, is_dark: true, bold_text: true, reduce_motion: true, always_24_hour_format: true };
        set_media_query(mq);
        assert_eq!(use_media_query(), mq);
        set_media_query(MediaQuery::default()); // restore, other tests share this global
    }

    #[test]
    fn set_media_query_forces_a_real_repaint() {
        // Simulate "some frame already ran and settled" — not globally dirty.
        rosace_state::reset_to_global_dirty();
        let _ = rosace_state::take_dirty_components();
        assert!(!rosace_state::is_global_dirty());

        // A push from OUTSIDE input dispatch (native OS callback) must force
        // the next frame to repaint on its own, not depend on some other
        // event happening to also mark something dirty — this is the exact
        // bug class that shipped a "changes apply after a random multi-
        // second delay" regression before `set_media_query` did this itself.
        set_media_query(MediaQuery { text_scale: 1.5, ..MediaQuery::default() });
        assert!(rosace_state::is_global_dirty());

        set_media_query(MediaQuery::default()); // restore
    }
}
