use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use rosace_state::Atom;

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

struct FocusNodeInner {
    pub id:      u64,
    pub next:    Mutex<Option<FocusNode>>,
    pub prev:    Mutex<Option<FocusNode>>,
    /// Reactive focus state. Widgets read this to draw their focus ring.
    pub focused: Atom<bool>,
}

/// A shared focus handle that can be attached to any focusable widget.
///
/// Nodes can be wired to explicit neighbors via `.focus_next()` / `.focus_prev()`.
/// When no neighbors are wired, `FocusManager` falls back to natural tree order.
///
/// ```rust,ignore
/// let username = FocusNode::new();
/// let password = FocusNode::new();
/// let submit   = FocusNode::new();
///
/// TextInput::new("Username").focus_node(username.clone())
/// TextInput::new("Password").focus_node(password.clone())
///     .focus_next_node(submit.clone())
///     .focus_prev_node(username.clone())
/// Button::new("Login").focus_node(submit.clone())
///     .focus_prev_node(password.clone())
/// ```
#[derive(Clone)]
pub struct FocusNode(Arc<FocusNodeInner>);

impl FocusNode {
    /// Create a new, unconnected focus node.
    pub fn new() -> Self {
        FocusNode(Arc::new(FocusNodeInner {
            id:      ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            next:    Mutex::new(None),
            prev:    Mutex::new(None),
            focused: rosace_state::use_atom(false),
        }))
    }

    /// Unique ID for this node. Used by `FocusManager` for traversal.
    pub fn id(&self) -> u64 { self.0.id }

    /// Reactive focused state. Read this in `paint()` to draw a focus ring.
    pub fn focused(&self) -> Atom<bool> { self.0.focused.clone() }

    /// Returns `true` if this node currently has focus.
    pub fn is_focused(&self) -> bool { self.0.focused.get() }

    /// Programmatically request focus on this node.
    pub fn request(&self) { self.0.focused.set(true); }

    /// Release focus from this node (does not move focus elsewhere).
    pub fn release(&self) { self.0.focused.set(false); }

    /// Wire an explicit next node (Tab / ArrowDown direction).
    pub fn set_next(&self, node: FocusNode) {
        *self.0.next.lock().unwrap() = Some(node);
    }

    /// Wire an explicit previous node (Shift+Tab / ArrowUp direction).
    pub fn set_prev(&self, node: FocusNode) {
        *self.0.prev.lock().unwrap() = Some(node);
    }

    /// Returns the wired next node if one was set.
    pub fn next_node(&self) -> Option<FocusNode> {
        self.0.next.lock().unwrap().clone()
    }

    /// Returns the wired prev node if one was set.
    pub fn prev_node(&self) -> Option<FocusNode> {
        self.0.prev.lock().unwrap().clone()
    }
}

impl Default for FocusNode {
    fn default() -> Self { FocusNode::new() }
}

impl std::fmt::Debug for FocusNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FocusNode(id={}, focused={})", self.0.id, self.0.focused.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_nodes_have_unique_ids() {
        let a = FocusNode::new();
        let b = FocusNode::new();
        assert_ne!(a.id(), b.id());
    }

    #[test]
    fn new_node_not_focused() {
        let node = FocusNode::new();
        assert!(!node.is_focused());
    }

    #[test]
    fn request_sets_focused() {
        let node = FocusNode::new();
        node.request();
        assert!(node.is_focused());
    }

    #[test]
    fn release_clears_focused() {
        let node = FocusNode::new();
        node.request();
        node.release();
        assert!(!node.is_focused());
    }

    #[test]
    fn explicit_next_wiring() {
        let a = FocusNode::new();
        let b = FocusNode::new();
        a.set_next(b.clone());
        let next = a.next_node().expect("next should be set");
        assert_eq!(next.id(), b.id());
    }

    #[test]
    fn explicit_prev_wiring() {
        let a = FocusNode::new();
        let b = FocusNode::new();
        a.set_prev(b.clone());
        let prev = a.prev_node().expect("prev should be set");
        assert_eq!(prev.id(), b.id());
    }

    #[test]
    fn clone_shares_state() {
        let a = FocusNode::new();
        let b = a.clone();
        a.request();
        assert!(b.is_focused(), "cloned node should share focused state");
    }
}
