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
    /// Who owns this node. Needed to propagate a repaint upward: a dirty
    /// nested node is invisible unless its ancestors re-assemble, because a
    /// clean parent replays a cached picture that still holds the child's OLD
    /// commands.
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// Child slot cursor for the current paint of this node.
    cursor: usize,
    /// Child slot cursor for the current LAYOUT of this node.
    ///
    /// Separate from `cursor` because layout and paint are two independent
    /// top-down walks over the same children: layout runs first for the whole
    /// tree, then paint. Sharing one cursor would have layout consume slots
    /// 0..n and paint then start at n, allocating a second set of nodes.
    ///
    /// Both walks visit a widget's children in the same order — the order its
    /// `layout`/`paint` call them — so the same index resolves to the same
    /// node. Widgets that lay out a different set than they paint would break
    /// that; `paint_child`'s debug assertion catches it. (Verified for the
    /// virtualized case: `ListView::layout` lays out no children at all and
    /// does build/layout/paint together inside `paint`.)
    layout_cursor: usize,
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
    /// Arbitrary widget-owned state declared through
    /// [`super::PaintCtx::widget_state`] — this framework's `remember`.
    ///
    /// Each entry is an `Arc<Mutex<T>>` boxed as `dyn Any`, so the handle
    /// handed to the widget and the copy retained here are the SAME cell: a
    /// handle captured into a click callback mutates what the next paint
    /// reads. (An `Arc<Mutex<dyn Any>>` cannot be downcast back to
    /// `Arc<Mutex<T>>`, which is why the `Arc` is inside the `Box`, not
    /// outside it.)
    ///
    /// Indexed positionally within a node's paint, the same shape as React's
    /// hooks and Compose's slot table — and with the same rule: call
    /// `widget_state` unconditionally, in a stable order.
    pub state: Vec<Box<dyn std::any::Any + Send>>,
    /// Position of the next [`super::PaintCtx::widget_state`] call in
    /// `state`. Reset by `begin`, so indices are per-paint.
    state_cursor: usize,
    /// How many `widget_state` calls the previous paint of this node made,
    /// for the debug-only stability check. `usize::MAX` = never painted.
    prev_state_count: usize,
    /// Persistent cursor/selection state for an editable node (D091/D112)
    /// — NOT cleared on repaint, so the caret survives a rebuild with the
    /// same displayed value.
    pub text_edit: super::text_edit::TextEditState,

    // ── Picture cache (Phase 20 unification — was the flat RenderNode) ───
    /// Widget type name at this position; a mismatch resets the caches.
    pub tag: &'static str,
    /// The tag the LAYOUT walk claimed for this slot this frame, or `""`.
    ///
    /// Layout and paint address children through separate cursors, on the
    /// assumption that both visit a widget's children in the same order. When
    /// a widget's `layout` measures a different set than its `paint` slots —
    /// `Accordion` paints chevron, header, body but measures only the body —
    /// the two disagree and a child silently inherits a sibling's cached size.
    ///
    /// So the layout walk records what it claimed, `paint_child` asserts it
    /// still agrees, and the mismatch becomes a loud dev-time failure instead
    /// of plausible-looking wrong output. Debug-only in effect: the field is
    /// written unconditionally (one pointer store) but only read by
    /// `debug_assert!`.
    pub layout_tag: &'static str,
    /// The frame `layout_tag` was written in.
    ///
    /// The tag alone is not enough to assert on. It is cleared by
    /// `paint_child`, so a child that LAYOUT measured but PAINT then chose not
    /// to paint — a conditional or early-returning `paint` — leaves its tag
    /// behind. On a later frame a different widget legitimately takes that
    /// slot and the stale tag accuses it of a misalignment that never
    /// happened. Found by running the showcase, which panicked on exactly
    /// this.
    ///
    /// A tag from a previous frame says nothing about THIS frame's layout, so
    /// the check applies only when the stamp is current. Same-frame
    /// misalignment — the real bug — is still caught.
    pub layout_tag_frame: u64,
    /// Constraints used for the last successful layout pass.
    pub last_constraints: Option<Constraints>,
    /// Size returned by the last layout pass.
    pub cached_size: Option<Size>,
    /// Display list from the last paint pass.
    pub cached_picture: Option<Arc<Picture>>,
    /// World-space rect of the last paint (also the damage extent).
    pub cached_rect: Option<Rect>,
    /// This node's `cached_size` is stale — re-run `layout`.
    ///
    /// Separate from [`Self::needs_paint`] because the two have genuinely
    /// different causes, and one flag doing both jobs made every appearance
    /// change pay for a re-measure. A hover is the clearest case: it changes
    /// how a widget LOOKS and cannot change how big it is.
    ///
    /// That last claim is structural, not a convention to be careful about.
    /// `LayoutCtx` does now carry the tree and this node (it must, to cache
    /// per node), but it exposes only `layout_child`: `hovered()`, `pressed()`
    /// and `animate_to()` live on `PaintCtx` alone. So `layout` still cannot
    /// observe interaction state even if a widget wanted to — the guarantee
    /// moved from "the field is absent" to "the API does not expose it", which
    /// is how Flutter and Compose both draw the same line. Size remains a
    /// function of (widget config, constraints, font, theme) and nothing else,
    /// so only those can invalidate it.
    pub needs_layout: bool,
    /// The subtree a `Stateful` widget last built, kept so `layout` and
    /// `paint` share ONE build per frame rather than running the closure
    /// twice — the mistake `9b4ba78` just removed from Row/Column.
    /// `Arc`, not `Box`, because both `layout` and `paint` need this subtree
    /// while the tree itself is behind a `RefCell`. A `Box` cannot be cloned
    /// out, so using one would mean holding a borrow across a call that
    /// re-borrows the tree mutably — a guaranteed panic. `Widget` already
    /// requires `Send + Sync`, so an `Arc` clones cheaply and lets the borrow
    /// drop immediately.
    pub built: Option<Arc<dyn super::Widget>>,
    /// Whether a `StatefulWidget` at this node has already been mounted.
    ///
    /// Tree membership, not object construction: the widget OBJECT is rebuilt
    /// on every structural frame, so `on_mount` keyed to a fresh object fired
    /// once per rebuild and `on_dispose` handlers stacked up one per rebuild.
    /// Keyed to the node, both happen exactly once per stay in the tree.
    ///
    /// Cleared by `adopt_tag` (a different widget type took this slot) and by
    /// `dispose_node_contents` replacing the node wholesale — so both removal
    /// paths reset it for free, and a widget that leaves and returns is
    /// correctly a new mount.
    pub mounted: bool,
    /// Callbacks to run when this node leaves the tree. See
    /// [`RenderTree::dispose_subtree`].
    pub dispose: Vec<Box<dyn FnOnce() + Send>>,
    /// This widget asked for another frame from inside its own `paint`
    /// (a spinner, a shimmer). Such a node must never replay its cached
    /// picture: replaying skips the request, and the animation stops.
    pub self_animating: bool,
    /// This node's `cached_picture` is stale — re-run `paint`.
    ///
    /// A superset of [`Self::needs_layout`] in practice: anything that
    /// changes size also changes appearance, so re-running layout always sets
    /// this too. The reverse does not hold, and that asymmetry is the point.
    pub needs_paint: bool,

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

    /// Claim `node` for a widget of type `tag`, discarding everything the
    /// PREVIOUS occupant left behind if the type changed. Returns true when a
    /// reset happened.
    ///
    /// # Why a slot needs a type
    ///
    /// Node identity is positional: `slot()` hands out the child at the
    /// parent's cursor. That is the right default — it keeps a widget's scroll
    /// position and edit buffer across repaints without anyone declaring a key.
    /// It is wrong the moment a slot changes what KIND of widget lives there:
    ///
    /// ```ignore
    /// if flag { col.child(Button) } else { col.child(TextField) }
    /// ```
    ///
    /// Both branches paint one child, so the slot is reused and `finalize`'s
    /// truncate never fires. Without a type check the `TextField`'s edit
    /// buffer, focus and press state survive into the `Button` — and, once
    /// nodes cache pictures, so does its appearance.
    ///
    /// `walk_element` has always done this for element boundaries
    /// (`rosace/src/lib.rs`). Nested nodes never had it: `PaintCtx::child`
    /// writes only `cached_rect`, so every node inside a component's widget
    /// tree carried `tag == ""` and could not tell one widget from another.
    ///
    /// # What is NOT reset
    ///
    /// Per-paint declarations (hits, semantics, overlays, scroll regions) are
    /// already cleared by `begin`, so they are not touched here. This clears
    /// only what PERSISTS across paints and would otherwise leak between two
    /// unrelated widgets.
    pub fn adopt_tag(&mut self, node: NodeId, tag: &'static str) -> bool {
        let n = &mut self.nodes[node];
        if n.tag == tag {
            return false;
        }
        // The previous occupant is leaving this slot. Its cleanup has to run
        // here as well as in `finalize`: a branch whose child COUNT is
        // unchanged (`if flag { Button } else { TextField }`) never reaches
        // truncate, so this is the ONLY signal that a widget went away.
        // Missing it would drop a subscription without cancelling it.
        for f in std::mem::take(&mut n.dispose) {
            f();
        }
        n.built = None;
        // A different widget type in this slot is a different widget: whatever
        // was here has left the tree, and whatever arrives has not mounted.
        n.mounted = false;

        // A brand-new node has tag "" and is being claimed for the first time.
        // Clearing default-valued fields is a no-op, so this costs nothing and
        // keeps one code path instead of two.
        let n = &mut self.nodes[node];
        n.tag = tag;

        // Caches — same set walk_element clears on a mismatch.
        n.last_constraints = None;
        n.cached_size = None;
        n.cached_picture = None;
        n.cached_rect = None;
        n.needs_layout = true;
        n.needs_paint = true;
        n.self_animating = false;

        // Persistent widget state. A different widget type must never inherit
        // these: an edit buffer belonging to a TextField appearing inside a
        // Button is a data leak, not just a visual bug.
        n.text_edit = Default::default();
        // Widget-owned state, same reasoning as the edit buffer directly
        // above: a `Button` must never be handed the state a `TextField` left
        // in this slot. `widget_state`'s downcast would reject the type
        // anyway, but by then it is a panic rather than a fresh start.
        n.state.clear();
        n.prev_state_count = usize::MAX;
        n.scroll_ctrl = None;
        n.focus_node = None;
        n.anim = None;
        n.anim_channels.clear();

        // Interaction state is dispatcher-owned and refers to the widget that
        // was there. Leaving it set would paint the new widget pre-hovered or
        // stuck pressed.
        n.hovered = false;
        n.pressed = false;
        true
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
        n.state_cursor = 0;
        n.begun = true;
        // NOT reset here. `begin` runs during PAINT, which happens after the
        // whole layout walk; zeroing the layout cursor here would be harmless
        // but misleading. `layout_child` resets its child's cursor before
        // descending, exactly as `paint_child` relies on `slot(reset: true)`.
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
            self.nodes[id].parent = Some(parent);
            self.nodes[parent].children.push(id);
            id
        };

        if reset {
            self.begin(child);
        }
        child
    }

    /// Consume the next child slot of `parent` for the LAYOUT walk.
    ///
    /// The layout mirror of [`Self::slot`], driving `layout_cursor` instead of
    /// `cursor` so the two walks cannot consume each other's slots. It never
    /// calls `begin`: layout declares no hits, semantics or scroll regions, so
    /// there is nothing to clear, and clearing would throw away declarations
    /// the previous paint made and this frame's paint may replay.
    pub fn layout_slot(&mut self, parent: NodeId) -> NodeId {
        let cursor = self.nodes[parent].layout_cursor;
        self.nodes[parent].layout_cursor += 1;

        if cursor < self.nodes[parent].children.len() {
            self.nodes[parent].children[cursor]
        } else {
            let id = self.nodes.len();
            self.nodes.push(TreeNode::default());
            self.nodes[id].parent = Some(parent);
            self.nodes[parent].children.push(id);
            id
        }
    }

    /// The node `slot` would return next, WITHOUT consuming it.
    ///
    /// For measuring a child during `paint`: the widget needs the child's size
    /// to compute its rect, then calls `paint_child(rect, child)` which
    /// consumes this same slot. Peek-then-consume means both address one node,
    /// so the measurement caches where the paint will look for it.
    ///
    /// Creates the node when the slot does not exist yet (first frame), so the
    /// subsequent `slot` finds it rather than appending a second one.
    pub fn peek_slot(&mut self, parent: NodeId) -> NodeId {
        let cursor = self.nodes[parent].cursor;
        if cursor < self.nodes[parent].children.len() {
            self.nodes[parent].children[cursor]
        } else {
            let id = self.nodes.len();
            self.nodes.push(TreeNode::default());
            self.nodes[id].parent = Some(parent);
            self.nodes[parent].children.push(id);
            id
        }
    }

    /// `parent`'s child at an EXPLICIT index, creating slots up to it.
    ///
    /// For containers whose `layout` makes more than one pass over their
    /// children and does not touch every child on every pass — `Row`/`Column`
    /// measure the non-flex children first, then the flex ones. A cursor would
    /// hand those passes slots in skipping order (`[flex, fixed, fixed]` gives
    /// the flex child slot 2 while paint gives it slot 0), silently aliasing
    /// each child onto a sibling's cached size.
    ///
    /// Addressing by the loop index is exact and pass-order independent, and
    /// it matches paint by construction: `paint_child` is called once per
    /// child in the same order, so `children[i]` is the same node either way.
    pub fn layout_slot_at(&mut self, parent: NodeId, index: usize) -> NodeId {
        while self.nodes[parent].children.len() <= index {
            let id = self.nodes.len();
            self.nodes.push(TreeNode::default());
            self.nodes[id].parent = Some(parent);
            self.nodes[parent].children.push(id);
        }
        self.nodes[parent].children[index]
    }

    /// Mark `node` mounted, returning true only the FIRST time.
    ///
    /// The one place `on_mount` and the `on_dispose` registration should be
    /// gated on — see [`TreeNode::mounted`] for why the node rather than the
    /// widget object is the right key.
    pub fn mark_mounted(&mut self, node: NodeId) -> bool {
        let n = &mut self.nodes[node];
        let first = !n.mounted;
        n.mounted = true;
        first
    }

    /// The subtree a `StatefulWidget` at `node` last built, if any.
    pub fn built(&self, node: NodeId) -> Option<Arc<dyn super::Widget>> {
        self.nodes[node].built.clone()
    }

    /// Store the subtree a `StatefulWidget` at `node` just built.
    pub fn set_built(&mut self, node: NodeId, w: Arc<dyn super::Widget>) {
        self.nodes[node].built = Some(w);
    }

    /// Reserve `node`'s next positional `widget_state` slot.
    pub fn next_state_slot(&mut self, node: NodeId) -> usize {
        let n = &mut self.nodes[node];
        let i = n.state_cursor;
        n.state_cursor += 1;
        i
    }

    /// Close a node's state scope at the end of its paint.
    ///
    /// Returns `Some((previous, now))` when the count CHANGED from the last
    /// paint, which means `widget_state` was called conditionally — React's
    /// hook-ordering footgun, where every later index silently shifts by one
    /// and each state handle starts addressing its neighbour's value. Callers
    /// turn that into a debug-only panic; in release the count is just
    /// recorded.
    pub fn close_state_scope(&mut self, node: NodeId) -> Option<(usize, usize)> {
        let n = &mut self.nodes[node];
        let now = n.state_cursor;
        let prev = std::mem::replace(&mut n.prev_state_count, now);
        if prev != usize::MAX && prev != now { Some((prev, now)) } else { None }
    }

    /// Reset `node`'s layout cursor so its children are addressed from 0.
    ///
    /// Called before a widget's `layout` runs — by `LayoutCtx::layout_child`
    /// for nested widgets, and by `walk_element` for the element boundary,
    /// which is where the layout walk enters the tree.
    pub fn begin_layout(&mut self, node: NodeId) {
        self.nodes[node].layout_cursor = 0;
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
                // The parent link is NOT optional, and its absence was a real
                // bug: `mark_dirty_with_ancestors` walks `parent` upward, so a
                // keyed child with `parent: None` stopped the walk dead at
                // itself. Every SCREEN is a keyed child, so scrolling inside
                // any screen marked the scroll node and nothing above it —
                // `ScreenTransitionView`, `Scaffold` and the arena root stayed
                // clean, the walk returned their caches without descending,
                // and the scroll never reached the screen.
                //
                // It was invisible while scrolling dirtied the whole COMPONENT,
                // because every frame was then structural and repainted
                // everything regardless of parent links. Precise invalidation
                // is what exposed it. `slot()` has always set this.
                self.nodes[id].parent = Some(parent);
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
        let mut removed: Vec<NodeId> = Vec::new();
        for i in 0..self.begun_this_frame.len() {
            let id = self.begun_this_frame[i];
            let cursor = self.nodes[id].cursor;

            // Children past the cursor were not slotted this frame. `truncate`
            // used to drop these ids on the floor; they are the structural
            // signal that a widget was removed, which nothing was listening to.
            // Draining is exact — O(removed), not O(all nodes) like the set
            // difference components have to use.
            let dropped: Vec<NodeId> = if cursor < self.nodes[id].children.len() {
                self.nodes[id].children.drain(cursor..).collect()
            } else {
                Vec::new()
            };

            // What actually survived: the children slotted this frame.
            let live: std::collections::HashSet<NodeId> =
                self.nodes[id].children.iter().copied().collect();

            // A dropped id is NOT necessarily gone. `keyed_slot` writes its
            // node OVER `children[cursor]` rather than appending, so a parent
            // that painted two keyed children and then one leaves a stale
            // DUPLICATE past the cursor: `[A, B]` becomes `[B, B]` when B
            // survives and A does not. Disposing that duplicate destroys a
            // live widget — a settling screen transition went blank.
            removed.extend(dropped.into_iter().filter(|c| !live.contains(c)));

            // KEYED children are deliberately NOT disposed when they are not
            // painted. That is the entire purpose of `child_keyed`: a screen
            // that has been navigated away from keeps its scroll position and
            // animation state so the round trip restores it, rather than
            // aliasing onto whoever next occupies the slot. Pruning them here
            // reset Screen A's scroll to the top on the way back.
            //
            // They are released when their PARENT is disposed —
            // `dispose_subtree` walks `keyed_children` for exactly that.
        }
        removed.sort_unstable();
        removed.dedup();
        for id in removed {
            self.dispose_subtree(id);
        }
    }

    /// Run every `on_dispose` in this subtree and release what it held.
    ///
    /// # Depth-first, children before parents
    ///
    /// A parent's cleanup must not run while its children still hold
    /// references to what it is tearing down — the same ordering Flutter and
    /// React use, and for the same reason.
    ///
    /// # The slot is emptied, not reused
    ///
    /// Node ids ESCAPE the frame: `node_rect`'s own documentation records that
    /// an accessibility action names a node from the tree published last
    /// frame. Recycling ids through a free list would let such a stale id
    /// address a DIFFERENT widget — an a11y action activating the wrong
    /// button, which is worse than the leak it would fix.
    ///
    /// So this frees the CONTENTS — state, caches, pictures, handlers, which
    /// is essentially all the memory — and leaves an empty slot behind, so a
    /// stale id resolves to something inert rather than to somebody else.
    /// Reusing slots needs a generation counter on `NodeId`; that is a wider
    /// change and deliberately not bundled here.
    pub fn dispose_subtree(&mut self, node: NodeId) {
        let children = std::mem::take(&mut self.nodes[node].children);
        for c in children {
            self.dispose_subtree(c);
        }
        // Keyed children survive frames they are not painted in, so they are
        // not reachable through `children` — but when the PARENT goes, they go
        // with it, or the retention that makes them useful becomes a leak.
        let keyed: Vec<NodeId> = std::mem::take(&mut self.nodes[node].keyed_children)
            .into_values()
            .collect();
        for c in keyed {
            self.dispose_subtree(c);
        }
        self.dispose_node_contents(node);
    }

    /// Fire this ONE node's dispose callbacks and drop everything it owns.
    /// Callers handle the recursion; see [`Self::dispose_subtree`].
    fn dispose_node_contents(&mut self, node: NodeId) {
        // Before the callbacks: a widget that observed the app lifecycle must
        // stop receiving phases the moment it leaves the tree, or a backgrounded
        // app would still be calling into widgets that are gone.
        super::unregister_lifecycle_observer(node);
        for f in std::mem::take(&mut self.nodes[node].dispose) {
            f();
        }
        // Replacing with a default is how the memory is actually reclaimed:
        // cached pictures and widget state dominate a node's footprint, and
        // an empty TreeNode is small.
        self.nodes[node] = TreeNode::default();
    }

    /// Like [`Self::node_mut`], but tolerates an id that no longer exists.
    ///
    /// A handle can outlive its widget: a callback fires, marks its node, and
    /// the widget is removed from the tree before the next frame runs. The
    /// mark is then meaningless rather than an error — same reasoning as
    /// [`Self::node_rect`]'s bounds check.
    pub fn node_mut_checked(&mut self, id: NodeId) -> Option<&mut TreeNode> {
        self.nodes.get_mut(id)
    }

    /// Mark `node` for a full update, and every ancestor for re-assembly.
    ///
    /// Three things propagate three different distances, and conflating them
    /// is what made an earlier draft of this claim "the parent is untouched":
    ///
    /// * LAYOUT stops at `node`. Whether anything above must re-measure is
    ///   decided by comparing the new size against `cached_size`, not assumed.
    /// * ASSEMBLY reaches the root, always. `Picture` is a flat
    ///   `Vec<DrawCommand>` with no nested sub-pictures, so an ancestor that
    ///   replays its own cache would replay the child's OLD commands and the
    ///   change would never appear on screen.
    /// * RASTERIZATION is damage-scoped, handled elsewhere.
    ///
    /// Ancestors therefore get `needs_paint` only. They re-run `paint`, which
    /// is their own background plus `paint_child` calls — the siblings still
    /// replay, which is the entire saving.
    pub fn mark_dirty_with_ancestors(&mut self, node: NodeId) {
        if self.nodes.get(node).is_none() {
            return;
        }
        self.nodes[node].needs_layout = true;
        self.nodes[node].needs_paint = true;
        let mut cur = self.nodes[node].parent;
        while let Some(p) = cur {
            self.nodes[p].needs_paint = true;
            // ALSO re-measure. A state change can change SIZE, and the
            // framework cannot know whether it did without measuring — that
            // depends on font metrics, text scale and the widget's internals.
            //
            // Without this, a parent keeps the child's old size and positions
            // it at the old rect: a grandchild that grew silently stayed at
            // its old height, clipped, with no error to notice. Re-measuring
            // the spine is cheap; the saving that matters — siblings replaying
            // their pictures instead of repainting — is untouched, because
            // needs_paint is what governs that and only the marked node and
            // its ancestors have it.
            self.nodes[p].needs_layout = true;
            cur = self.nodes[p].parent;
        }
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut TreeNode {
        &mut self.nodes[id]
    }

    /// The painted rect of a node, if it exists and was painted.
    ///
    /// Bounds-checked: callers can hold a node id from a PREVIOUS frame — an
    /// accessibility action names a node from the tree that was published
    /// last frame, and the tree may have shrunk since. Indexing directly
    /// would panic on a list that got shorter between the announcement and
    /// the user acting on it.
    pub fn node_rect(&self, id: NodeId) -> Option<Rect> {
        self.nodes.get(id).and_then(|n| n.cached_rect)
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
            self.nodes[old].needs_paint = true;
        }
        if let Some(new) = target {
            self.nodes[new].hovered = true;
            self.nodes[new].needs_paint = true;
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
            self.nodes[old].needs_paint = true;
        }
        if let Some(new) = target {
            self.nodes[new].pressed = true;
            self.nodes[new].needs_paint = true;
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


    /// A slot that changes WIDGET TYPE must not hand the newcomer the old
    /// widget's state.
    ///
    /// The shape that produces this is a branch whose child COUNT is
    /// unchanged — `if flag { Button } else { TextField }`. One child either
    /// way, so the slot is reused and `finalize`'s truncate never fires.
    /// Without a type check the TextField's edit buffer, focus and press
    /// state survive into the Button.
    ///
    /// Asserted on state rather than on `tag` because the tag is the
    /// mechanism, not the promise. What must hold is that nothing leaks.
    #[test]
    fn a_slot_changing_widget_type_does_not_inherit_the_old_widgets_state() {
        let mut t = RenderTree::new();
        t.start_frame();
        let n = t.slot(RenderTree::ROOT, true);

        // Frame 1: a text field lives here, with real state on it.
        t.adopt_tag(n, "TextField");
        t.node_mut(n).text_edit.scroll_x = 42.0;
        t.node_mut(n).text_edit.selection = Default::default();
        t.node_mut(n).pressed = true;
        t.node_mut(n).hovered = true;
        t.node_mut(n).cached_size = Some(Size { width: 10.0, height: 10.0 });

        // Frame 2: the branch flipped and a button occupies the same slot.
        let reset = t.adopt_tag(n, "Button");
        assert!(reset, "a type change must report that it reset the slot");

        let node = t.node(n);
        assert_eq!(node.text_edit.scroll_x, 0.0,
            "the previous widget's edit state leaked into a different widget");
        assert!(!node.pressed, "stale press state would paint the button held down");
        assert!(!node.hovered, "stale hover state would paint the button pre-hovered");
        assert!(node.cached_size.is_none(), "a stale size would misplace the new widget");
        assert!(node.needs_paint, "the new widget must be painted, not replayed");
        assert!(node.needs_layout, "a different widget's size must be re-measured");
    }

    /// The same widget type re-occupying its slot must KEEP its state — that
    /// is the whole point of positional identity, and the common case by far.
    /// A type check that also reset on a match would throw away every scroll
    /// position and caret on every frame.
    #[test]
    fn a_slot_keeping_its_widget_type_keeps_its_state() {
        let mut t = RenderTree::new();
        t.start_frame();
        let n = t.slot(RenderTree::ROOT, true);

        t.adopt_tag(n, "TextField");
        t.node_mut(n).text_edit.scroll_x = 7.0;

        let reset = t.adopt_tag(n, "TextField");
        assert!(!reset, "an unchanged type must not report a reset");
        assert_eq!(t.node(n).text_edit.scroll_x, 7.0,
            "repainting the same widget threw away its state");
    }


    /// A hover change must invalidate APPEARANCE ONLY.
    ///
    /// One flag used to do both jobs, so moving the mouse re-measured every
    /// widget it touched. That is pure waste: `LayoutCtx` carries only
    /// constraints, font and theme, so `layout` has no way to observe hover
    /// even if a widget wanted it to. Size cannot change, so it must not be
    /// recomputed.
    #[test]
    fn hovering_invalidates_the_picture_but_never_the_size() {
        let mut t = RenderTree::new();
        t.start_frame();
        let n = t.slot(RenderTree::ROOT, true);
        t.adopt_tag(n, "Button");

        // Settle the node as a fully cached, clean one.
        t.node_mut(n).cached_size = Some(Size { width: 80.0, height: 24.0 });
        t.node_mut(n).needs_layout = false;
        t.node_mut(n).needs_paint = false;

        assert!(t.set_hover(Some(n)), "the hover target changed");

        assert!(t.node(n).needs_paint, "a hover must re-record the picture");
        assert!(!t.node(n).needs_layout,
            "a hover re-measured the widget — layout cannot see hover, so this is pure waste");
        assert_eq!(t.node(n).cached_size, Some(Size { width: 80.0, height: 24.0 }),
            "the cached size must survive a hover");
    }

    /// Same for press, which shares the mechanism.
    #[test]
    fn pressing_invalidates_the_picture_but_never_the_size() {
        let mut t = RenderTree::new();
        t.start_frame();
        let n = t.slot(RenderTree::ROOT, true);
        t.adopt_tag(n, "Button");
        t.node_mut(n).needs_layout = false;
        t.node_mut(n).needs_paint = false;

        assert!(t.set_pressed(Some(n)));
        assert!(t.node(n).needs_paint, "a press must re-record the picture");
        assert!(!t.node(n).needs_layout, "a press cannot change a widget's size");
    }


    /// A keyed child that SURVIVES must not be disposed just because a stale
    /// duplicate of it sat past the cursor.
    ///
    /// `keyed_slot` writes its node over `children[cursor]` instead of
    /// appending, so a parent that painted two keyed children and then one
    /// leaves `[B, B]` behind. Truncating dropped the duplicate harmlessly;
    /// draining and disposing it destroyed a live widget — a settling screen
    /// transition went blank. Pinned because the failure is spectacular and
    /// the cause is one line away from looking correct.
    #[test]
    fn a_surviving_keyed_child_is_not_disposed_by_its_own_stale_duplicate() {
        let mut t = RenderTree::new();

        // Frame 1: two keyed children, A then B.
        t.start_frame();
        let a = t.keyed_slot(RenderTree::ROOT, 1);
        let b = t.keyed_slot(RenderTree::ROOT, 2);
        t.node_mut(a).cached_size = Some(Size { width: 1.0, height: 1.0 });
        t.node_mut(b).cached_size = Some(Size { width: 2.0, height: 2.0 });
        t.finalize();
        assert_ne!(a, b);

        // Frame 2: the transition settled — only B is painted.
        t.start_frame();
        let b2 = t.keyed_slot(RenderTree::ROOT, 2);
        assert_eq!(b2, b, "the keyed node must be reused, not recreated");
        t.finalize();

        assert_eq!(
            t.node(b).cached_size, Some(Size { width: 2.0, height: 2.0 }),
            "the surviving screen was disposed — its stale duplicate past the cursor \
             was mistaken for a removal"
        );
        // A is NOT disposed: keyed children persist across frames they are not
        // painted in, which is what lets a screen keep its scroll position
        // while navigated away.
        assert_eq!(t.node(a).cached_size, Some(Size { width: 1.0, height: 1.0 }),
            "a keyed child must survive not being painted — that is what keys are for");
    }

    /// A keyed child that was not painted for a while must come back with its
    /// state intact — a screen navigated away from and returned to.
    #[test]
    fn a_keyed_child_returns_with_its_state_after_being_unpainted() {
        let mut t = RenderTree::new();
        t.start_frame();
        let a = t.keyed_slot(RenderTree::ROOT, 1);
        let _b = t.keyed_slot(RenderTree::ROOT, 2);
        t.node_mut(a).cached_size = Some(Size { width: 9.0, height: 9.0 });
        t.finalize();

        // Navigate away: only B paints, for several frames.
        for _ in 0..3 {
            t.start_frame();
            let _b2 = t.keyed_slot(RenderTree::ROOT, 2);
            t.finalize();
        }

        // Navigate back.
        t.start_frame();
        let a_again = t.keyed_slot(RenderTree::ROOT, 1);
        t.finalize();

        assert_eq!(a_again, a, "the key must resolve to the same node");
        assert_eq!(t.node(a).cached_size, Some(Size { width: 9.0, height: 9.0 }),
            "the returning screen lost its state — this is the scroll-position \
             regression child_keyed exists to prevent");
    }


    /// The slot-misalignment assertion must still FIRE on a real, same-frame
    /// disagreement between the layout walk and the paint walk.
    ///
    /// This exists because the assertion was weakened after it had done its
    /// job: it originally compared a `layout_tag` that only `paint_child`
    /// cleared, so a child measured-but-not-painted left a stale tag that
    /// accused an innocent widget on a LATER frame (it crashed the showcase).
    /// Scoping the check to the current frame fixed that — and removed every
    /// case that was proving the check worked, since the widgets which used to
    /// trip it were moved to the uncached path.
    ///
    /// A net that was loosened and never re-tested is a net that has quietly
    /// stopped catching things. This is the re-test.
    #[test]
    #[should_panic(expected = "slot misalignment")]
    fn a_same_frame_layout_paint_disagreement_still_panics() {
        use crate::tree::{LayoutCtx, PaintCtx, Widget};
        use rosace_core::types::{Point, Rect, Size};
        use rosace_layout::Constraints;
        use std::{cell::RefCell, rc::Rc};

        struct Leaf(&'static str);
        impl Widget for Leaf {
            fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 10.0, height: 10.0 } }
            fn paint(&self, _ctx: &mut PaintCtx) {}
        }
        // Two distinct concrete types, so `type_tag()` differs.
        struct Other;
        impl Widget for Other {
            fn layout(&self, _c: &LayoutCtx) -> Size { Size { width: 10.0, height: 10.0 } }
            fn paint(&self, _ctx: &mut PaintCtx) {}
        }

        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let mut rec = rosace_render::PictureRecorder::new();

        super::super::begin_frame();
        let rect = Rect { origin: Point { x: 0.0, y: 0.0 },
                          size: Size { width: 50.0, height: 50.0 } };
        let mut ctx = PaintCtx::root(&mut rec, rect, &font, theme.clone(), Rc::clone(&tree));

        // LAYOUT claims slot 0 for `Leaf`...
        let lctx = LayoutCtx::with_tree(
            Constraints::loose(50.0, 50.0), &font, &theme,
            Rc::clone(&tree), RenderTree::ROOT,
        );
        let _ = lctx.layout_child(Constraints::loose(50.0, 50.0), &Leaf("a"));

        // ...and PAINT puts `Other` there, in the same frame.
        ctx.paint_child(rect, &Other);
    }

}
