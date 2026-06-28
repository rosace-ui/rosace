use std::collections::HashMap;
use crate::node::A11yNode;
use crate::role::Role;

/// The accessibility semantic tree.
///
/// Nodes are stored by id. The root node is the top-level container.
#[derive(Debug, Clone, Default)]
pub struct A11yTree {
    nodes: HashMap<u64, A11yNode>,
    pub root: u64,
    next_id: u64,
}

impl A11yTree {
    pub fn new(root_id: u64) -> Self {
        Self { nodes: HashMap::new(), root: root_id, next_id: root_id + 1 }
    }

    /// Generate a unique node ID.
    pub fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_node(&mut self, node: A11yNode) {
        self.nodes.insert(node.id, node);
    }

    /// Add `child` as a child of `parent_id`. Updates both parent's children list
    /// and child's parent field.
    pub fn add_child(&mut self, parent_id: u64, mut child: A11yNode) {
        child.parent = Some(parent_id);
        let child_id = child.id;
        self.nodes.insert(child_id, child);
        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.add_child(child_id);
        }
    }

    pub fn remove_node(&mut self, id: u64) {
        if let Some(node) = self.nodes.remove(&id) {
            // Remove from parent's children list
            if let Some(parent_id) = node.parent {
                if let Some(parent) = self.nodes.get_mut(&parent_id) {
                    parent.children.retain(|&c| c != id);
                }
            }
        }
    }

    pub fn get(&self, id: u64) -> Option<&A11yNode> { self.nodes.get(&id) }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut A11yNode> { self.nodes.get_mut(&id) }

    pub fn find_by_role(&self, role: Role) -> Vec<&A11yNode> {
        self.nodes.values().filter(|n| n.role == role).collect()
    }

    pub fn find_by_label(&self, label: &str) -> Option<&A11yNode> {
        self.nodes.values().find(|n| n.label.as_deref() == Some(label))
    }

    pub fn children_of(&self, id: u64) -> Vec<&A11yNode> {
        match self.nodes.get(&id) {
            None => vec![],
            Some(parent) => parent.children.iter()
                .filter_map(|cid| self.nodes.get(cid))
                .collect(),
        }
    }

    /// BFS traversal returning all node IDs in breadth-first order.
    pub fn bfs_order(&self) -> Vec<u64> {
        let mut order = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        if self.nodes.contains_key(&self.root) {
            queue.push_back(self.root);
        }
        while let Some(id) = queue.pop_front() {
            order.push(id);
            if let Some(node) = self.nodes.get(&id) {
                for &child_id in &node.children {
                    queue.push_back(child_id);
                }
            }
        }
        order
    }

    /// All focusable nodes in BFS order.
    pub fn focusable_nodes(&self) -> Vec<&A11yNode> {
        self.bfs_order()
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .filter(|n| n.is_focusable())
            .collect()
    }

    pub fn node_count(&self) -> usize { self.nodes.len() }
    pub fn is_empty(&self) -> bool { self.nodes.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_new_empty() {
        let tree = A11yTree::new(0);
        assert!(tree.is_empty());
        assert_eq!(tree.node_count(), 0);
    }

    #[test]
    fn tree_add_node() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog));
        assert_eq!(tree.node_count(), 1);
    }

    #[test]
    fn tree_node_count() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog));
        tree.add_node(A11yNode::new(1, Role::Button));
        assert_eq!(tree.node_count(), 2);
    }

    #[test]
    fn tree_add_child_sets_parent() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog));
        tree.add_child(0, A11yNode::new(1, Role::Button));
        let child = tree.get(1).unwrap();
        assert_eq!(child.parent, Some(0));
        let parent = tree.get(0).unwrap();
        assert!(parent.children.contains(&1));
    }

    #[test]
    fn tree_remove_node() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog));
        tree.add_child(0, A11yNode::new(1, Role::Button));
        tree.remove_node(1);
        assert!(tree.get(1).is_none());
        let parent = tree.get(0).unwrap();
        assert!(!parent.children.contains(&1));
    }

    #[test]
    fn tree_find_by_role_empty() {
        let tree = A11yTree::new(0);
        assert!(tree.find_by_role(Role::Button).is_empty());
    }

    #[test]
    fn tree_find_by_role_found() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog));
        tree.add_child(0, A11yNode::new(1, Role::Button));
        tree.add_child(0, A11yNode::new(2, Role::Button));
        let buttons = tree.find_by_role(Role::Button);
        assert_eq!(buttons.len(), 2);
    }

    #[test]
    fn tree_find_by_label() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog).with_label("Settings"));
        tree.add_child(0, A11yNode::new(1, Role::Button).with_label("Save"));
        let node = tree.find_by_label("Save");
        assert!(node.is_some());
        assert_eq!(node.unwrap().id, 1);
    }

    #[test]
    fn tree_children_of() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog));
        tree.add_child(0, A11yNode::new(1, Role::Button));
        tree.add_child(0, A11yNode::new(2, Role::Button));
        let children = tree.children_of(0);
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn tree_children_of_unknown_id() {
        let tree = A11yTree::new(0);
        let children = tree.children_of(99);
        assert!(children.is_empty());
    }

    #[test]
    fn tree_bfs_order() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog));
        tree.add_child(0, A11yNode::new(1, Role::Button));
        tree.add_child(0, A11yNode::new(2, Role::Button));
        let order = tree.bfs_order();
        assert_eq!(order[0], 0);
        assert!(order.contains(&1));
        assert!(order.contains(&2));
    }

    #[test]
    fn tree_focusable_nodes_only_interactive() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Dialog)); // not interactive
        tree.add_child(0, A11yNode::new(1, Role::Button)); // interactive
        tree.add_child(0, A11yNode::new(2, Role::Text));   // not interactive
        let focusable = tree.focusable_nodes();
        assert_eq!(focusable.len(), 1);
        assert_eq!(focusable[0].id, 1);
    }

    #[test]
    fn tree_is_empty() {
        let tree = A11yTree::new(0);
        assert!(tree.is_empty());
    }

    #[test]
    fn tree_get_mut() {
        let mut tree = A11yTree::new(0);
        tree.add_node(A11yNode::new(0, Role::Button));
        let node = tree.get_mut(0).unwrap();
        node.disabled = true;
        assert!(tree.get(0).unwrap().disabled);
    }
}
