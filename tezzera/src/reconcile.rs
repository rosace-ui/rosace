use std::collections::HashMap;
use tezzera_core::{Element, types::Key};
use super::render_node::RenderNode;

/// Reconcile a flat list of `new_elements` against the existing `render_nodes`.
///
/// Nodes are matched by DFS position within a sibling list. When a sibling group
/// contains any keyed elements, the group is first matched by key (keyed elements
/// to keyed old nodes) and then unkeyed by position within the unkeyed sublist.
///
/// On match: the node is retained and its caches are left intact (stable update).
/// On mismatch: a fresh dirty `RenderNode` replaces the old one.
/// Old nodes beyond the new list length are dropped.
pub fn reconcile(render_nodes: &mut Vec<RenderNode>, new_elements: &[Element]) {
    reconcile_siblings(render_nodes, new_elements);
}

fn reconcile_siblings(nodes: &mut Vec<RenderNode>, elements: &[Element]) {
    let has_keys = elements.iter().any(|e| element_key(e).is_some());

    if has_keys {
        reconcile_keyed(nodes, elements);
    } else {
        reconcile_by_position(nodes, elements);
    }
}

fn reconcile_by_position(nodes: &mut Vec<RenderNode>, elements: &[Element]) {
    // Grow the node list if needed.
    while nodes.len() < elements.len() {
        nodes.push(RenderNode::new("__new__", None));
    }
    // Truncate extra old nodes.
    nodes.truncate(elements.len());

    for (node, element) in nodes.iter_mut().zip(elements.iter()) {
        reconcile_one(node, element);
    }
}

fn reconcile_keyed(nodes: &mut Vec<RenderNode>, elements: &[Element]) {
    // Build key → old-node index map from existing nodes.
    let mut key_map: HashMap<u64, usize> = nodes.iter().enumerate()
        .filter_map(|(i, n)| n.key.as_ref().map(|k| (k.0, i)))
        .collect();

    // Track which old indices we have consumed.
    let mut consumed = vec![false; nodes.len()];
    // Collect unkeyed old nodes in order for positional fallback.
    let unkeyed_old: Vec<usize> = nodes.iter().enumerate()
        .filter(|(_, n)| n.key.is_none())
        .map(|(i, _)| i)
        .collect();
    let mut unkeyed_cursor = 0;

    let mut new_nodes: Vec<RenderNode> = Vec::with_capacity(elements.len());

    for element in elements {
        let elem_key = element_key(element);

        if let Some(key) = elem_key {
            if let Some(&old_idx) = key_map.get(&key.0) {
                // Key match — take the existing node.
                let old = std::mem::replace(&mut nodes[old_idx], sentinel());
                consumed[old_idx] = true;
                key_map.remove(&key.0);
                let mut reused = old;
                reconcile_one(&mut reused, element);
                new_nodes.push(reused);
            } else {
                // Keyed element with no matching old node — fresh.
                let mut fresh = RenderNode::new(element_tag(element), Some(key));
                reconcile_children_only(&mut fresh, element);
                new_nodes.push(fresh);
            }
        } else {
            // Unkeyed element — match positionally against unkeyed old nodes.
            if let Some(&old_idx) = unkeyed_old.get(unkeyed_cursor) {
                unkeyed_cursor += 1;
                if !consumed[old_idx] {
                    let old = std::mem::replace(&mut nodes[old_idx], sentinel());
                    consumed[old_idx] = true;
                    let mut reused = old;
                    reconcile_one(&mut reused, element);
                    new_nodes.push(reused);
                } else {
                    let mut fresh = RenderNode::new(element_tag(element), None);
                    reconcile_children_only(&mut fresh, element);
                    new_nodes.push(fresh);
                }
            } else {
                let mut fresh = RenderNode::new(element_tag(element), None);
                reconcile_children_only(&mut fresh, element);
                new_nodes.push(fresh);
            }
        }
    }

    *nodes = new_nodes;
}

/// Update a single node in-place against a new element, recursing into children.
fn reconcile_one(node: &mut RenderNode, element: &Element) {
    let new_tag = element_tag(element);
    let new_key = element_key(element);

    let type_matches = node.tag == new_tag;
    let key_matches  = match (&node.key, &new_key) {
        (None, None)         => true,
        (Some(a), Some(b))   => a.0 == b.0,
        _                    => false,
    };

    if type_matches && key_matches {
        // Stable node — leave layout + paint caches intact; the paint pass
        // decides if dirty. Hit/scroll regions live on the render tree (D091).
        // Recurse into children.
        reconcile_children_of(node, element);
    } else {
        // Type or key mismatch — replace with a fresh dirty node.
        *node = RenderNode::new(new_tag, new_key);
        reconcile_children_of(node, element);
    }
}

/// Reconcile children without touching the node's own caches.
fn reconcile_children_only(node: &mut RenderNode, element: &Element) {
    reconcile_children_of(node, element);
}

fn reconcile_children_of(node: &mut RenderNode, element: &Element) {
    match element {
        Element::Native(n)    => reconcile_siblings(&mut node.children, &n.children),
        Element::Component(c) => reconcile_siblings(&mut node.children, &c.children),
        _                     => node.children.clear(),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn element_tag(e: &Element) -> &'static str {
    match e {
        Element::Component(_) => "__component__",
        Element::Native(n)    => n.tag,
        Element::Text(_)      => "__text__",
        Element::Empty        => "__empty__",
    }
}

fn element_key(e: &Element) -> Option<Key> {
    match e {
        Element::Native(n)    => n.key.clone(),
        Element::Component(c) => c.key.clone(),
        _                     => None,
    }
}

/// A sentinel node used temporarily during keyed reconciliation to avoid
/// leaving gaps (borrow checker) while extracting from the old list.
fn sentinel() -> RenderNode {
    RenderNode::new("__sentinel__", None)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tezzera_core::Element;

    fn native(tag: &'static str) -> Element {
        use tezzera_core::{NativeElement, types::Key};
        Element::Native(NativeElement {
            tag,
            payload: None,
            children: vec![],
            key: None,
        })
    }

    fn native_keyed(tag: &'static str, key: u64) -> Element {
        use tezzera_core::{NativeElement, types::Key};
        Element::Native(NativeElement {
            tag,
            payload: None,
            children: vec![],
            key: Some(Key(key)),
        })
    }

    #[test]
    fn stable_node_is_reused() {
        let mut nodes = vec![RenderNode::new("Button", None)];
        // Prime with a fake cached size to verify it's preserved.
        nodes[0].cached_size = Some(tezzera_core::types::Size { width: 80.0, height: 32.0 });

        let elements = [native("Button")];
        reconcile(&mut nodes, &elements);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].tag, "Button");
        // Cache was preserved (stable reuse).
        assert!(nodes[0].cached_size.is_some());
    }

    #[test]
    fn type_mismatch_creates_fresh_node() {
        let mut nodes = vec![RenderNode::new("Button", None)];
        nodes[0].cached_size = Some(tezzera_core::types::Size { width: 80.0, height: 32.0 });

        let elements = [native("Text")];
        reconcile(&mut nodes, &elements);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].tag, "Text");
        assert!(nodes[0].cached_size.is_none(), "fresh node must not carry old cache");
        assert!(nodes[0].paint_dirty);
    }

    #[test]
    fn list_shrinks_when_elements_removed() {
        let mut nodes: Vec<RenderNode> = (0..5).map(|_| RenderNode::new("Text", None)).collect();
        let elements: Vec<Element> = (0..3).map(|_| native("Text")).collect();
        reconcile(&mut nodes, &elements);
        assert_eq!(nodes.len(), 3);
    }

    #[test]
    fn list_grows_when_elements_added() {
        let mut nodes: Vec<RenderNode> = vec![RenderNode::new("Text", None)];
        let elements: Vec<Element> = (0..4).map(|_| native("Text")).collect();
        reconcile(&mut nodes, &elements);
        assert_eq!(nodes.len(), 4);
    }

    #[test]
    fn keyed_node_survives_reorder() {
        let mut nodes = vec![
            { let mut n = RenderNode::new("Text", Some(tezzera_core::types::Key(1))); n.cached_size = Some(tezzera_core::types::Size { width: 40.0, height: 16.0 }); n },
            RenderNode::new("Text", Some(tezzera_core::types::Key(2))),
        ];

        // New order: key=2 first, key=1 second.
        let elements = [native_keyed("Text", 2), native_keyed("Text", 1)];
        reconcile(&mut nodes, &elements);

        assert_eq!(nodes.len(), 2);
        // After reorder, first node should be key=2, second key=1.
        assert_eq!(nodes[0].key.as_ref().map(|k| k.0), Some(2));
        assert_eq!(nodes[1].key.as_ref().map(|k| k.0), Some(1));
        // key=1 node had cached_size — it should be preserved after reorder.
        assert!(nodes[1].cached_size.is_some(), "keyed reorder must preserve cache");
    }
}
