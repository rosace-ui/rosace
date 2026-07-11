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

use std::sync::Arc;

use rosace_core::types::{Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Picture;

use super::overlay::OverlayEntry;
use super::TransformLayerEntry;

pub type NodeId = usize;

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

/// One render-tree node. Declared data is cleared when the node is repainted
/// (`begin`) and persists untouched otherwise.
#[derive(Default)]
pub struct TreeNode {
    pub children: Vec<NodeId>,
    /// Child slot cursor for the current paint of this node.
    cursor: usize,
    /// True if this node was begun (repainted) in the current frame.
    begun: bool,

    // ── Declared per-paint data (D091) ────────────────────────────────────
    pub hits:       Vec<HitRegion>,
    pub hits_at:    Vec<HitRegionAt>,
    pub scrolls:    Vec<ScrollRegion>,
    pub focus:      Vec<rosace_a11y::FocusNode>,
    pub overlays:   Vec<OverlayEntry>,
    pub transforms: Vec<TransformLayerEntry>,
    pub semantics:  Vec<super::Semantics>,

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
    pub scroll_ctrl: Option<rosace_scroll::ScrollController>,
    /// A persistent eased scalar (0..1) for toggle transitions — advanced by
    /// PaintCtx::animate_to. `None` until first observed (then snaps).
    pub anim: Option<f32>,
    /// This node's [`rosace_a11y::FocusNode`] (D112/Phase 28 Step 1) —
    /// created lazily by [`super::PaintCtx::focus_node`], survives
    /// rebuilds like `scroll_ctrl` above.
    pub focus_node: Option<rosace_a11y::FocusNode>,
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
        n.scrolls.clear();
        n.focus.clear();
        n.overlays.clear();
        n.transforms.clear();
        n.semantics.clear();
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

    // ── Derivations (D091/D092) ───────────────────────────────────────────

    /// Hit-test walk: children before own regions, later siblings first —
    /// paint order is z-order, so the topmost match wins structurally (D092).
    /// Returns the topmost hit callback and whether it is POSITIONAL —
    /// positional hits become the active drag grab (streamed MouseMove
    /// positions until release); plain hits fire once.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<(Arc<dyn Fn(f32, f32) + Send + Sync>, bool)> {
        self.hit_test_node(Self::ROOT, x, y)
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
        (x - vp.origin.x + off[0], y - vp.origin.y + off[1], false)
    }

    fn hit_test_node(&self, id: NodeId, x: f32, y: f32) -> Option<(Arc<dyn Fn(f32, f32) + Send + Sync>, bool)> {
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
        if !clipped {
            for &child in n.children.iter().rev() {
                if let Some((cb, positional)) = self.hit_test_node(child, cx, cy) {
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
                    let wrapped: Arc<dyn Fn(f32, f32) + Send + Sync> = match n.transforms.first() {
                        Some(entry) => {
                            let vp = entry.viewport_rect;
                            Arc::new(move |sx: f32, sy: f32| {
                                let off = rosace_state::scroll_offset(id as u64);
                                cb(sx - vp.origin.x + off[0], sy - vp.origin.y + off[1]);
                            })
                        }
                        None => cb,
                    };
                    return Some((wrapped, positional));
                }
            }
        }
        // Positional regions first within a node (more specific intent).
        for (rect, cb) in n.hits_at.iter().rev() {
            if contains(rect, x, y) {
                return Some((cb.clone(), true));
            }
        }
        for (rect, cb) in n.hits.iter().rev() {
            if contains(rect, x, y) {
                let cb = cb.clone();
                return Some((Arc::new(move |_, _| cb()), false));
            }
        }
        None
    }

    /// Topmost node under the cursor that owns any interactive or hover
    /// region — drives hover state (buttons, tiles, tooltips).
    pub fn hover_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.hover_test_node(Self::ROOT, x, y)
    }

    fn hover_test_node(&self, id: NodeId, x: f32, y: f32) -> Option<NodeId> {
        let n = &self.nodes[id];
        if n.pointer_mode == 1 {
            return None;
        }
        let (cx, cy, clipped) = self.child_coords(n, id, x, y);
        if !clipped {
            for &child in n.children.iter().rev() {
                if let Some(hit) = self.hover_test_node(child, cx, cy) {
                    return Some(hit);
                }
            }
        }
        let owns = n.hits.iter().map(|(r, _)| r)
            .chain(n.hits_at.iter().map(|(r, _)| r))
            .chain(n.long_hits.iter().map(|(r, _)| r))
            .chain(n.hover_regions.iter())
            .any(|r| contains(r, x, y));
        if owns { Some(id) } else { None }
    }

    /// Topmost long-press callback under the cursor.
    pub fn long_press_test(&self, x: f32, y: f32) -> Option<Arc<dyn Fn() + Send + Sync>> {
        self.long_press_node(Self::ROOT, x, y)
    }

    fn long_press_node(&self, id: NodeId, x: f32, y: f32) -> Option<Arc<dyn Fn() + Send + Sync>> {
        let n = &self.nodes[id];
        if n.pointer_mode == 1 {
            return None;
        }
        let (cx, cy, clipped) = self.child_coords(n, id, x, y);
        if !clipped {
            for &child in n.children.iter().rev() {
                if let Some(cb) = self.long_press_node(child, cx, cy) {
                    return Some(cb);
                }
            }
        }
        for (rect, cb) in n.long_hits.iter().rev() {
            if contains(rect, x, y) {
                return Some(cb.clone());
            }
        }
        None
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
        -> Option<Arc<dyn Fn(f32, f32) + Send + Sync>>
    {
        let mut candidates: Vec<(ScrollAxes, Arc<dyn Fn(f32, f32) + Send + Sync>)> = Vec::new();
        self.scroll_candidates(Self::ROOT, x, y, &mut candidates);
        select_scroll_handler(&candidates, dx, dy)
    }

    fn scroll_candidates(
        &self,
        id: NodeId,
        x: f32,
        y: f32,
        out: &mut Vec<(ScrollAxes, Arc<dyn Fn(f32, f32) + Send + Sync>)>,
    ) {
        let n = &self.nodes[id];
        // Children first (topmost/innermost priority), later siblings first.
        for &child in n.children.iter().rev() {
            self.scroll_candidates(child, x, y, out);
        }
        for (rect, axes, cb) in n.scrolls.iter().rev() {
            if contains(rect, x, y) {
                out.push((*axes, cb.clone()));
            }
        }
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
    pub fn collect_focus(&self) -> Vec<rosace_a11y::FocusNode> {
        let mut out = Vec::new();
        self.collect_focus_node(Self::ROOT, &mut out);
        out
    }

    fn collect_focus_node(&self, id: NodeId, out: &mut Vec<rosace_a11y::FocusNode>) {
        let n = &self.nodes[id];
        out.extend(n.focus.iter().cloned());
        for &child in &n.children {
            self.collect_focus_node(child, out);
        }
    }

    /// The render-tree node that declared the [`rosace_a11y::FocusNode`]
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
        self.editable_test_node(Self::ROOT, x, y)
    }

    fn editable_test_node(&self, id: NodeId, x: f32, y: f32) -> Option<NodeId> {
        let n = &self.nodes[id];
        if n.pointer_mode == 1 {
            return None;
        }
        let (cx, cy, clipped) = self.child_coords(n, id, x, y);
        if !clipped {
            for &child in n.children.iter().rev() {
                if let Some(hit) = self.editable_test_node(child, cx, cy) {
                    return Some(hit);
                }
            }
        }
        if let Some(e) = &n.editable {
            if contains(&e.rect, x, y) {
                return Some(id);
            }
        }
        None
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
        for s in &n.semantics {
            let mut sn = rosace_core::SemanticNode::new().role(s.role.clone());
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
}

impl Default for RenderTree {
    fn default() -> Self { Self::new() }
}

/// Shared axis-preference selection (also used for overlay scroll routes):
/// first candidate handling the dominant delta axis, else first handling
/// the other axis.
pub fn select_scroll_handler(
    candidates: &[(ScrollAxes, Arc<dyn Fn(f32, f32) + Send + Sync>)],
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
        assert!(t.hit_test(5.0, 5.0).is_some(), "hit must survive the clean frame");
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

        assert!(t.hit_test(5.0, 5.0).is_none(), "repaint must clear stale hits");
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
        let (cb, _) = t.hit_test(5.0, 5.0).unwrap();
        cb(0.0, 0.0);
        assert!(!hit_first.load(std::sync::atomic::Ordering::SeqCst));
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
        let (cb, _) = t.hit_test(75.0, 90.0).expect("content region must be hit through the offset");
        cb(0.0, 0.0);
        assert!(hit.load(Ordering::SeqCst), "click mapped into scrolled content");

        // Screen (75, 40): ABOVE the viewport → clipped, no hit.
        assert!(t.hit_test(75.0, 40.0).is_none(), "clicks outside the viewport are clipped");

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
        let (cb, positional) = t.hit_test(75.0, 90.0).expect("must hit the positional region");
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
            crate::tree::Semantics::new(Role::Button).label("Save"),
        );
        // Button's inner text node — must nest under the Button.
        let label = t.slot(button, true);
        t.node_mut(label).semantics.push(
            crate::tree::Semantics::new(Role::Text).label("Save"),
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
            crate::tree::Semantics::new(Role::TextInput).label("Name").value("Ada"),
        );
        let heading = t.slot(RenderTree::ROOT, true);
        t.node_mut(heading).semantics.push(
            crate::tree::Semantics::new(Role::Heading).label("Section").heading_level(2),
        );
        let link = t.slot(RenderTree::ROOT, true);
        t.node_mut(link).semantics.push(
            crate::tree::Semantics::new(Role::Link).label("Docs").href("https://example.com"),
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

        assert!(t.hit_test(25.0, 5.0).is_none(), "removed child left a ghost hit");
    }
}
