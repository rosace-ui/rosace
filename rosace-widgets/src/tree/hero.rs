//! Hero / shared-element transitions.
//!
//! A `.hero_tag(id)`'d widget is a pass-through with zero behaviour change
//! outside a transition. While `ScreenTransitionView` has one running, it
//! registers its world rect and a HANDLE TO ITSELF here under its tag.
//! `ScreenTransitionView` — the only reader — pairs the two sides by tag after
//! painting both screens and promotes ONE live instance to the root layer,
//! laid out at a rect interpolated between the two ends.
//!
//! An endpoint stands aside only once its tag is ACTUALLY in the air, and it
//! is told so (`is_flying`) rather than inferring it from "a transition is
//! happening". A tag with no counterpart never pairs, so inferring made that
//! widget vanish for the whole transition — and, once the empty frame was
//! cached, for good. Same shape as Flutter, where the navigator starts the
//! flight and the endpoints are notified.
//!
//! ## Why a widget and not a Picture
//!
//! This registry used to hold a captured `Picture` per side, and the flight
//! was those dead pixels replayed morphed. That froze anything animating
//! inside a hero, re-captured BOTH screens every frame, and made the
//! transition depend on a paint side effect — a cached outgoing screen that
//! replayed instead of repainting never registered, so the pair never formed
//! and the element vanished for the whole flight (69e0cde).
//!
//! `BoxedWidget` is an `Arc<dyn Widget>`, so registering the widget itself is
//! a refcount bump. The promoted copy then paints live, reflows at each
//! interpolated size, and runs its own animations mid-flight.
//!
//! ## What does not travel
//!
//! The promoted copy is a fresh instance built from the same config, so
//! per-node state does not survive the flight: a text field loses its cursor,
//! a scroll offset resets. This is Flutter's documented Hero limitation and
//! the same trade it makes. Carrying state across would mean reparenting the
//! real node, which needs a deferred-disposal window — nodes dispose
//! immediately on removal today, so the widget is destroyed before the layer
//! could claim it. Recorded as separate work.
//!
//! Thread-local, mirroring the paint walk: paint always runs on one thread,
//! and this is drained once per frame.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use rosace_core::types::Rect;

use super::BoxedWidget;

/// Which side of an in-flight transition a `Hero`-tagged widget is currently
/// painting on. Set by `ScreenTransitionView` immediately before painting each
/// side and cleared (`None`) the rest of the time — that `None` is what makes
/// `Hero` a zero-cost pass-through by default.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeroRole {
    Outgoing,
    Incoming,
}

/// One side's registration: where it is, and what to fly.
struct HeroEnd {
    rect: Rect,
    widget: BoxedWidget,
}

thread_local! {
    static ACTIVE_ROLE: RefCell<Option<HeroRole>> = const { RefCell::new(None) };
    /// Tags with a flight ACTUALLY in the air right now.
    ///
    /// An endpoint hides itself only while its tag is in here, never merely
    /// because a transition is running. A tag present on one side only never
    /// pairs, so hiding on the strength of "a transition is happening" made
    /// that widget invisible for the whole transition — and, once the frame
    /// it was hidden on got cached, indefinitely afterwards.
    ///
    /// This is Flutter's shape: the flight is started by the navigator and
    /// the endpoints are told, rather than each endpoint inferring it.
    static FLYING: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static OUTGOING: RefCell<HashMap<String, HeroEnd>> = RefCell::new(HashMap::new());
    static INCOMING: RefCell<HashMap<String, HeroEnd>> = RefCell::new(HashMap::new());
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
pub fn register(tag: String, role: HeroRole, rect: Rect, widget: BoxedWidget) {
    let end = HeroEnd { rect, widget };
    match role {
        HeroRole::Outgoing => OUTGOING.with(|m| { m.borrow_mut().insert(tag, end); }),
        HeroRole::Incoming => INCOMING.with(|m| { m.borrow_mut().insert(tag, end); }),
    };
}

/// One matched hero: where it starts, where it lands, and what flies.
pub struct HeroFlight {
    pub tag: String,
    pub from: Rect,
    pub to: Rect,
    /// The INCOMING side's widget — the destination is what the user is
    /// travelling towards, and it is what should be on screen when the
    /// flight lands. Flutter's default flight shuttle makes the same choice.
    pub widget: BoxedWidget,
}

/// Is a flight for `tag` in the air, so its endpoints should stand aside?
pub fn is_flying(tag: &str) -> bool {
    FLYING.with(|f| f.borrow().contains(tag))
}

/// The transition is over — every endpoint paints itself again.
///
/// Called on the first settled frame. Endpoints mark themselves dirty while
/// hidden precisely so this frame re-records them: without that they would
/// replay the cached picture they were hidden in, and the element would never
/// come back.
pub fn end_flights() {
    FLYING.with(|f| f.borrow_mut().clear());
    OUTGOING.with(|m| m.borrow_mut().clear());
    INCOMING.with(|m| m.borrow_mut().clear());
}

/// Drain both sides, pairing tags present on BOTH. Unmatched entries are
/// dropped — see the module docs.
pub fn drain_pairs() -> Vec<HeroFlight> {
    let outgoing: HashMap<String, HeroEnd> = OUTGOING.with(|m| m.borrow_mut().drain().collect());
    let mut incoming: HashMap<String, HeroEnd> = INCOMING.with(|m| m.borrow_mut().drain().collect());
    let flights: Vec<HeroFlight> = outgoing
        .into_iter()
        .filter_map(|(tag, out)| {
            incoming.remove(&tag).map(|inc| HeroFlight {
                tag,
                from: out.rect,
                to: inc.rect,
                widget: inc.widget,
            })
        })
        .collect();
    // Only tags that actually PAIRED are in the air. Recorded for the next
    // frame: on this one the endpoints have already painted themselves, and
    // at t=0 the flight sits exactly on the source rect, so the overlap is
    // invisible.
    FLYING.with(|f| {
        let mut set = f.borrow_mut();
        set.clear();
        for fl in &flights { set.insert(fl.tag.clone()); }
    });
    flights
}
