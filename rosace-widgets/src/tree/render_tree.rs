//! Persistent render tree — the single owner of per-node retained state (D091).
//!
//! Every widget position gets a node. During paint a widget *declares* its
//! interactive regions and attachments onto its node; the frame pipeline then
//! derives hit-test order, scroll routing, the overlay stack, focus order, and
//! transform layers from the tree. Nothing is re-emitted per frame through
//! side channels, so state survives cache-hit frames by construction.
//!
//! # Identity
//! A node's identity is its position within its parent's paint order. This is
//! safe because widget paint recursion always descends fully once entered —
//! only the element walker may skip a subtree (picture cache hit), and it
//! consumes the child slot *without* resetting it, keeping siblings aligned
//! and the skipped subtree's state intact.
//!
//! The one place positional identity is NOT safe: [`ScreenTransitionView`]
//! (`screen_transition_view.rs`), where the exact same tree position holds a
//! completely different, unrelated screen's subtree every time navigation
//! changes. Positional reuse there silently aliased one screen's scroll
//! offset/animation state onto the next screen that happened to land on the
//! same `NodeId` (2026-08-01, real trackpad + navigation testing). Its child
//! is addressed through [`RenderTree::keyed_slot`] instead of the ordinary
//! [`RenderTree::slot`] — a small, explicitly-keyed side table scoped to
//! that one call site, not a general per-widget keying system.
//!
//! [`ScreenTransitionView`]: super::ScreenTransitionView

use std::collections::HashMap;
use std::sync::Arc;

use rosace_core::types::{Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Picture;

use super::overlay::OverlayEntry;
use super::TransformLayerEntry;

pub type NodeId = usize;

/// A resolved hit/scroll handler — invoked with the event's (x, y) in
/// window-space logical pixels.
pub type HitHandler = Arc<dyn Fn(f32, f32) + Send + Sync>;

/// A nested-scroll chain link (D-NESTED-SCROLL, 2026-08-02) — takes a
/// `(dx, dy)` DELTA (not an absolute position, unlike [`HitHandler`]) and
/// returns whether it actually moved: `true` if it consumed some or all of
/// the delta, `false` if it's already fully exhausted in that exact
/// direction (hard-clamped, or stretched to `Bounce`'s own overscroll
/// limit) and had NO effect. A gesture starting inside nested scrollable
/// regions (an inner `ScrollView`/carousel sitting inside an outer one, or
/// a plain-hit `Button`/`ListTile` sitting inside any `ScrollView`) tries
/// the innermost link first each move and only offers the SAME delta to
/// the next link outward once the current one declines — so scrolling
/// naturally "hands off" to an enclosing scrollable ancestor exactly when,
/// and only when, the inner one has nothing left to give.
pub type ScrollHandler = Arc<dyn Fn(f32, f32) -> bool + Send + Sync>;

/// A click callback with its hit rect in window-space logical pixels.
pub type HitRegion = (Rect, Arc<dyn Fn() + Send + Sync>);
/// A positional click callback — receives the click point in window-space
/// logical pixels (sliders, color pickers, canvases).
pub type HitRegionAt = (Rect, Arc<dyn Fn(f32, f32) + Send + Sync>);

/// Which wheel/trackpad axes a scroll region can consume. Routing prefers
/// the innermost region that handles the DOMINANT axis of a delta — an
/// x-only carousel must not swallow a vertical page scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollAxes {
    pub x: bool,
    pub y: bool,
}

impl ScrollAxes {
    pub const BOTH: ScrollAxes = ScrollAxes { x: true, y: true };
    pub const X: ScrollAxes = ScrollAxes { x: true, y: false };
    pub const Y: ScrollAxes = ScrollAxes { x: false, y: true };
}

/// A `(delta_x, delta_y)` scroll callback with its viewport rect and the
/// axes it handles.
pub type ScrollRegion = (Rect, ScrollAxes, Arc<dyn Fn(f32, f32) + Send + Sync>);

/// A registered pinch-to-zoom region (`InteractiveViewer`, Phase 32) — the
/// callback receives the gesture's `delta` (winit's `PinchGesture::delta`:
/// positive = magnify, negative = shrink; NOT a multiplier, an increment —
/// callers typically do `zoom *= 1.0 + delta`).
pub type ZoomRegion = (Rect, Arc<dyn Fn(f32) + Send + Sync>);

/// One render-tree node. Declared data is cleared when the node is repainted
/// (`begin`) and persists untouched otherwise.
#[derive(Default)]
pub struct TreeNode {
    pub children: Vec<NodeId>,
    /// Child slot cursor for the current paint of this node.
    cursor: usize,
    /// Children addressed by [`RenderTree::keyed_slot`] instead of position
    /// — see the module doc's "Identity" section. Only [`ScreenTransitionView`]
    /// (`screen_transition_view.rs`) uses this; every other widget's children
    /// live in `children`/`cursor` above, untouched.
    ///
    /// [`ScreenTransitionView`]: super::ScreenTransitionView
    pub keyed_children: HashMap<u64, NodeId>,
    /// True if this node was begun (repainted) in the current frame.
    begun: bool,

    // ── Declared per-paint data (D091) ────────────────────────────────────
    pub hits:       Vec<HitRegion>,
    pub hits_at:    Vec<HitRegionAt>,
    /// Nested-scroll chain links declared this node (D-NESTED-SCROLL) —
    /// see [`ScrollHandler`]'s own doc. Separate from `hits_at`: a plain
    /// slider-style positional drag always fully "consumes" a gesture by
    /// definition, but a `ScrollView`'s pan needs to report exhaustion so
    /// an enclosing scrollable ancestor gets a turn.
    pub nested_scrolls: Vec<(Rect, ScrollHandler)>,
    pub scrolls:    Vec<ScrollRegion>,
    pub zooms:      Vec<ZoomRegion>,
    pub focus:      Vec<rosace_core::a11y::FocusNode>,
    pub overlays:   Vec<OverlayEntry>,
    pub transforms: Vec<TransformLayerEntry>,
    pub semantics:  Vec<super::SemanticsProps>,
    /// Drops this node AND its whole subtree from the accessibility tree
    /// (`Semantics::exclude()`). Declared per paint like `semantics` above.
    ///
    /// Subtree-wide rather than node-only on purpose: the point of excluding
    /// is to silence a decorative region entirely, and leaving its children
    /// audible while hiding their parent would produce orphaned, context-free
    /// announcements — worse than either extreme.
    pub semantics_excluded: bool,
    /// Absorbs descendants into this node's own semantics
    /// (`Semantics::merge()`): this node's entry is emitted, then its
    /// subtree is not walked.
    ///
    /// For composites that are one control to a user but many nodes to the
    /// framework — an icon + label + tap target. Without it a screen reader
    /// announces the wrapper's meaning AND the raw inner content, which for
    /// hand-painted content ("::chart::") is noise.
    pub semantics_merges_descendants: bool,

    /// Editable text content declared this paint (D112/Phase 28 Step 1) —
    /// current value, rect, and the `on_change` callback. Cleared each
    /// repaint like `hits`/`scrolls`; the engine's key/click dispatch
    /// reads it fresh rather than caching, since a rebuild may swap in a
    /// different `on_change` closure.
    pub editable: Option<super::text_edit::EditableDecl>,

    // ── Persistent per-node state (NOT cleared on repaint) ───────────────
    /// The node's implicit scroll position (D101) — created lazily by the
    /// first scrollable painted at this position, survives rebuilds like
    /// Flutter's ScrollPosition.
    pub scroll_ctrl: Option<crate::scroll::ScrollController>,
    /// A persistent eased scalar (0..1) for toggle transitions — advanced by
    /// PaintCtx::animate_to. `None` until first observed (then snaps).
    pub anim: Option<f32>,
    /// Multiple independent persistent eased scalars for a widget that needs
    /// to animate more than one value at once (e.g. a Switch's position AND
    /// its hover/press state-layer) — advanced by `PaintCtx::animate_channel`,
    /// indexed by an explicit channel id. Each entry is `None` until first
    /// observed (then snaps), exactly like `anim`. Grows on demand; persists
    /// across repaints and cache-hit frames like the other retained state.
    pub anim_channels: Vec<Option<f32>>,
    /// This node's [`rosace_core::a11y::FocusNode`] (D112/Phase 28 Step 1) —
    /// created lazily by [`super::PaintCtx::focus_node`], survives
    /// rebuilds like `scroll_ctrl` above.
    pub focus_node: Option<rosace_core::a11y::FocusNode>,
    /// Persistent cursor/selection state for an editable node (D091/D112)
    /// — NOT cleared on repaint, so the caret survives a rebuild with the
    /// same displayed value.
    pub text_edit: super::text_edit::TextEditState,

    // ── Picture cache (Phase 20 unification — was the flat RenderNode) ───
    /// Widget type name at this position; a mismatch resets the caches.
    pub tag: &'static str,
    /// Constraints used for the last successful layout pass.
    pub last_constraints: Option<Constraints>,
    /// Size returned by the last layout pass.
    pub cached_size: Option<Size>,
    /// Display list from the last paint pass.
    pub cached_picture: Option<Arc<Picture>>,
    /// World-space rect of the last paint (also the damage extent).
    pub cached_rect: Option<Rect>,
    /// When true, the subtree must re-layout/re-paint this frame.
    pub paint_dirty: bool,

    // ── Interaction state (dispatcher-owned) ─────────────────────────────
    /// True while the cursor is over this node's hit/hover region.
    pub hovered: bool,
    /// True from MouseDown until MouseUp on this node — drives press/tap
    /// feedback (D108/Phase 26 Step 1), same dispatcher-owned shape as
    /// `hovered`.
    pub pressed: bool,
    /// Pointer interception: 1 = ignore (subtree transparent to hits),
    /// 2 = absorb (consume everything in rect). Declared per paint.
    pub pointer_mode: u8,
    /// Hover-only regions (tooltips) — participate in hover_test but not
    /// in click dispatch.
    pub hover_regions: Vec<Rect>,
    /// Long-press callbacks with their rects.
    pub long_hits: Vec<HitRegion>,
}

/// Arena-allocated persistent render tree. Node 0 is always the root.
pub struct RenderTree {
    nodes: Vec<TreeNode>,
    /// Nodes begun this frame — finalized (children truncated) at frame end.
    begun_this_frame: Vec<NodeId>,
}

/// The outcome of one pointer walk over a subtree.
///
/// A bare `Option` cannot express this: "nothing here, keep looking behind
/// me" and "a barrier covers this point, stop looking" are both "no result",
/// but they must produce opposite behaviour in the caller's sibling loop.
/// Conflating them is exactly why `AbsorbPointer` failed to block anything
/// but clicks.
enum Walk<T> {
    /// A match — return it.
    Hit(T),
    /// An `AbsorbPointer` covers this point; the whole walk stops, and
    /// nothing behind the barrier may match.
    Blocked,
    /// No match here; the caller should carry on with its other children.
    Miss,
}

impl<T> Walk<T> {
    /// The result, with a barrier reported as "nothing" to public callers.
    fn hit(self) -> Option<T> {
        match self {
            Walk::Hit(v) => Some(v),
            _ => None,
        }
    }

    /// For a CHILD recursion: `Some(..)` when the parent must return
    /// immediately (a hit to propagate, or a barrier), `None` to keep
    /// iterating siblings.
    fn settled(self) -> Option<Walk<T>> {
        match self {
            Walk::Hit(v) => Some(Walk::Hit(v)),
            Walk::Blocked => Some(Walk::Blocked),
            Walk::Miss => None,
        }
    }
}

impl Walk<()> {
    /// For the GATE at the top of a walk: `Some(..)` when this node must not
    /// be descended into at all.
    fn stop<T>(self) -> Option<Walk<T>> {
        match self {
            Walk::Hit(()) => None,
            Walk::Blocked => Some(Walk::Blocked),
            Walk::Miss => Some(Walk::Miss),
        }
    }
}

impl RenderTree {
    pub fn new() -> Self {
        Self {
            nodes: vec![TreeNode::default()],
            begun_this_frame: Vec::new(),
        }
    }

    pub const ROOT: NodeId = 0;

    /// Start a new frame and begin the root. Must be called before painting.
    pub fn start_frame(&mut self) {
        for &id in &self.begun_this_frame {
            self.nodes[id].begun = false;
        }
        self.begun_this_frame.clear();
        self.begin(Self::ROOT);
    }

    /// Reset a node for a fresh paint: clears its declarations (the picture
    /// cache fields persist — the walker manages those explicitly).
    pub fn reset(&mut self, node: NodeId) {
        self.begin(node);
    }

    /// Begin (re)painting `node`: clear declared data, reset the child cursor.
    fn begin(&mut self, node: NodeId) {
        let n = &mut self.nodes[node];
        n.cursor = 0;
        n.begun = true;
        n.hits.clear();
        n.hits_at.clear();
        n.nested_scrolls.clear();
        n.scrolls.clear();
        n.zooms.clear();
        n.focus.clear();
        n.overlays.clear();
        n.transforms.clear();
        n.semantics.clear();
        n.semantics_excluded = false;
        n.semantics_merges_descendants = false;
        n.pointer_mode = 0;
        n.hover_regions.clear();
        n.long_hits.clear();
        n.editable = None;
        self.begun_this_frame.push(node);
    }

    /// Consume the next child slot of `parent`.
    ///
    /// `reset == true` (normal paint descent): the child is begun — its
    /// declared data is cleared for re-declaration.
    /// `reset == false` (cache-hit replay): the slot is consumed so siblings
    /// stay positionally aligned, but the child subtree keeps all its state.
    pub fn slot(&mut self, parent: NodeId, reset: bool) -> NodeId {
        let cursor = self.nodes[parent].cursor;
        self.nodes[parent].cursor += 1;

        let child = if cursor < self.nodes[parent].children.len() {
            self.nodes[parent].children[cursor]
        } else {
            let id = self.nodes.len();
            self.nodes.push(TreeNode::default());
            self.nodes[parent].children.push(id);
            id
        };

        if reset {
            self.begin(child);
        }
        child
    }

    /// Like [`Self::slot`], but the returned `NodeId` is resolved by an
    /// explicit stable `key` instead of "whatever was previously at this
    /// position" — see the module doc's "Identity" section. Reusing an
    /// existing key's node preserves ALL its sticky state (`scroll_ctrl`,
    /// `anim_channels`, hover/press, and everything underneath it in the
    /// subtree, however deep) exactly like an ordinary same-position
    /// repaint does; a new key gets a brand-new node with empty
    /// `children`/`keyed_children`, so nothing nested under it — however
    /// many `ScrollView`s/`Tabs`/`TextArea`s it contains — can possibly
    /// alias whatever a DIFFERENT key's subtree left behind.
    ///
    /// The resolved node is ALSO written into `parent`'s ordinary
    /// `children`/`cursor` slot, same as `slot()` — the key only changes
    /// which `NodeId` ends up at that position, not how it's found
    /// afterward. This matters: hit-testing, hover, semantics/accessibility,
    /// and the picture-cache walk all traverse `children`, not
    /// `keyed_children` — a node reachable ONLY through the keyed map would
    /// be invisible to all of them (found via a real test failure —
    /// `semantic_labels` came back empty for a screen reached this way).
    pub fn keyed_slot(&mut self, parent: NodeId, key: u64) -> NodeId {
        let child = match self.nodes[parent].keyed_children.get(&key) {
            Some(&id) => id,
            None => {
                let id = self.nodes.len();
                self.nodes.push(TreeNode::default());
                self.nodes[parent].keyed_children.insert(key, id);
                id
            }
        };

        let cursor = self.nodes[parent].cursor;
        self.nodes[parent].cursor += 1;
        if cursor < self.nodes[parent].children.len() {
            self.nodes[parent].children[cursor] = child;
        } else {
            self.nodes[parent].children.push(child);
        }

        self.begin(child);
        child
    }

    /// Drop any of `parent`'s keyed children whose key is no longer in
    /// `valid_keys` — called once per frame by `ScreenTransitionView` with
    /// the navigation stack's current keys, so a screen's cached subtree
    /// (scroll position, animation state, everything) is released once
    /// it's actually been popped, not retained forever. The dropped node's
    /// arena slot itself isn't reclaimed (this arena never frees — same
    /// tradeoff `slot()`'s positional children already have for any widget
    /// that stops being painted), only the reference to it.
    pub fn prune_keyed_children(&mut self, parent: NodeId, valid_keys: &[u64]) {
        self.nodes[parent].keyed_children.retain(|k, _| valid_keys.contains(k));
    }

    /// End of frame: drop unused child slots of every node repainted this
    /// frame, so removed widgets cannot leave ghost hit regions behind.
    pub fn finalize(&mut self) {
        for i in 0..self.begun_this_frame.len() {
            let id = self.begun_this_frame[i];
            let cursor = self.nodes[id].cursor;
            self.nodes[id].children.truncate(cursor);
        }
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut TreeNode {
        &mut self.nodes[id]
    }

    pub fn node(&self, id: NodeId) -> &TreeNode {
        &self.nodes[id]
    }

    /// Every node in the arena, for callers that need to scan rather than
    /// look up a specific id (e.g. tests asserting some node reached a
    /// given interaction state without knowing its id in advance).
    pub fn nodes_iter(&self) -> impl Iterator<Item = &TreeNode> {
        self.nodes.iter()
    }

    /// Same as [`Self::nodes_iter`], paired with each node's [`NodeId`] —
    /// needed by callers that must look the node back up for a second,
    /// mutable pass (D116's `EditController` draining: the engine collects
    /// `(NodeId, controller, ops)` immutably first, since it can't mutate
    /// the tree while iterating it).
    pub fn nodes_indexed(&self) -> impl Iterator<Item = (NodeId, &TreeNode)> {
        self.nodes.iter().enumerate()
    }

    // ── Derivations (D091/D092) ───────────────────────────────────────────

    /// Hit-test walk: children before own regions, later siblings first —
    /// paint order is z-order, so the topmost match wins structurally (D092).
    /// Returns the topmost hit callback, whether it is POSITIONAL —
    /// positional hits become the active drag grab (streamed MouseMove
    /// positions until release); plain hits fire once — and, when the
    /// winner is a plain hit, so a touch/mouse gesture that starts on a
    /// plain-hit child (Button, ListTile, …) sitting inside e.g. a
    /// `ScrollView` can still fall back to dragging that ancestor once
    /// movement shows it's a scroll, not a tap (2026-08-02, real Android
    /// touch testing — without this a plain-hit child sitting anywhere in
    /// a scrollable page permanently shadowed the ScrollView's own drag
    /// region, so touch-drag scrolling silently did nothing on any page
    /// with interactive content — desktop was unaffected since
    /// wheel/trackpad scroll is a wholly separate `InputEvent::Scroll`
    /// path).
    ///
    /// The chain is the SECOND return value, always present — collected
    /// independently of what the leaf hit resolves to (`None`, a plain
    /// tap, or even a positional widget like a `Slider`), so touching
    /// blank scrollable space directly (no leaf hit at all) still yields
    /// a usable chain even though the first value is `None`.
    pub fn hit_test(&self, x: f32, y: f32) -> (Option<(HitHandler, bool)>, Vec<ScrollHandler>) {
        let mut chain = Vec::new();
        let leaf = self.hit_test_node(Self::ROOT, x, y, &mut chain);
        (leaf, chain)
    }

    /// Map screen coords into the content space of a node hosting a placed
    /// scroll layer (D090). A transform node's children declare their hit
    /// regions at content-local coords `(0,0)`-based, but the content is drawn
    /// at the viewport scrolled by the live channel offset. Returns the coords
    /// to descend into children with, and `true` when the point falls OUTSIDE
    /// the viewport (children receive nothing — content is clipped to it).
    /// Non-transform nodes pass coords through unchanged.
    fn child_coords(&self, n: &TreeNode, id: NodeId, x: f32, y: f32) -> (f32, f32, bool) {
        let Some(entry) = n.transforms.first() else { return (x, y, false); };
        let vp = entry.viewport_rect;
        if !contains(&vp, x, y) {
            return (x, y, true);
        }
        let off = rosace_state::scroll_offset(id as u64);
        // `offset` lives in content-native (unzoomed) pixels — a screen
        // delta maps to a SMALLER content delta at higher zoom (the view is
        // magnified), matching InteractiveViewer's pan-by-drag divisor.
        let z = entry.zoom;
        ((x - vp.origin.x) / z + off[0], (y - vp.origin.y) / z + off[1], false)
    }

    /// Walks the SAME recursion `hit_test`/`nested_scroll_chain` both need,
    /// so the two stay perfectly in sync by construction (one traversal,
    /// not two): returns the leaf hit exactly like the old two-element
    /// version did, and — independently of what that leaf is, or even
    /// whether one was found at all — pushes every node's own
     /// How a pointer walk must treat `n`.
    ///
    /// `IgnorePointer` (mode 1) is TRANSPARENT: its subtree never matches,
    /// but content behind it still does, so the walk skips this branch and
    /// carries on with the siblings. `AbsorbPointer` (mode 2) is a BARRIER:
    /// inside its rect neither the subtree nor anything behind it matches,
    /// so the walk stops dead — which is why `Miss` and `Blocked` cannot be
    /// the same answer, and why these walks return [`Walk`] rather than a
    /// bare `Option`.
    ///
    /// Both modes used to be consulted only in `hit_test_node` (and scroll
    /// consulted neither), so an `AbsorbPointer` placed as a modal barrier —
    /// the use its own doc describes — still let hover, long-press, scroll
    /// and click-to-edit reach the content underneath it.
    fn pointer_gate(&self, n: &TreeNode, x: f32, y: f32) -> Walk<()> {
        match n.pointer_mode {
            1 => Walk::Miss,
            2 if n.cached_rect.as_ref().is_some_and(|r| contains(r, x, y)) => Walk::Blocked,
            _ => Walk::Hit(()),
        }
    }


   /// `nested_scrolls` entry covering `(x, y)` onto `chain` as the
    /// recursion unwinds, innermost first.
    fn hit_test_node(&self, id: NodeId, x: f32, y: f32, chain: &mut Vec<ScrollHandler>) -> Option<(HitHandler, bool)> {
        let n = &self.nodes[id];
        // Pointer interceptors (IgnorePointer / AbsorbPointer widgets):
        // 1 = subtree transparent to hits; 2 = consume everything in rect.
        if n.pointer_mode == 1 {
            return None;
        }
        if n.pointer_mode == 2 {
            if let Some(r) = &n.cached_rect {
                if contains(r, x, y) {
                    return Some((Arc::new(|_, _| {}), false));
                }
            }
        }
        // Descend into children in the content space of a placed scroll layer
        // (screen coords elsewhere). Outside the viewport, content is clipped.
        let (cx, cy, clipped) = self.child_coords(n, id, x, y);
        let mut leaf = None;
        if !clipped {
            for &child in n.children.iter().rev() {
                if let Some((cb, positional)) = self.hit_test_node(child, cx, cy, chain) {
                    // Wrap so LATER invocations are remapped too, not just this
                    // one. `child_coords` only converts the coordinates used to
                    // find the hit; the returned callback was previously handed
                    // straight to the caller, which re-invokes it directly with
                    // raw SCREEN coords on every subsequent MouseMove during a
                    // drag (`active_drag` in rosace/src/lib.rs — the callback
                    // is never re-hit-tested once a drag starts). A positional
                    // widget (e.g. Slider) declared inside a GPU-composited
                    // scroll view (D090) expects content-space coordinates on
                    // every call, so bake the SAME remap into the callback
                    // itself whenever this node is a transform host — it then
                    // self-corrects on every future invocation, not just the
                    // first. Composes for nested transforms: each ancestor
                    // wraps once more as the recursion unwinds.
                    let wrapped: HitHandler = match n.transforms.first() {
                        Some(entry) => {
                            let vp = entry.viewport_rect;
                            let z = entry.zoom;
                            Arc::new(move |sx: f32, sy: f32| {
                                let off = rosace_state::scroll_offset(id as u64);
                                cb((sx - vp.origin.x) / z + off[0], (sy - vp.origin.y) / z + off[1]);
                            })
                        }
                        None => cb,
                    };
                    leaf = Some((wrapped, positional));
                    break;
                }
            }
        }
        if leaf.is_none() {
            // Only reached when no child matched — same order as before:
            // positional own-regions first (more specific intent), then
            // plain ones.
            for (rect, cb) in n.hits_at.iter().rev() {
                if contains(rect, x, y) {
                    leaf = Some((cb.clone(), true));
                    break;
                }
            }
            if leaf.is_none() {
                for (rect, cb) in n.hits.iter().rev() {
                    if contains(rect, x, y) {
                        let cb = cb.clone();
                        leaf = Some((Arc::new(move |_, _| cb()), false));
                        break;
                    }
                }
            }
        }
        // Collect THIS node's own nested-scroll region, remapped the same
        // way a hit callback would be if this node hosts a transform —
        // unconditional (runs whether or not a leaf was found above, and
        // regardless of what it was), so the chain always reflects every
        // scrollable ancestor along the real visual path, not just the
        // ones "under" wherever the leaf tap/drag happened to resolve.
        if let Some((_, handler)) = n.nested_scrolls.iter().rev().find(|(r, _)| contains(r, x, y)) {
            let handler = handler.clone();
            let wrapped: ScrollHandler = match n.transforms.first() {
                Some(entry) => {
                    let z = entry.zoom;
                    Arc::new(move |dx: f32, dy: f32| handler(dx / z, dy / z))
                }
                None => handler,
            };
            chain.push(wrapped);
        }
        leaf
    }

    /// Topmost node under the cursor that owns any interactive or hover
    /// region — drives hover state (buttons, tiles, tooltips).
    pub fn hover_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.hover_test_node(Self::ROOT, x, y).hit()
    }

    fn hover_test_node(&self, id: NodeId, x: f32, y: f32) -> Walk<NodeId> {
        let n = &self.nodes[id];
        if let Some(stop) = self.pointer_gate(n, x, y).stop() {
            return stop;
        }
        let (cx, cy, clipped) = self.child_coords(n, id, x, y);
        if !clipped {
            for &child in n.children.iter().rev() {
                if let Some(stop) = self.hover_test_node(child, cx, cy).settled() {
                    return stop;
                }
            }
        }
        let owns = n.hits.iter().map(|(r, _)| r)
            .chain(n.hits_at.iter().map(|(r, _)| r))
            .chain(n.long_hits.iter().map(|(r, _)| r))
            .chain(n.hover_regions.iter())
            .chain(n.nested_scrolls.iter().map(|(r, _)| r))
            .any(|r| contains(r, x, y));
        if owns { Walk::Hit(id) } else { Walk::Miss }
    }

    /// Topmost long-press callback under the cursor.
    pub fn long_press_test(&self, x: f32, y: f32) -> Option<Arc<dyn Fn() + Send + Sync>> {
        self.long_press_node(Self::ROOT, x, y).hit()
    }

    fn long_press_node(&self, id: NodeId, x: f32, y: f32) -> Walk<Arc<dyn Fn() + Send + Sync>> {
        let n = &self.nodes[id];
        if let Some(stop) = self.pointer_gate(n, x, y).stop() {
            return stop;
        }
        let (cx, cy, clipped) = self.child_coords(n, id, x, y);
        if !clipped {
            for &child in n.children.iter().rev() {
                if let Some(stop) = self.long_press_node(child, cx, cy).settled() {
                    return stop;
                }
            }
        }
        for (rect, cb) in n.long_hits.iter().rev() {
            if contains(rect, x, y) {
                return Walk::Hit(cb.clone());
            }
        }
        Walk::Miss
    }

    /// Set the hovered node, clearing the previous one. Marks both the old
    /// and new node dirty so the next walk repaints exactly them (localized
    /// damage). Returns true when the hover target changed.
    pub fn set_hover(&mut self, target: Option<NodeId>) -> bool {
        let current = self.nodes.iter().position(|n| n.hovered);
        if current == target {
            return false;
        }
        if let Some(old) = current {
            self.nodes[old].hovered = false;
            self.nodes[old].paint_dirty = true;
        }
        if let Some(new) = target {
            self.nodes[new].hovered = true;
            self.nodes[new].paint_dirty = true;
        }
        true
    }

    /// Set the pressed node, clearing the previous one — same shape as
    /// [`Self::set_hover`], driven by MouseDown/MouseUp instead of
    /// MouseMove. Returns true when the pressed target changed.
    pub fn set_pressed(&mut self, target: Option<NodeId>) -> bool {
        let current = self.nodes.iter().position(|n| n.pressed);
        if current == target {
            return false;
        }
        if let Some(old) = current {
            self.nodes[old].pressed = false;
            self.nodes[old].paint_dirty = true;
        }
        if let Some(new) = target {
            self.nodes[new].pressed = true;
            self.nodes[new].paint_dirty = true;
        }
        true
    }

    /// Axis-aware scroll routing: among the viewports under the cursor
    /// (innermost first), pick the first that handles the DOMINANT axis of
    /// the delta; fall back to the innermost that handles the other axis.
    /// A horizontal carousel no longer intercepts a vertical page scroll.
    pub fn scroll_test(&self, x: f32, y: f32, dx: f32, dy: f32)
        -> Option<HitHandler>
    {
        let mut candidates: Vec<(ScrollAxes, HitHandler)> = Vec::new();
        self.scroll_candidates(Self::ROOT, x, y, &mut candidates);
        // `scroll_candidates` stops collecting at a barrier, so `candidates`
        // already excludes anything behind it.
        select_scroll_handler(&candidates, dx, dy)
    }

    fn scroll_candidates(
        &self,
        id: NodeId,
        x: f32,
        y: f32,
        out: &mut Vec<(ScrollAxes, HitHandler)>,
    ) -> Walk<()> {
        let n = &self.nodes[id];
        if let Some(stop) = self.pointer_gate(n, x, y).stop() {
            return stop;
        }
        // Descend in the CHILD's coordinate space when this node hosts a
        // transform (D090/D092) — bug found live: a scrollable widget
        // (InteractiveViewer) nested inside another scroll view (a normal
        // scrolling page) registers its own scroll target in that OUTER
        // view's content-local space, not real screen space; recursing with
        // the raw, unremapped (x, y) meant its rect could never match a real
        // cursor position, so scroll silently fell through to the outer
        // page every time. `hit_test_node` already gets this right via
        // `child_coords` for clicks — mirror it here for wheel/trackpad too.
        let (cx, cy, clipped) = self.child_coords(n, id, x, y);
        if !clipped {
            // Children first (topmost/innermost priority), later siblings first.
            for &child in n.children.iter().rev() {
                // A barrier below us also hides US from the scroll: it is
                // painted on top, so the wheel belongs to it, not to the
                // viewport it covers.
                if let Some(stop) = self.scroll_candidates(child, cx, cy, out).settled() {
                    return stop;
                }
            }
        }
        for (rect, axes, cb) in n.scrolls.iter().rev() {
            if contains(rect, x, y) {
                out.push((*axes, cb.clone()));
            }
        }
        Walk::Miss
    }

    /// Innermost registered zoom region under `(x, y)` (trackpad pinch,
    /// `InteractiveViewer`) — same innermost-first, later-sibling-first
    /// priority as `scroll_test`, but with no axis-selection step (a pinch
    /// gesture has no "axis", just one delta).
    pub fn zoom_test(&self, x: f32, y: f32) -> Option<Arc<dyn Fn(f32) + Send + Sync>> {
        self.zoom_candidate(Self::ROOT, x, y).hit()
    }

    fn zoom_candidate(&self, id: NodeId, x: f32, y: f32) -> Walk<Arc<dyn Fn(f32) + Send + Sync>> {
        let n = &self.nodes[id];
        if let Some(stop) = self.pointer_gate(n, x, y).stop() {
            return stop;
        }
        // Same nested-transform remap as `scroll_candidates` — see its
        // comment for the bug this fixes.
        let (cx, cy, clipped) = self.child_coords(n, id, x, y);
        if !clipped {
            for &child in n.children.iter().rev() {
                if let Some(stop) = self.zoom_candidate(child, cx, cy).settled() {
                    return stop;
                }
            }
        }
        for (rect, cb) in n.zooms.iter().rev() {
            if contains(rect, x, y) {
                return Walk::Hit(cb.clone());
            }
        }
        Walk::Miss
    }

    /// All hit regions in tree (paint) order — used by the overlay pass to
    /// flatten a per-entry subtree into a dispatch list.
    pub fn collect_hits(&self) -> Vec<HitRegion> {
        let mut out = Vec::new();
        self.collect_hits_node(Self::ROOT, &mut out);
        out
    }

    fn collect_hits_node(&self, id: NodeId, out: &mut Vec<HitRegion>) {
        let n = &self.nodes[id];
        out.extend(n.hits.iter().cloned());
        for &child in &n.children {
            self.collect_hits_node(child, out);
        }
    }

    /// All scroll regions in tree (paint) order.
    pub fn collect_scrolls(&self) -> Vec<ScrollRegion> {
        let mut out = Vec::new();
        self.collect_scrolls_node(Self::ROOT, &mut out);
        out
    }

    fn collect_scrolls_node(&self, id: NodeId, out: &mut Vec<ScrollRegion>) {
        let n = &self.nodes[id];
        out.extend(n.scrolls.iter().cloned());
        for &child in &n.children {
            self.collect_scrolls_node(child, out);
        }
    }

    /// All focus nodes in tree (paint) order — feeds the Tab cycle each frame,
    /// including cache-hit frames where no widget was repainted.
    pub fn collect_focus(&self) -> Vec<rosace_core::a11y::FocusNode> {
        let mut out = Vec::new();
        self.collect_focus_node(Self::ROOT, &mut out);
        out
    }

    fn collect_focus_node(&self, id: NodeId, out: &mut Vec<rosace_core::a11y::FocusNode>) {
        let n = &self.nodes[id];
        out.extend(n.focus.iter().cloned());
        for &child in &n.children {
            self.collect_focus_node(child, out);
        }
    }

    /// The render-tree node that declared the [`rosace_core::a11y::FocusNode`]
    /// with id `focus_id` (D112/Phase 28 Step 1) — bridges
    /// `FocusManager::focused` (a `FocusNode`'s own global id) back to a
    /// `NodeId`, so the engine's key dispatch can find and mutate that
    /// node's persistent `text_edit`/`editable` state.
    pub fn focus_owner(&self, focus_id: u64) -> Option<NodeId> {
        self.nodes.iter().position(|n| n.focus.iter().any(|f| f.id() == focus_id))
    }

    /// Topmost editable node whose declared rect contains `(x, y)` — used
    /// by the engine to focus (and, Step 1: place the caret at the end
    /// of) an editable widget on click (D112/Phase 28). Same z-order
    /// traversal as [`Self::hover_test`]; editable rects live in
    /// `TreeNode::editable`, declared by [`super::PaintCtx::register_editable`].
    pub fn editable_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.editable_test_node(Self::ROOT, x, y).hit()
    }

    fn editable_test_node(&self, id: NodeId, x: f32, y: f32) -> Walk<NodeId> {
        let n = &self.nodes[id];
        if let Some(stop) = self.pointer_gate(n, x, y).stop() {
            return stop;
        }
        let (cx, cy, clipped) = self.child_coords(n, id, x, y);
        if !clipped {
            for &child in n.children.iter().rev() {
                if let Some(stop) = self.editable_test_node(child, cx, cy).settled() {
                    return stop;
                }
            }
        }
        if let Some(e) = &n.editable {
            if contains(&e.rect, x, y) {
                return Walk::Hit(id);
            }
        }
        Walk::Miss
    }

    /// Derive the accessibility tree (D099): semantics entries in paint
    /// order, nested by render-tree structure. Branches with no semantic
    /// content anywhere below them are pruned.
    pub fn collect_semantics(&self) -> rosace_core::SemanticNode {
        let mut root = rosace_core::SemanticNode::new();
        self.collect_semantics_node(Self::ROOT, &mut root);
        root
    }

    fn collect_semantics_node(&self, id: NodeId, parent: &mut rosace_core::SemanticNode) {
        let n = &self.nodes[id];
        // `Semantics::exclude()` — prune here and the whole subtree goes with
        // it, since we simply never recurse.
        if n.semantics_excluded {
            return;
        }
        for (i, s) in n.semantics.iter().enumerate() {
            let mut sn = rosace_core::SemanticNode::new().role(s.role.clone());
            // Identity + geometry for platform accessibility APIs, which
            // (unlike the HTML/SEO consumer) hold node references across
            // frames and need to answer "where is this on screen".
            // A node may declare several semantics entries, so the id
            // mixes in the entry index to stay unique within the tree —
            // the render-tree NodeId alone would collide.
            sn = sn.id(((id as u64) << 8) | (i as u64 & 0xff));
            if let Some(r) = n.cached_rect { sn = sn.bounds(r); }
            if let Some(l) = &s.label { sn = sn.label(l.clone()); }
            // `value`/`heading_level`/`href` were silently dropped here before
            // D107/Phase 25 — a real gap for a `TextInput`'s current text, a
            // `Slider`/`ProgressBar`'s value, and (once widgets start setting
            // them) a heading's level or a link's target, all of which matter
            // for a faithful HTML/SEO mapping, not just for assistive tech.
            if let Some(v) = &s.value { sn = sn.value(v.clone()); }
            if let Some(lvl) = s.heading_level { sn = sn.heading_level(lvl); }
            if let Some(h) = &s.href { sn = sn.href(h.clone()); }
            parent.children.push(sn);
        }
        // `Semantics::merge()` — this node speaks for its whole subtree, so
        // stop here. Its own entry above is already emitted; the descendants
        // are absorbed rather than announced separately.
        if n.semantics_merges_descendants {
            return;
        }
        // Children nest under THIS node's last semantic entry when it declared
        // one (a Button's inner Text belongs to the Button); nodes with no
        // semantics of their own flatten their children into the parent.
        let target: &mut rosace_core::SemanticNode = if n.semantics.is_empty() {
            parent
        } else {
            let last = parent.children.len() - 1;
            &mut parent.children[last]
        };
        for &child in &n.children {
            self.collect_semantics_node(child, target);
        }
    }

    /// All overlay entries in tree order (insertion order = z-order, D058).
    /// Map a point expressed in `target`'s CONTENT space to window/screen
    /// space, applying the inverse of every transform-host remap on the
    /// path from the root (each is a pure translation: + viewport origin
    /// − scroll offset). Phase 32 bug fix (user-reported): an overlay
    /// anchored by a widget inside a GPU scroll layer (e.g. a Tooltip's
    /// `Absolute` position) carried content coords into the window-space
    /// overlay pass and rendered far from its anchor.
    pub fn content_to_screen(&self, target: NodeId, p: rosace_core::types::Point) -> rosace_core::types::Point {
        let mut path = Vec::new();
        if !self.path_to(Self::ROOT, target, &mut path) {
            return p;
        }
        let mut out = p;
        for &id in &path {
            if id == target {
                continue; // a host remaps its CHILDREN, not itself
            }
            let n = &self.nodes[id];
            if let Some(entry) = n.transforms.first() {
                let off = rosace_state::scroll_offset(id as u64);
                // Inverse of child_coords' `(screen - vp.origin)/zoom + offset`.
                out.x = (out.x - off[0]) * entry.zoom + entry.viewport_rect.origin.x;
                out.y = (out.y - off[1]) * entry.zoom + entry.viewport_rect.origin.y;
            }
        }
        out
    }

    fn path_to(&self, cur: NodeId, target: NodeId, path: &mut Vec<NodeId>) -> bool {
        path.push(cur);
        if cur == target {
            return true;
        }
        for &child in &self.nodes[cur].children {
            if self.path_to(child, target, path) {
                return true;
            }
        }
        path.pop();
        false
    }

    pub fn overlay_ids(&self) -> Vec<(NodeId, usize)> {
        let mut out = Vec::new();
        self.overlay_ids_node(Self::ROOT, &mut out);
        out
    }

    fn overlay_ids_node(&self, id: NodeId, out: &mut Vec<(NodeId, usize)>) {
        let n = &self.nodes[id];
        for i in 0..n.overlays.len() {
            out.push((id, i));
        }
        for &child in &n.children {
            self.overlay_ids_node(child, out);
        }
    }

    /// All transform-layer entries in tree order.
    pub fn transform_ids(&self) -> Vec<(NodeId, usize)> {
        let mut out = Vec::new();
        self.transform_ids_node(Self::ROOT, &mut out);
        out
    }

    fn transform_ids_node(&self, id: NodeId, out: &mut Vec<(NodeId, usize)>) {
        let n = &self.nodes[id];
        for i in 0..n.transforms.len() {
            out.push((id, i));
        }
        for &child in &n.children {
            self.transform_ids_node(child, out);
        }
    }

    /// Read-only snapshot of the live tree (D123/O2) — plain data, safe to
    /// hand to a DevTools overlay: no callbacks, no `Arc<dyn Fn>`, nothing
    /// that could be invoked or mutated through it. "Live" means reachable
    /// from the root through `children` as of the last `finalize()` — an
    /// arena slot orphaned by a removed widget is not included, even though
    /// its `TreeNode` still physically exists until the slot is reused.
    ///
    /// Additive and non-invasive: reads fields every node already carries,
    /// touches nothing about how painting/hit-testing/layout work.
    pub fn inspect(&self) -> Vec<InspectNode> {
        let mut out = Vec::new();
        self.inspect_node(Self::ROOT, None, &mut out);
        out
    }

    fn inspect_node(&self, id: NodeId, parent: Option<NodeId>, out: &mut Vec<InspectNode>) {
        let n = &self.nodes[id];
        out.push(InspectNode {
            id,
            parent,
            children: n.children.clone(),
            tag: n.tag,
            rect: n.cached_rect,
            size: n.cached_size,
            constraints: n.last_constraints,
            semantics: n.semantics.iter()
                .map(|s| (s.role.clone(), s.label.clone()))
                .collect(),
            hit_count: n.hits.len() + n.hits_at.len() + n.long_hits.len(),
            scroll_count: n.scrolls.len(),
            overlay_count: n.overlays.len(),
            has_editable: n.editable.is_some(),
            hovered: n.hovered,
            pressed: n.pressed,
        });
        for &child in &n.children {
            self.inspect_node(child, Some(id), out);
        }
    }

    /// The node whose `rect` contains `(x, y)` and is deepest (most
    /// specific) in the tree — the element-picker hit target (D123/O2).
    /// Unlike [`Self::hover_test`]/[`Self::hit_test`], this considers EVERY
    /// node's paint rect, not just ones that declared an interactive
    /// region — a plain `Container`/`Text` is pickable too. Ties (same
    /// depth) go to the one painted later (topmost in z-order), mirroring
    /// every other hit-order convention in this file.
    pub fn pick(&self, x: f32, y: f32) -> Option<NodeId> {
        let snapshot = self.inspect();
        let by_id: std::collections::HashMap<NodeId, &InspectNode> =
            snapshot.iter().map(|n| (n.id, n)).collect();

        fn depth(by_id: &std::collections::HashMap<NodeId, &InspectNode>, mut id: NodeId) -> u32 {
            let mut d = 0;
            while let Some(p) = by_id.get(&id).and_then(|n| n.parent) {
                d += 1;
                id = p;
            }
            d
        }

        let mut best: Option<(NodeId, u32)> = None;
        for n in &snapshot {
            let Some(r) = n.rect else { continue; };
            if !contains(&r, x, y) { continue; }
            let d = depth(&by_id, n.id);
            match best {
                Some((_, bd)) if bd > d => {}
                Some((bid, bd)) if bd == d && bid > n.id => {}
                _ => best = Some((n.id, d)),
            }
        }
        best.map(|(id, _)| id)
    }
}

/// One node in an [`RenderTree::inspect`] snapshot — plain data only.
#[derive(Clone, Debug)]
pub struct InspectNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// Widget type name (`std::any::type_name`-derived tag already tracked
    /// per node for the picture cache).
    pub tag: &'static str,
    pub rect: Option<Rect>,
    pub size: Option<Size>,
    pub constraints: Option<Constraints>,
    /// This node's own declared semantics (role, label) — usually 0 or 1
    /// entries; a few widgets (e.g. a labeled group) declare more than one.
    pub semantics: Vec<(rosace_core::Role, Option<String>)>,
    pub hit_count: usize,
    pub scroll_count: usize,
    pub overlay_count: usize,
    pub has_editable: bool,
    pub hovered: bool,
    pub pressed: bool,
}

impl Default for RenderTree {
    fn default() -> Self { Self::new() }
}

/// Shared axis-preference selection (also used for overlay scroll routes):
/// first candidate handling the dominant delta axis, else first handling
/// the other axis.
pub fn select_scroll_handler(
    candidates: &[(ScrollAxes, HitHandler)],
    dx: f32,
    dy: f32,
) -> Option<Arc<dyn Fn(f32, f32) + Send + Sync>> {
    let dominant_is_x = dx.abs() > dy.abs();
    let handles_dominant = |a: &ScrollAxes| if dominant_is_x { a.x } else { a.y };
    let handles_other = |a: &ScrollAxes| if dominant_is_x { a.y } else { a.x };
    candidates.iter().find(|(a, _)| handles_dominant(a))
        .or_else(|| candidates.iter().find(|(a, _)| handles_other(a)))
        .map(|(_, cb)| cb.clone())
}

#[inline]
fn contains(r: &Rect, x: f32, y: f32) -> bool {
    x >= r.origin.x
        && x <= r.origin.x + r.size.width
        && y >= r.origin.y
        && y <= r.origin.y + r.size.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::{Point, Size};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
    }

    #[test]
    fn hits_persist_on_unpainted_subtree() {
        let mut t = RenderTree::new();
        t.start_frame();
        let a = t.slot(RenderTree::ROOT, true);
        t.node_mut(a).hits.push((rect(0.0, 0.0, 10.0, 10.0), Arc::new(|| {})));
        t.finalize();

        // Next frame: root repaints but the child slot is kept (cache hit).
        t.start_frame();
        let a2 = t.slot(RenderTree::ROOT, false);
        t.finalize();

        assert_eq!(a, a2);
        assert!(t.hit_test(5.0, 5.0).0.is_some(), "hit must survive the clean frame");
    }

    #[test]
    fn set_pressed_clears_the_previous_target_and_reports_whether_it_changed() {
        let mut t = RenderTree::new();
        t.start_frame();
        let a = t.slot(RenderTree::ROOT, true);
        let b = t.slot(RenderTree::ROOT, true);
        t.finalize();

        assert!(t.set_pressed(Some(a)), "unset -> Some(a) is a change");
        assert!(t.node(a).pressed);
        assert!(!t.node(b).pressed);

        assert!(!t.set_pressed(Some(a)), "Some(a) -> Some(a) is not a change");

        assert!(t.set_pressed(Some(b)), "Some(a) -> Some(b) is a change");
        assert!(!t.node(a).pressed, "old target must be cleared");
        assert!(t.node(b).pressed);

        assert!(t.set_pressed(None), "Some(b) -> None is a change");
        assert!(!t.node(b).pressed);
    }

    #[test]
    fn repaint_clears_declared_data() {
        let mut t = RenderTree::new();
        t.start_frame();
        let a = t.slot(RenderTree::ROOT, true);
        t.node_mut(a).hits.push((rect(0.0, 0.0, 10.0, 10.0), Arc::new(|| {})));
        t.finalize();

        t.start_frame();
        let _a = t.slot(RenderTree::ROOT, true); // fresh repaint, declares nothing
        t.finalize();

        assert!(t.hit_test(5.0, 5.0).0.is_none(), "repaint must clear stale hits");
    }

    #[test]
    fn later_siblings_win_hit_test() {
        let mut t = RenderTree::new();
        t.start_frame();
        let first = t.slot(RenderTree::ROOT, true);
        let hit_first = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hf = hit_first.clone();
        t.node_mut(first).hits.push((rect(0.0, 0.0, 10.0, 10.0), Arc::new(move || {
            hf.store(true, std::sync::atomic::Ordering::SeqCst);
        })));
        let second = t.slot(RenderTree::ROOT, true);
        t.node_mut(second).hits.push((rect(0.0, 0.0, 10.0, 10.0), Arc::new(|| {})));
        t.finalize();

        // Overlapping rects: the later sibling (painted on top) must win.
        let (cb, _) = t.hit_test(5.0, 5.0).0.unwrap();
        cb(0.0, 0.0);
        assert!(!hit_first.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn content_to_screen_inverts_the_scroll_layer_remap() {
        // Same fixture shape as hit_test_maps_through_scroll_layer_offset:
        // viewport at (50,50), scrolled 200 down. A content point at
        // (0, 240) must map to screen (50, 90) — the exact inverse of the
        // hit-test's screen→content mapping (Phase 32 tooltip-position fix).
        let mut t = RenderTree::new();
        t.start_frame();
        let tl = t.slot(RenderTree::ROOT, true);
        t.node_mut(tl).transforms.push(TransformLayerEntry {
            picture: rosace_render::PictureRecorder::new().finish(),
            child_size: Size { width: 100.0, height: 1000.0 },
            viewport_rect: rect(50.0, 50.0, 100.0, 100.0),
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });
        let child = t.slot(tl, true);
        t.finalize();
        rosace_state::set_scroll_offset(tl as u64, [0.0, 200.0]);

        let p = t.content_to_screen(child, rosace_core::types::Point { x: 0.0, y: 240.0 });
        assert_eq!((p.x, p.y), (50.0, 90.0), "content→screen must invert child_coords");

        // A node OUTSIDE any layer maps through unchanged.
        let plain = t.content_to_screen(tl, rosace_core::types::Point { x: 7.0, y: 9.0 });
        assert_eq!((plain.x, plain.y), (7.0, 9.0));

        rosace_state::clear_scroll_offset(tl as u64);
    }

    #[test]
    fn hit_test_maps_through_scroll_layer_offset() {
        use std::sync::atomic::{AtomicBool, Ordering};
        // A transform node with a 100×100 viewport at (50,50), scrolled 200px
        // down. Its child declares a hit at content-local (0,300)-(100,340).
        let mut t = RenderTree::new();
        t.start_frame();
        let tl = t.slot(RenderTree::ROOT, true);
        t.node_mut(tl).transforms.push(TransformLayerEntry {
            picture: rosace_render::PictureRecorder::new().finish(),
            child_size: Size { width: 100.0, height: 1000.0 },
            viewport_rect: rect(50.0, 50.0, 100.0, 100.0),
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });
        let child = t.slot(tl, true);
        let hit = Arc::new(AtomicBool::new(false));
        let h = hit.clone();
        // Content-local region visible at scroll 200 (content y 200..300).
        t.node_mut(child).hits.push((rect(0.0, 220.0, 100.0, 40.0), Arc::new(move || {
            h.store(true, Ordering::SeqCst);
        })));
        t.finalize();

        // Live offset lives in the channel keyed by the transform node id.
        rosace_state::set_scroll_offset(tl as u64, [0.0, 200.0]);

        // Screen (75,90): inside the viewport (50..150); content y = 90-50+200
        // = 240, which lands in the child's [220,260) region → hits.
        let (cb, _) = t.hit_test(75.0, 90.0).0.expect("content region must be hit through the offset");
        cb(0.0, 0.0);
        assert!(hit.load(Ordering::SeqCst), "click mapped into scrolled content");

        // Screen (75, 40): ABOVE the viewport → clipped, no hit.
        assert!(t.hit_test(75.0, 40.0).0.is_none(), "clicks outside the viewport are clipped");

        rosace_state::clear_scroll_offset(tl as u64);
    }

    #[test]
    fn positional_hit_through_transform_remaps_every_invocation() {
        // A positional widget (e.g. a Slider knob) declared inside a
        // GPU-composited scroll view (D090). The app dispatch loop invokes
        // the returned callback once at press time AND again on every
        // subsequent MouseMove for the rest of the drag, WITHOUT re-running
        // hit_test (see the `active_drag` mechanism in rosace/src/lib.rs) —
        // so the callback itself must remap raw screen coords through the
        // transform on every call, not just the one made at hit-test time.
        let mut t = RenderTree::new();
        t.start_frame();
        let tl = t.slot(RenderTree::ROOT, true);
        t.node_mut(tl).transforms.push(TransformLayerEntry {
            picture: rosace_render::PictureRecorder::new().finish(),
            child_size: Size { width: 100.0, height: 1000.0 },
            viewport_rect: rect(50.0, 50.0, 100.0, 100.0),
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        });
        let child = t.slot(tl, true);
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        t.node_mut(child).hits_at.push((rect(0.0, 220.0, 100.0, 40.0), Arc::new(move |cx, cy| {
            r.lock().unwrap().push((cx, cy));
        })));
        t.finalize();

        rosace_state::set_scroll_offset(tl as u64, [0.0, 200.0]);

        // Screen (75,90): content = (75-50+0, 90-50+200) = (25, 240) → inside [220,260).
        let (cb, positional) = t.hit_test(75.0, 90.0).0.expect("must hit the positional region");
        assert!(positional, "hits_at region must report positional=true");
        cb(75.0, 90.0); // initial press — dispatch calls back with the same raw coords used to find it

        // Simulated drag continuation: fresh raw screen coords, same callback,
        // no re-hit-test. Before this fix these would leak straight through
        // unmapped.
        cb(80.0, 95.0); // content = (80-50+0, 95-50+200) = (30, 245)

        let got = received.lock().unwrap();
        assert_eq!(
            *got,
            vec![(25.0, 240.0), (30.0, 245.0)],
            "every invocation must be remapped through the transform, not just the first"
        );

        rosace_state::clear_scroll_offset(tl as u64);
    }

    #[test]
    fn semantics_tree_nests_under_declaring_node() {
        use rosace_core::Role;
        let mut t = RenderTree::new();
        t.start_frame();
        let button = t.slot(RenderTree::ROOT, true);
        t.node_mut(button).semantics.push(
            crate::tree::SemanticsProps::new(Role::Button).label("Save"),
        );
        // Button's inner text node — must nest under the Button.
        let label = t.slot(button, true);
        t.node_mut(label).semantics.push(
            crate::tree::SemanticsProps::new(Role::Text).label("Save"),
        );
        t.finalize();

        let sem = t.collect_semantics();
        assert_eq!(sem.children.len(), 1, "one top-level semantic node");
        assert_eq!(sem.children[0].role, Role::Button);
        assert_eq!(sem.children[0].children.len(), 1);
        assert_eq!(sem.children[0].children[0].role, Role::Text);
    }

    #[test]
    fn collect_semantics_carries_value_heading_level_and_href() {
        // D107/Phase 25: these three were silently dropped by
        // collect_semantics_node before this fix — real gap for HTML/SEO
        // mapping (a TextInput's current text, a heading's level, a link's
        // target all matter for a faithful export, not just role/label).
        use rosace_core::Role;
        let mut t = RenderTree::new();
        t.start_frame();
        let input = t.slot(RenderTree::ROOT, true);
        t.node_mut(input).semantics.push(
            crate::tree::SemanticsProps::new(Role::TextInput).label("Name").value("Ada"),
        );
        let heading = t.slot(RenderTree::ROOT, true);
        t.node_mut(heading).semantics.push(
            crate::tree::SemanticsProps::new(Role::Heading).label("Section").heading_level(2),
        );
        let link = t.slot(RenderTree::ROOT, true);
        t.node_mut(link).semantics.push(
            crate::tree::SemanticsProps::new(Role::Link).label("Docs").href("https://example.com"),
        );
        t.finalize();

        let sem = t.collect_semantics();
        assert_eq!(sem.children[0].value.as_deref(), Some("Ada"));
        assert_eq!(sem.children[1].heading_level, Some(2));
        assert_eq!(sem.children[2].href.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn finalize_drops_removed_children() {
        let mut t = RenderTree::new();
        t.start_frame();
        let a = t.slot(RenderTree::ROOT, true);
        t.node_mut(a).hits.push((rect(0.0, 0.0, 10.0, 10.0), Arc::new(|| {})));
        let b = t.slot(RenderTree::ROOT, true);
        t.node_mut(b).hits.push((rect(20.0, 0.0, 10.0, 10.0), Arc::new(|| {})));
        t.finalize();

        // Next frame the root only paints one child.
        t.start_frame();
        let _a = t.slot(RenderTree::ROOT, true);
        t.finalize();

        assert!(t.hit_test(25.0, 5.0).0.is_none(), "removed child left a ghost hit");
    }

    #[test]
    fn inspect_reports_parent_child_rect_and_tag() {
        let mut t = RenderTree::new();
        t.start_frame();
        let a = t.slot(RenderTree::ROOT, true);
        t.node_mut(a).tag = "Container";
        t.node_mut(a).cached_rect = Some(rect(0.0, 0.0, 100.0, 50.0));
        t.node_mut(a).cached_size = Some(Size { width: 100.0, height: 50.0 });
        t.finalize();

        let snap = t.inspect();
        assert_eq!(snap.len(), 2, "root + one child");
        let root = snap.iter().find(|n| n.id == RenderTree::ROOT).unwrap();
        assert_eq!(root.parent, None);
        assert_eq!(root.children, vec![a]);

        let child = snap.iter().find(|n| n.id == a).unwrap();
        assert_eq!(child.parent, Some(RenderTree::ROOT));
        assert_eq!(child.tag, "Container");
        assert_eq!(child.rect.map(|r| (r.origin.x, r.size.width)), Some((0.0, 100.0)));
        assert_eq!(child.size, Some(Size { width: 100.0, height: 50.0 }));
    }

    #[test]
    fn inspect_omits_nodes_dropped_by_finalize() {
        let mut t = RenderTree::new();
        t.start_frame();
        let a = t.slot(RenderTree::ROOT, true);
        let _b = t.slot(RenderTree::ROOT, true);
        t.finalize();
        assert_eq!(t.inspect().len(), 3, "root + a + b");

        // Next frame only paints `a` — `b`'s slot is dropped by finalize.
        t.start_frame();
        let _a2 = t.slot(RenderTree::ROOT, true);
        t.finalize();

        let snap = t.inspect();
        assert_eq!(snap.len(), 2, "root + a only — the orphaned slot must not appear");
        assert!(snap.iter().any(|n| n.id == a));
    }

    #[test]
    fn inspect_surfaces_semantics_and_interaction_flags() {
        use rosace_core::Role;
        let mut t = RenderTree::new();
        t.start_frame();
        let btn = t.slot(RenderTree::ROOT, true);
        t.node_mut(btn).semantics.push(super::super::SemanticsProps::new(Role::Button).label("Save"));
        t.node_mut(btn).hits.push((rect(0.0, 0.0, 10.0, 10.0), Arc::new(|| {})));
        t.node_mut(btn).hovered = true;
        t.finalize();

        let snap = t.inspect();
        let node = snap.iter().find(|n| n.id == btn).unwrap();
        assert_eq!(node.semantics, vec![(Role::Button, Some("Save".to_string()))]);
        assert_eq!(node.hit_count, 1);
        assert!(node.hovered);
        assert!(!node.pressed);
    }

    #[test]
    fn pick_finds_the_deepest_node_containing_the_point() {
        let mut t = RenderTree::new();
        t.start_frame();
        t.node_mut(RenderTree::ROOT).cached_rect = Some(rect(0.0, 0.0, 200.0, 200.0));
        let outer = t.slot(RenderTree::ROOT, true);
        t.node_mut(outer).cached_rect = Some(rect(0.0, 0.0, 100.0, 100.0));
        let inner = t.slot(outer, true);
        t.node_mut(inner).cached_rect = Some(rect(10.0, 10.0, 30.0, 30.0));
        t.finalize();

        // Inside the inner rect: must pick the deepest (most specific) node.
        assert_eq!(t.pick(15.0, 15.0), Some(inner));
        // Inside outer but outside inner: picks outer.
        assert_eq!(t.pick(50.0, 50.0), Some(outer));
        // Inside root but outside everything else: picks root.
        assert_eq!(t.pick(150.0, 150.0), Some(RenderTree::ROOT));
        // Outside all rects: nothing.
        assert_eq!(t.pick(-5.0, -5.0), None);
    }

    /// A barrier must stop EVERY pointer walk, not just clicks.
    ///
    /// `AbsorbPointer`'s whole purpose is to be a modal barrier, and
    /// `pointer_mode == 2` was checked only in `hit_test_node` — so hover,
    /// long-press, scroll and click-to-edit all reached the content behind
    /// it. One test per walk, because each is a separate recursion.
    #[test]
    fn absorb_pointer_blocks_every_pointer_walk_not_just_clicks() {
        let mut t = RenderTree::new();
        t.start_frame();

        // Content behind the barrier, registered on all five paths.
        let behind = t.slot(RenderTree::ROOT, true);
        let r = rect(0.0, 0.0, 100.0, 100.0);
        t.node_mut(behind).cached_rect = Some(r);
        t.node_mut(behind).hits.push((r, Arc::new(|| {})));
        t.node_mut(behind).hover_regions.push(r);
        t.node_mut(behind).long_hits.push((r, Arc::new(|| {})));
        t.node_mut(behind).editable = Some(crate::tree::text_edit::EditableDecl {
            value: String::new(),
            rect: r,
            multiline: false,
            obscure: false,
            on_change: Arc::new(|_| {}),
            controller: None,
            layout: Default::default(),
            filters: Vec::new(),
        });
        t.node_mut(behind).scrolls.push((r, ScrollAxes::BOTH, Arc::new(|_, _| {})));

        // The barrier, painted after it so it sits on top.
        let barrier = t.slot(RenderTree::ROOT, true);
        t.node_mut(barrier).pointer_mode = 2;
        t.node_mut(barrier).cached_rect = Some(rect(0.0, 0.0, 100.0, 100.0));
        t.finalize();

        assert!(t.hit_test(50.0, 50.0).0.is_some(), "click is absorbed, not passed through");
        assert_eq!(t.hover_test(50.0, 50.0), None, "hover must not reach content behind");
        assert!(t.long_press_test(50.0, 50.0).is_none(), "long-press must not reach behind");
        assert!(t.editable_test(50.0, 50.0).is_none(), "click-to-edit must not reach behind");
        assert!(t.scroll_test(50.0, 50.0, 0.0, -10.0).is_none(), "scroll must not reach behind");
    }

    /// The barrier is bounded by its rect — outside it, everything works.
    #[test]
    fn absorb_pointer_only_blocks_inside_its_own_rect() {
        let mut t = RenderTree::new();
        t.start_frame();

        let behind = t.slot(RenderTree::ROOT, true);
        let r = rect(0.0, 0.0, 200.0, 200.0);
        t.node_mut(behind).cached_rect = Some(r);
        t.node_mut(behind).hover_regions.push(r);

        let barrier = t.slot(RenderTree::ROOT, true);
        t.node_mut(barrier).pointer_mode = 2;
        t.node_mut(barrier).cached_rect = Some(rect(0.0, 0.0, 50.0, 50.0));
        t.finalize();

        assert_eq!(t.hover_test(25.0, 25.0), None, "inside the barrier: blocked");
        assert_eq!(t.hover_test(150.0, 150.0), Some(behind), "outside it: normal");
    }

    /// `IgnorePointer` is transparent, not a barrier: its own subtree never
    /// matches, but a sibling behind it still does. Scroll was missing this
    /// check entirely — `scroll_candidates` consulted neither mode.
    #[test]
    fn ignore_pointer_is_transparent_to_scroll_but_its_subtree_is_not() {
        let mut t = RenderTree::new();
        t.start_frame();

        let r = rect(0.0, 0.0, 100.0, 100.0);
        let behind = t.slot(RenderTree::ROOT, true);
        t.node_mut(behind).cached_rect = Some(r);
        t.node_mut(behind).scrolls.push((r, ScrollAxes::BOTH, Arc::new(|_, _| {})));

        let ignored = t.slot(RenderTree::ROOT, true);
        t.node_mut(ignored).pointer_mode = 1;
        t.node_mut(ignored).cached_rect = Some(r);
        t.node_mut(ignored).scrolls.push((r, ScrollAxes::BOTH, Arc::new(|_, _| {})));
        t.finalize();

        assert!(
            t.scroll_test(50.0, 50.0, 0.0, -10.0).is_some(),
            "the ignored node's own scroll target is skipped, but the one behind it still wins"
        );
    }

}
