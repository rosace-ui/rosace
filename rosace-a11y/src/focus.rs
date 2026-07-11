use crate::focus_node::FocusNode;
use crate::tree::A11yTree;

/// Manages keyboard focus across the accessibility tree.
#[derive(Debug, Clone, Default)]
pub struct FocusManager {
    pub focused: Option<u64>,
    focus_order: Vec<u64>,
    /// Live node references for Tab cycling — rebuilt each frame by `sync_from_nodes`.
    nodes: Vec<FocusNode>,
}

impl FocusManager {
    pub fn new() -> Self { Self::default() }

    /// Rebuild Tab order from a flat list of `FocusNode`s collected during the
    /// paint pass (DFS order). Releases focus from any node that no longer appears.
    pub fn sync_from_nodes(&mut self, nodes: Vec<FocusNode>) {
        let new_ids: Vec<u64> = nodes.iter().map(|n| n.id()).collect();
        // Release focus from stale nodes.
        if let Some(fid) = self.focused {
            if !new_ids.contains(&fid) {
                // Find old node and release it.
                if let Some(old) = self.nodes.iter().find(|n| n.id() == fid) {
                    old.release();
                }
                self.focused = None;
            }
        }
        self.focus_order = new_ids;
        self.nodes = nodes;
    }

    /// Advance focus to the next node in Tab order (wraps). Calls `request()`
    /// on the new node and `release()` on the previous.
    pub fn focus_next_node(&mut self) {
        if self.nodes.is_empty() { return; }
        let next_idx = match self.focused {
            None => 0,
            Some(cur) => {
                let pos = self.focus_order.iter().position(|&id| id == cur);
                match pos {
                    None => 0,
                    Some(i) => (i + 1) % self.nodes.len(),
                }
            }
        };
        self.activate(next_idx);
    }

    /// Move focus to the previous node (Shift+Tab, wraps).
    pub fn focus_prev_node(&mut self) {
        if self.nodes.is_empty() { return; }
        let prev_idx = match self.focused {
            None => self.nodes.len() - 1,
            Some(cur) => {
                let pos = self.focus_order.iter().position(|&id| id == cur);
                match pos {
                    None => self.nodes.len() - 1,
                    Some(i) => if i == 0 { self.nodes.len() - 1 } else { i - 1 },
                }
            }
        };
        self.activate(prev_idx);
    }

    /// Directly focus a KNOWN node by id (D112/Phase 28 Step 1:
    /// click-to-focus) — unlike `focus_next_node`/`focus_prev_node`,
    /// which step relative to the current position, this jumps straight
    /// to a target found some other way (a click hit test). Same
    /// release-old/request-new invariant as `activate`, so a click never
    /// leaves two nodes simultaneously reporting `is_focused() == true`.
    /// A no-op if `id` isn't in the current Tab-order snapshot (stale
    /// caller, or called before the first `sync_from_nodes`).
    pub fn focus_specific(&mut self, id: u64) {
        if self.focused == Some(id) {
            return;
        }
        if let Some(fid) = self.focused {
            if let Some(old) = self.nodes.iter().find(|n| n.id() == fid) {
                old.release();
            }
        }
        if let Some(node) = self.nodes.iter().find(|n| n.id() == id) {
            node.request();
            self.focused = Some(id);
        }
    }

    /// Release focus entirely — no node becomes focused (D112/Phase 28
    /// Step 1: clicking blank space blurs whatever was focused). Distinct
    /// from `clear_focus` (which only forgets the id, leaving the actual
    /// `FocusNode`'s own reactive `focused` flag — and thus its drawn
    /// focus ring — stuck on).
    pub fn blur(&mut self) {
        if let Some(fid) = self.focused {
            if let Some(node) = self.nodes.iter().find(|n| n.id() == fid) {
                node.release();
            }
        }
        self.focused = None;
    }

    fn activate(&mut self, idx: usize) {
        // Release current.
        if let Some(fid) = self.focused {
            if let Some(old) = self.nodes.iter().find(|n| n.id() == fid) {
                old.release();
            }
        }
        // Request next.
        let node = &self.nodes[idx];
        node.request();
        self.focused = Some(node.id());
    }

    /// Rebuild focus order from `tree` (BFS order of focusable nodes).
    pub fn sync(&mut self, tree: &A11yTree) {
        self.focus_order = tree.focusable_nodes().iter().map(|n| n.id).collect();
        // Remove stale focused id if it's no longer in focus order
        if let Some(fid) = self.focused {
            if !self.focus_order.contains(&fid) {
                self.focused = None;
            }
        }
    }

    pub fn set_focus(&mut self, id: u64) {
        self.focused = Some(id);
    }

    pub fn clear_focus(&mut self) {
        self.focused = None;
    }

    /// Move focus to the next focusable node (Tab). Wraps around.
    pub fn focus_next(&mut self) -> Option<u64> {
        if self.focus_order.is_empty() { return None; }
        let next = match self.focused {
            None => self.focus_order[0],
            Some(cur) => {
                let pos = self.focus_order.iter().position(|&id| id == cur);
                match pos {
                    None => self.focus_order[0],
                    Some(i) => self.focus_order[(i + 1) % self.focus_order.len()],
                }
            }
        };
        self.focused = Some(next);
        Some(next)
    }

    /// Move focus to the previous focusable node (Shift+Tab). Wraps around.
    pub fn focus_prev(&mut self) -> Option<u64> {
        if self.focus_order.is_empty() { return None; }
        let prev = match self.focused {
            None => *self.focus_order.last().unwrap(),
            Some(cur) => {
                let pos = self.focus_order.iter().position(|&id| id == cur);
                match pos {
                    None => *self.focus_order.last().unwrap(),
                    Some(i) => {
                        if i == 0 {
                            self.focus_order[self.focus_order.len() - 1]
                        } else {
                            self.focus_order[i - 1]
                        }
                    }
                }
            }
        };
        self.focused = Some(prev);
        Some(prev)
    }

    pub fn tab_order(&self) -> &[u64] { &self.focus_order }
    pub fn focus_count(&self) -> usize { self.focus_order.len() }
    pub fn has_focus(&self) -> bool { self.focused.is_some() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::A11yNode;
    use crate::role::Role;
    use crate::tree::A11yTree;

    fn make_two_button_tree() -> A11yTree {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog));
        tree.add_child(0, A11yNode::new(1, Role::Button).with_label("First"));
        tree.add_child(0, A11yNode::new(2, Role::Button).with_label("Second"));
        tree
    }

    #[test]
    fn focus_new() {
        let fm = FocusManager::new();
        assert!(fm.focused.is_none());
        assert!(fm.focus_order.is_empty());
    }

    #[test]
    fn focus_sync_builds_order() {
        let tree = make_two_button_tree();
        let mut fm = FocusManager::new();
        fm.sync(&tree);
        assert_eq!(fm.focus_count(), 2);
    }

    #[test]
    fn focus_set_focus() {
        let mut fm = FocusManager::new();
        fm.set_focus(42);
        assert_eq!(fm.focused, Some(42));
    }

    #[test]
    fn focus_clear_focus() {
        let mut fm = FocusManager::new();
        fm.set_focus(1);
        fm.clear_focus();
        assert!(fm.focused.is_none());
    }

    #[test]
    fn focus_next_from_none() {
        let tree = make_two_button_tree();
        let mut fm = FocusManager::new();
        fm.sync(&tree);
        let id = fm.focus_next();
        assert!(id.is_some());
    }

    #[test]
    fn focus_next_wraps() {
        let tree = make_two_button_tree();
        let mut fm = FocusManager::new();
        fm.sync(&tree);
        let first = fm.focus_next().unwrap();
        let second = fm.focus_next().unwrap();
        let wrapped = fm.focus_next().unwrap();
        assert_ne!(first, second);
        assert_eq!(first, wrapped);
    }

    #[test]
    fn focus_prev_from_none() {
        let tree = make_two_button_tree();
        let mut fm = FocusManager::new();
        fm.sync(&tree);
        let id = fm.focus_prev();
        assert!(id.is_some());
    }

    #[test]
    fn focus_prev_wraps() {
        let tree = make_two_button_tree();
        let mut fm = FocusManager::new();
        fm.sync(&tree);
        // Start at last element
        let last = fm.focus_prev().unwrap();
        // Going prev again gives second-to-last
        let second_last = fm.focus_prev().unwrap();
        // Going prev again wraps back to last
        let wrapped = fm.focus_prev().unwrap();
        assert_ne!(last, second_last);
        assert_eq!(last, wrapped);
    }

    #[test]
    fn focus_next_single_node() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Button));
        let mut fm = FocusManager::new();
        fm.sync(&tree);
        let first = fm.focus_next();
        let second = fm.focus_next(); // wraps to same
        assert_eq!(first, second);
    }

    #[test]
    fn focus_tab_order_len() {
        let tree = make_two_button_tree();
        let mut fm = FocusManager::new();
        fm.sync(&tree);
        assert_eq!(fm.tab_order().len(), 2);
    }

    #[test]
    fn focus_has_focus_true() {
        let mut fm = FocusManager::new();
        fm.set_focus(1);
        assert!(fm.has_focus());
    }

    #[test]
    fn focus_sync_removes_stale() {
        let tree = make_two_button_tree();
        let mut fm = FocusManager::new();
        fm.sync(&tree);
        // Set focus to an id that won't be in a new empty tree
        fm.set_focus(1);
        // Sync with an empty tree — stale focus should be removed
        let empty_tree = A11yTree::new(99);
        fm.sync(&empty_tree);
        assert!(fm.focused.is_none());
    }

    // ── focus_specific / blur (D112/Phase 28 Step 1 — click-to-focus) ────
    //
    // These exercise the REAL `FocusNode` path (`sync_from_nodes`), not
    // the older bare-`u64` `sync`/`set_focus` API above — `set_focus`
    // only writes `self.focused`, it never touches an actual `FocusNode`'s
    // own reactive `is_focused()` flag, which is exactly the gap that let
    // a click silently focus-in-name-only (caught live via
    // `rosace/src/engine.rs`'s integration tests, root-caused here).

    #[test]
    fn focus_specific_sets_focused_and_the_real_node_reports_is_focused() {
        let a = FocusNode::new();
        let b = FocusNode::new();
        let mut fm = FocusManager::new();
        fm.sync_from_nodes(vec![a.clone(), b.clone()]);

        fm.focus_specific(b.id());
        assert_eq!(fm.focused, Some(b.id()));
        assert!(b.is_focused(), "the actual FocusNode must report focused, not just FocusManager's id field");
        assert!(!a.is_focused());
    }

    #[test]
    fn focus_specific_releases_the_previously_focused_node() {
        let a = FocusNode::new();
        let b = FocusNode::new();
        let mut fm = FocusManager::new();
        fm.sync_from_nodes(vec![a.clone(), b.clone()]);

        fm.focus_specific(a.id());
        assert!(a.is_focused());
        fm.focus_specific(b.id());
        assert!(!a.is_focused(), "moving focus must release the old node — two simultaneously focused nodes is the exact bug this method exists to prevent");
        assert!(b.is_focused());
    }

    #[test]
    fn focus_specific_on_already_focused_node_is_a_noop() {
        let a = FocusNode::new();
        let mut fm = FocusManager::new();
        fm.sync_from_nodes(vec![a.clone()]);
        fm.focus_specific(a.id());
        fm.focus_specific(a.id()); // must not release-then-immediately-refocus
        assert!(a.is_focused());
    }

    #[test]
    fn focus_specific_unknown_id_is_a_noop() {
        let a = FocusNode::new();
        let mut fm = FocusManager::new();
        fm.sync_from_nodes(vec![a.clone()]);
        fm.focus_specific(999_999);
        assert_eq!(fm.focused, None, "an id outside the current Tab-order snapshot must not become focused");
    }

    #[test]
    fn blur_releases_the_focused_node_and_clears_the_id() {
        let a = FocusNode::new();
        let mut fm = FocusManager::new();
        fm.sync_from_nodes(vec![a.clone()]);
        fm.focus_specific(a.id());
        assert!(a.is_focused());

        fm.blur();
        assert!(fm.focused.is_none());
        assert!(!a.is_focused(), "blur must release the real node, not just forget its id");
    }

    #[test]
    fn blur_with_nothing_focused_is_a_noop() {
        let mut fm = FocusManager::new();
        fm.blur(); // must not panic
        assert!(fm.focused.is_none());
    }
}
