//! Hero / shared-element transition support (D108/Phase 26 Step 5).
//!
//! A `.hero_tag(id)`'d widget is a pass-through with zero behavior change
//! UNLESS it paints while `ScreenTransitionView` has an active transition in
//! flight — in that case it captures its own world rect + a standalone
//! [`Picture`] instead of painting itself in place, and registers it here
//! under its tag. `ScreenTransitionView` (the only reader of this registry)
//! drains both sides after painting the outgoing and incoming screens for
//! the frame, pairs up entries sharing a tag, and paints a single floating
//! copy on top, LERP'd between the two captured rects by the transition's
//! progress. Entries present on only one side (no matching tag) are simply
//! dropped — there's nothing to morph between, so that widget just doesn't
//! render for the duration of the transition on that side (an honest,
//! documented limitation, not a crash).
//!
//! Thread-local, mirroring `overlay.rs`'s own registry — paint always runs
//! on one thread, and this is drained once per frame just like overlays.

use std::cell::RefCell;
use std::collections::HashMap;

use tezzera_core::types::Rect;
use tezzera_render::Picture;

/// Which side of an in-flight transition a `Hero`-tagged widget is
/// currently painting on. Set by `ScreenTransitionView` immediately before
/// painting each side, and cleared (`None`) the rest of the time — that
/// `None` state is what makes `Hero` a zero-cost pass-through by default.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeroRole {
    Outgoing,
    Incoming,
}

struct HeroCapture {
    rect: Rect,
    picture: Picture,
}

thread_local! {
    static ACTIVE_ROLE: RefCell<Option<HeroRole>> = const { RefCell::new(None) };
    static OUTGOING: RefCell<HashMap<String, HeroCapture>> = RefCell::new(HashMap::new());
    static INCOMING: RefCell<HashMap<String, HeroCapture>> = RefCell::new(HashMap::new());
}

/// Marks (or clears) which side of a transition is about to be painted.
pub fn set_active_role(role: Option<HeroRole>) {
    ACTIVE_ROLE.with(|r| *r.borrow_mut() = role);
}

/// The role `Hero::paint` should register under right now, if any.
pub fn active_role() -> Option<HeroRole> {
    ACTIVE_ROLE.with(|r| *r.borrow())
}

/// Called by `Hero::paint` while a role is active.
pub fn register(tag: String, role: HeroRole, rect: Rect, picture: Picture) {
    let cap = HeroCapture { rect, picture };
    match role {
        HeroRole::Outgoing => OUTGOING.with(|m| { m.borrow_mut().insert(tag, cap); }),
        HeroRole::Incoming => INCOMING.with(|m| { m.borrow_mut().insert(tag, cap); }),
    }
}

/// Drain both sides' captures for this frame, pairing tags present on
/// BOTH — `(tag, outgoing_rect, outgoing_picture, incoming_rect, incoming_picture)`.
/// Unmatched entries (tag present on only one side) are dropped.
pub fn drain_pairs() -> Vec<(String, Rect, Picture, Rect, Picture)> {
    let outgoing: HashMap<String, HeroCapture> = OUTGOING.with(|m| m.borrow_mut().drain().collect());
    let mut incoming: HashMap<String, HeroCapture> = INCOMING.with(|m| m.borrow_mut().drain().collect());
    outgoing
        .into_iter()
        .filter_map(|(tag, out_cap)| {
            incoming.remove(&tag).map(|in_cap| (tag, out_cap.rect, out_cap.picture, in_cap.rect, in_cap.picture))
        })
        .collect()
}
