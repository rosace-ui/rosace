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

use tezzera_core::types::{Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::Picture;

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
    pub focus:      Vec<tezzera_a11y::FocusNode>,
    pub overlays:   Vec<OverlayEntry>,
    pub transforms: Vec<TransformLayerEntry>,
    pub semantics:  Vec<super::Semantics>,

    // ── Persistent per-node state (NOT cleared on repaint) ───────────────
    /// The node's implicit scroll position (D101) — created lazily by the
    /// first scrollable painted at this position, survives rebuilds like
    /// Flutter's ScrollPosition.
    pub scroll_ctrl: Option<tezzera_scroll::ScrollController>,

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

    // ── Derivations (D091/D092) ───────────────────────────────────────────

    /// Hit-test walk: children before own regions, later siblings first —
    /// paint order is z-order, so the topmost match wins structurally (D092).
    /// Returns the topmost hit callback and whether it is POSITIONAL —
    /// positional hits become the active drag grab (streamed MouseMove
    /// positions until release); plain hits fire once.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<(Arc<dyn Fn(f32, f32) + Send + Sync>, bool)> {
        self.hit_test_node(Self::ROOT, x, y)
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
        for &child in n.children.iter().rev() {
            if let Some(cb) = self.hit_test_node(child, x, y) {
                return Some(cb);
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
        for &child in n.children.iter().rev() {
            if let Some(hit) = self.hover_test_node(child, x, y) {
                return Some(hit);
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
        for &child in n.children.iter().rev() {
            if let Some(cb) = self.long_press_node(child, x, y) {
                return Some(cb);
            }
        }
        for (rect, cb) in n.long_hits.iter().rev() {
            if contains(rect, x, y) {
                return Some(cb.clone());
            }
        }
        None
    }

    /// Set the hovered node, clearing the previous one. Returns true when
    /// the hover target changed (caller repaints only then).
    pub fn set_hover(&mut self, target: Option<NodeId>) -> bool {
        let current = self.nodes.iter().position(|n| n.hovered);
        if current == target {
            return false;
        }
        if let Some(old) = current {
            self.nodes[old].hovered = false;
        }
        if let Some(new) = target {
            self.nodes[new].hovered = true;
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
    pub fn collect_focus(&self) -> Vec<tezzera_a11y::FocusNode> {
        let mut out = Vec::new();
        self.collect_focus_node(Self::ROOT, &mut out);
        out
    }

    fn collect_focus_node(&self, id: NodeId, out: &mut Vec<tezzera_a11y::FocusNode>) {
        let n = &self.nodes[id];
        out.extend(n.focus.iter().cloned());
        for &child in &n.children {
            self.collect_focus_node(child, out);
        }
    }

    /// Derive the accessibility tree (D099): semantics entries in paint
    /// order, nested by render-tree structure. Branches with no semantic
    /// content anywhere below them are pruned.
    pub fn collect_semantics(&self) -> tezzera_core::SemanticNode {
        let mut root = tezzera_core::SemanticNode::new();
        self.collect_semantics_node(Self::ROOT, &mut root);
        root
    }

    fn collect_semantics_node(&self, id: NodeId, parent: &mut tezzera_core::SemanticNode) {
        let n = &self.nodes[id];
        for s in &n.semantics {
            let mut sn = tezzera_core::SemanticNode::new().role(s.role.clone());
            if let Some(l) = &s.label { sn = sn.label(l.clone()); }
            parent.children.push(sn);
        }
        // Children nest under THIS node's last semantic entry when it declared
        // one (a Button's inner Text belongs to the Button); nodes with no
        // semantics of their own flatten their children into the parent.
        let target: &mut tezzera_core::SemanticNode = if n.semantics.is_empty() {
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
    use tezzera_core::types::{Point, Size};

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
        let cb = t.hit_test(5.0, 5.0).unwrap();
        cb();
        assert!(!hit_first.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn semantics_tree_nests_under_declaring_node() {
        use tezzera_core::Role;
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
