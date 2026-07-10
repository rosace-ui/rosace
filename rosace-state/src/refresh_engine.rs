use std::collections::{HashMap, HashSet};

use rosace_trace::event::ComponentId;

/// DFS entry/exit timestamps for a single component node.
///
/// The invariant `ancestor.entry < descendant.entry && descendant.exit <= ancestor.exit`
/// enables O(1) ancestor queries without walking parent pointers.
#[derive(Clone, Debug)]
pub struct TreeNode {
    /// Component being tracked.
    pub id: ComponentId,
    /// DFS entry (pre-order) timestamp.
    pub entry: u64,
    /// DFS exit (post-order) timestamp.
    pub exit: u64,
    /// Parent component, if any.
    pub parent: Option<ComponentId>,
}

/// Tracks the live component tree and computes the minimum set of roots to rebuild.
///
/// When an atom changes, the dirty subscriber list may include both a parent and
/// its descendants. Rebuilding the parent already covers its subtree, so the engine
/// prunes descendants before handing the list to the scheduler.
pub struct RefreshEngine {
    nodes: HashMap<ComponentId, TreeNode>,
    next_timestamp: u64,
}

impl RefreshEngine {
    /// Creates an empty [`RefreshEngine`].
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            next_timestamp: 0,
        }
    }

    /// Register a component in the tree.
    ///
    /// `parent` must already be registered if supplied. Exit timestamps of all
    /// ancestors are extended to maintain the DFS containment invariant.
    pub fn register(&mut self, id: ComponentId, parent: Option<ComponentId>) {
        let entry = self.next_timestamp;
        self.next_timestamp += 1;
        let exit = self.next_timestamp;
        self.next_timestamp += 1;

        // Extend every ancestor's exit so they contain this node's range.
        if let Some(pid) = parent {
            let mut current = pid;
            while let Some(node) = self.nodes.get_mut(&current) {
                if node.exit <= exit {
                    node.exit = exit + 1;
                    match node.parent {
                        Some(gp) => current = gp,
                        None => break,
                    }
                } else {
                    break;
                }
            }
        }

        self.nodes.insert(id, TreeNode { id, entry, exit, parent });
    }

    /// Remove a component from the tree.
    pub fn unregister(&mut self, id: ComponentId) {
        self.nodes.remove(&id);
    }

    /// Returns `true` if `ancestor` is a strict ancestor of `descendant`.
    ///
    /// Uses O(1) DFS timestamp comparison instead of walking parent pointers.
    pub fn is_ancestor(&self, ancestor: ComponentId, descendant: ComponentId) -> bool {
        let a = match self.nodes.get(&ancestor) {
            Some(n) => n,
            None => return false,
        };
        let d = match self.nodes.get(&descendant) {
            Some(n) => n,
            None => return false,
        };
        a.entry < d.entry && d.exit <= a.exit
    }

    /// Given a set of dirty component IDs, returns the minimal rebuild-root set.
    ///
    /// A component is pruned if any other dirty component is its ancestor —
    /// the ancestor's rebuild already covers it. The resulting list contains only
    /// roots whose rebuilds are not subsumed by a dirtier ancestor.
    pub fn find_rebuild_roots(&self, dirty: &HashSet<ComponentId>) -> Vec<ComponentId> {
        dirty
            .iter()
            .filter(|&&id| {
                !dirty
                    .iter()
                    .any(|&other| other != id && self.is_ancestor(other, id))
            })
            .copied()
            .collect()
    }
}

impl Default for RefreshEngine {
    fn default() -> Self {
        Self::new()
    }
}
