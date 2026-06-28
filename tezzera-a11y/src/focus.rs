use crate::tree::A11yTree;

/// Manages keyboard focus across the accessibility tree.
#[derive(Debug, Clone, Default)]
pub struct FocusManager {
    pub focused: Option<u64>,
    focus_order: Vec<u64>,
}

impl FocusManager {
    pub fn new() -> Self { Self::default() }

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
}
