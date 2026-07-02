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

use tezzera_core::types::Rect;

use super::overlay::OverlayEntry;
use super::TransformLayerEntry;

pub type NodeId = usize;

/// A click callback with its hit rect in window-space logical pixels.
pub type HitRegion = (Rect, Arc<dyn Fn() + Send + Sync>);
/// A `(delta_x, delta_y)` scroll callback with its viewport rect.
pub type ScrollRegion = (Rect, Arc<dyn Fn(f32, f32) + Send + Sync>);

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
    pub scrolls:    Vec<ScrollRegion>,
    pub focus:      Vec<tezzera_a11y::FocusNode>,
    pub overlays:   Vec<OverlayEntry>,
    pub transforms: Vec<TransformLayerEntry>,
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

    /// Begin (re)painting `node`: clear declared data, reset the child cursor.
    fn begin(&mut self, node: NodeId) {
        let n = &mut self.nodes[node];
        n.cursor = 0;
        n.begun = true;
        n.hits.clear();
        n.scrolls.clear();
        n.focus.clear();
        n.overlays.clear();
        n.transforms.clear();
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
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Arc<dyn Fn() + Send + Sync>> {
        self.hit_test_node(Self::ROOT, x, y)
    }

    fn hit_test_node(&self, id: NodeId, x: f32, y: f32) -> Option<Arc<dyn Fn() + Send + Sync>> {
        let n = &self.nodes[id];
        for &child in n.children.iter().rev() {
            if let Some(cb) = self.hit_test_node(child, x, y) {
                return Some(cb);
            }
        }
        for (rect, cb) in n.hits.iter().rev() {
            if contains(rect, x, y) {
                return Some(cb.clone());
            }
        }
        None
    }

    /// Scroll routing with the same structural z-order as `hit_test` —
    /// the innermost (deepest) viewport under the cursor wins.
    pub fn scroll_test(&self, x: f32, y: f32) -> Option<Arc<dyn Fn(f32, f32) + Send + Sync>> {
        self.scroll_test_node(Self::ROOT, x, y)
    }

    fn scroll_test_node(&self, id: NodeId, x: f32, y: f32) -> Option<Arc<dyn Fn(f32, f32) + Send + Sync>> {
        let n = &self.nodes[id];
        for &child in n.children.iter().rev() {
            if let Some(cb) = self.scroll_test_node(child, x, y) {
                return Some(cb);
            }
        }
        for (rect, cb) in n.scrolls.iter().rev() {
            if contains(rect, x, y) {
                return Some(cb.clone());
            }
        }
        None
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
