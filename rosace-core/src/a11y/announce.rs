//! One-off screen-reader announcements.
//!
//! The semantic tree describes what is ON SCREEN. Some things a user needs to
//! hear are not on screen at all, or are not where they are looking: "Copied
//! to clipboard", "3 results", "Message sent", "Upload failed". Those are
//! ANNOUNCEMENTS — spoken once, attached to no widget, gone afterwards.
//!
//! Without this, an app could only make a screen reader say something by
//! putting a visible widget on screen and hoping the user's cursor was on it.
//!
//! ```rust,ignore
//! rosace::a11y::announce("Copied to clipboard", Politeness::Polite);
//! ```
//!
//! Delivered by publishing a node with a LIVE REGION into the next semantic
//! tree; assistive technology speaks a live region's content when it appears
//! or changes. `Politeness` maps to `aria-live` and to AccessKit's `Live`,
//! which is what the macOS/Windows/Linux bridges consume, so one call reaches
//! VoiceOver, Narrator and Orca alike.
//!
//! Thread-local, like the rest of the frame-scoped queues: announcements are
//! made from event handlers, which run on the UI thread.

use std::cell::RefCell;

/// How urgently an announcement should interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Politeness {
    /// Wait for a natural pause. Correct for almost everything — a result
    /// count, a save confirmation. Interrupting someone mid-sentence to say
    /// "saved" is worse than saying it a moment later.
    #[default]
    Polite,
    /// Interrupt whatever is being spoken. For things the user must hear
    /// immediately: a failure, a session about to expire. Overuse makes an
    /// app hostile to listen to, so it is deliberately the longer word.
    Assertive,
}

thread_local! {
    static PENDING: RefCell<Vec<(String, Politeness)>> = const { RefCell::new(Vec::new()) };
}

/// Ask assistive technology to speak `message`.
///
/// No-op for sighted users and costs nothing when no screen reader is
/// attached — the bridge drops the tree before it reaches the OS.
pub fn announce(message: impl Into<String>, politeness: Politeness) {
    let message = message.into();
    if message.trim().is_empty() {
        return;
    }
    PENDING.with(|p| p.borrow_mut().push((message, politeness)));
    // A frame has to happen for the tree to be republished, and an
    // announcement often accompanies a change that paints nothing — "copied
    // to clipboard" with the UI unmoved. Requesting one here is what makes
    // those audible at all.
    rosace_state::request_frame();
}

/// Whether anything is waiting to be spoken.
///
/// The platform layer republishes the semantic tree only on frames that may
/// have changed something. An announcement IS such a change even when no
/// pixel moved, so the publish condition has to consult this too.
pub fn has_pending() -> bool {
    PENDING.with(|p| !p.borrow().is_empty())
}

/// Take everything queued, leaving the queue empty.
///
/// Drained rather than read: an announcement is spoken once. Leaving it in
/// place would re-speak it on every subsequent tree publish, which is the
/// most irritating possible failure mode.
pub fn take() -> Vec<(String, Politeness)> {
    PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()))
}
