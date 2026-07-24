//! The DevTools element inspector's PURE state + readout logic (D123/O2).
//!
//! Everything here is plain data over an [`InspectNode`] snapshot — no
//! canvas, no input plumbing, no `RenderTree` access. The engine owns the
//! hotkey, the hover/click `pick`, and the actual on-canvas drawing; it
//! reads the highlight rect and panel text from here so the "what does the
//! inspector say" logic stays unit-testable off a headless snapshot (the
//! whole point of the O-track: tools built on read-only seams).

use rosace_core::render_object::AxisBound;
use rosace_widgets::tree::{InspectNode, NodeId};

use crate::component_inspector::LayoutNode;

/// Convert a flat `RenderTree::inspect()` snapshot into the nested
/// [`LayoutNode`] tree [`crate::ComponentInspector`] renders (D123/O2) —
/// the seam a real app wires up: `to_layout_tree(&tree.inspect())` once per
/// frame, feed to `ComponentInspector::record`. `None` for an empty
/// snapshot.
pub fn to_layout_tree(nodes: &[InspectNode]) -> Option<LayoutNode> {
    let root_id = nodes.iter().find(|n| n.parent.is_none())?.id;
    Some(build_layout(nodes, root_id))
}

fn build_layout(nodes: &[InspectNode], id: NodeId) -> LayoutNode {
    let n = nodes.iter().find(|n| n.id == id)
        .expect("child id must reference a node in the same snapshot");
    let (x, y, w, h) = n.rect
        .map(|r| (r.origin.x, r.origin.y, r.size.width, r.size.height))
        .unwrap_or((0.0, 0.0, 0.0, 0.0));
    let name = if n.tag.is_empty() { "(unnamed)" } else { n.tag };
    let mut node = LayoutNode::new(name, x, y, w, h);
    for &child_id in &n.children {
        node = node.child(build_layout(nodes, child_id));
    }
    node
}

/// Live inspector state: whether it's on, and which nodes are hovered /
/// selected. Held by the engine across frames.
#[derive(Debug, Default)]
pub struct ElementInspector {
    pub enabled: bool,
    pub hover: Option<NodeId>,
    pub selected: Option<NodeId>,
}

impl ElementInspector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle the inspector. Turning it OFF clears the hover/selection so a
    /// re-open starts clean. Returns the new `enabled` state.
    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        if !self.enabled {
            self.hover = None;
            self.selected = None;
        }
        self.enabled
    }

    /// Set the hovered node. Returns whether it changed (the engine repaints
    /// only then).
    pub fn set_hover(&mut self, node: Option<NodeId>) -> bool {
        if self.hover == node {
            return false;
        }
        self.hover = node;
        true
    }

    /// Select the node under a click. Returns whether it changed.
    pub fn select(&mut self, node: Option<NodeId>) -> bool {
        if self.selected == node {
            return false;
        }
        self.selected = node;
        true
    }

    /// Escape behaviour: deselect if something is selected, else close the
    /// inspector. Returns `true` if anything changed (so the engine knows to
    /// repaint / stop consuming Escape).
    pub fn on_escape(&mut self) -> bool {
        if self.selected.is_some() {
            self.selected = None;
            true
        } else if self.enabled {
            self.enabled = false;
            self.hover = None;
            true
        } else {
            false
        }
    }
}

/// Look up a node's world-space rect `(x, y, w, h)` in logical pixels — the
/// highlight geometry the engine outlines. `None` if the node never painted.
pub fn node_rect(snapshot: &[InspectNode], node: NodeId) -> Option<(f32, f32, f32, f32)> {
    let n = snapshot.iter().find(|n| n.id == node)?;
    let r = n.rect?;
    Some((r.origin.x, r.origin.y, r.size.width, r.size.height))
}

/// The inspector panel's text lines for a selected node — a Flutter-style
/// readout with the W×H size first. `None` if the node isn't in the
/// snapshot (it was removed since selection).
pub fn panel_lines(snapshot: &[InspectNode], node: NodeId) -> Option<Vec<String>> {
    let n = snapshot.iter().find(|n| n.id == node)?;
    let mut lines = Vec::new();

    // Name: the widget-type tag when set (element/cache nodes), else the
    // semantic role (widget-tree nodes carry semantics but not a tag),
    // else a placeholder.
    let name = if !n.tag.is_empty() {
        n.tag.to_string()
    } else if let Some((role, _)) = n.semantics.first() {
        format!("{:?}", role)
    } else {
        "(widget)".to_string()
    };
    lines.push(name);

    // Size: the measured size when present, else the paint rect's own
    // dimensions (widget-tree nodes record a rect but not a cached_size).
    match (n.size, n.rect) {
        (Some(s), _) => lines.push(format!("size    {} × {}", fmt_f(s.width), fmt_f(s.height))),
        (None, Some(r)) => lines.push(format!("size    {} × {}", fmt_f(r.size.width), fmt_f(r.size.height))),
        (None, None) => lines.push("size    —".to_string()),
    }
    match n.rect {
        Some(r) => lines.push(format!(
            "rect    x {}  y {}",
            fmt_f(r.origin.x), fmt_f(r.origin.y)
        )),
        None => lines.push("rect    —".to_string()),
    }
    if let Some(c) = n.constraints {
        lines.push(format!("w  {}", fmt_axis(c.min_width, &c.max_width)));
        lines.push(format!("h  {}", fmt_axis(c.min_height, &c.max_height)));
    }
    if let Some((role, label)) = n.semantics.first() {
        match label {
            Some(l) => lines.push(format!("role    {:?} \"{}\"", role, l)),
            None => lines.push(format!("role    {:?}", role)),
        }
    }
    // Interaction summary — only when there's anything to show.
    let mut flags = Vec::new();
    if n.hit_count > 0 { flags.push(format!("hits {}", n.hit_count)); }
    if n.scroll_count > 0 { flags.push(format!("scroll {}", n.scroll_count)); }
    if n.overlay_count > 0 { flags.push(format!("overlay {}", n.overlay_count)); }
    if n.has_editable { flags.push("editable".to_string()); }
    if !flags.is_empty() {
        lines.push(flags.join("  "));
    }

    Some(lines)
}

fn fmt_f(v: f32) -> String {
    // Whole numbers read cleaner without a trailing `.0`; sub-pixel sizes
    // keep one decimal.
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

fn fmt_axis(min: f32, max: &AxisBound) -> String {
    let max_s = match max {
        AxisBound::Bounded(v) => fmt_f(*v),
        AxisBound::Unbounded => "∞".to_string(),
        AxisBound::Shrink => "fit".to_string(),
    };
    format!("[{}..{}]", fmt_f(min), max_s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_widgets::tree::render_tree::RenderTree;
    use rosace_core::types::{Point, Rect, Size};
    use rosace_core::render_object::Constraints;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
    }

    #[test]
    fn toggle_off_clears_hover_and_selection() {
        let mut ins = ElementInspector::new();
        assert!(ins.toggle(), "first toggle enables");
        ins.hover = Some(3);
        ins.selected = Some(4);
        assert!(!ins.toggle(), "second toggle disables");
        assert_eq!(ins.hover, None);
        assert_eq!(ins.selected, None);
    }

    #[test]
    fn set_hover_and_select_report_change_only_on_diff() {
        let mut ins = ElementInspector::new();
        assert!(ins.set_hover(Some(1)));
        assert!(!ins.set_hover(Some(1)), "same node is not a change");
        assert!(ins.set_hover(None));
        assert!(ins.select(Some(2)));
        assert!(!ins.select(Some(2)));
    }

    #[test]
    fn escape_deselects_then_closes() {
        let mut ins = ElementInspector::new();
        ins.enabled = true;
        ins.selected = Some(5);
        assert!(ins.on_escape(), "first escape deselects");
        assert_eq!(ins.selected, None);
        assert!(ins.enabled, "still open after deselect");
        assert!(ins.on_escape(), "second escape closes");
        assert!(!ins.enabled);
        assert!(!ins.on_escape(), "escape while closed is a no-op");
    }

    #[test]
    fn node_rect_and_panel_read_a_real_snapshot() {
        let mut t = RenderTree::new();
        t.start_frame();
        let node = t.slot(RenderTree::ROOT, true);
        t.node_mut(node).tag = "Button";
        t.node_mut(node).cached_rect = Some(rect(30.0, 210.0, 120.0, 40.0));
        t.node_mut(node).cached_size = Some(Size { width: 120.0, height: 40.0 });
        t.node_mut(node).last_constraints = Some(Constraints::loose(300.0, 600.0));
        t.node_mut(node).semantics.push(
            rosace_widgets::tree::Semantics::new(rosace_core::Role::Button).label("Save"),
        );
        t.node_mut(node).hits.push((rect(30.0, 210.0, 120.0, 40.0), std::sync::Arc::new(|| {})));
        t.finalize();
        let snap = t.inspect();

        assert_eq!(node_rect(&snap, node), Some((30.0, 210.0, 120.0, 40.0)));

        let lines = panel_lines(&snap, node).expect("selected node is present");
        assert_eq!(lines[0], "Button");
        assert!(lines.iter().any(|l| l == "size    120 × 40"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("rect    x 30  y 210")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("[0..300]")), "constraints line: {lines:?}");
        assert!(lines.iter().any(|l| l.contains("Button \"Save\"")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("hits 1")), "{lines:?}");
    }

    #[test]
    fn widget_node_names_from_role_and_sizes_from_rect_when_tag_absent() {
        // A widget-tree node (the common case for the picker) carries a
        // rect + semantics but no `tag`/`cached_size` — the readout must
        // still show a real name and size, not "(unnamed)"/"—".
        let mut t = RenderTree::new();
        t.start_frame();
        let node = t.slot(RenderTree::ROOT, true);
        t.node_mut(node).cached_rect = Some(rect(84.0, 230.0, 300.0, 40.0));
        t.node_mut(node).semantics.push(
            rosace_widgets::tree::Semantics::new(rosace_core::Role::TextInput).label("Your name"),
        );
        t.finalize();
        let snap = t.inspect();
        let lines = panel_lines(&snap, node).unwrap();
        assert_eq!(lines[0], "TextInput", "name falls back to the role");
        assert!(lines.iter().any(|l| l == "size    300 × 40"), "size from rect: {lines:?}");
    }

    #[test]
    fn panel_none_for_a_node_absent_from_the_snapshot() {
        let t = RenderTree::new();
        let snap = t.inspect();
        assert!(panel_lines(&snap, 999).is_none());
        assert!(node_rect(&snap, 999).is_none());
    }

    #[test]
    fn to_layout_tree_converts_a_real_render_tree() {
        let mut t = RenderTree::new();
        t.start_frame();
        t.node_mut(RenderTree::ROOT).tag = "Scaffold";
        t.node_mut(RenderTree::ROOT).cached_rect = Some(rect(0.0, 0.0, 400.0, 600.0));
        let child = t.slot(RenderTree::ROOT, true);
        t.node_mut(child).tag = "Button";
        t.node_mut(child).cached_rect = Some(rect(10.0, 20.0, 120.0, 40.0));
        t.finalize();

        let layout = to_layout_tree(&t.inspect()).expect("non-empty converts");
        assert_eq!(layout.name, "Scaffold");
        assert_eq!(layout.children.len(), 1);
        assert_eq!(layout.children[0].name, "Button");
        assert!(to_layout_tree(&[]).is_none());
    }

    #[test]
    fn unbounded_constraints_render_as_infinity() {
        let mut t = RenderTree::new();
        t.start_frame();
        let node = t.slot(RenderTree::ROOT, true);
        t.node_mut(node).tag = "Column";
        t.node_mut(node).cached_rect = Some(rect(0.0, 0.0, 10.0, 10.0));
        t.node_mut(node).last_constraints = Some(Constraints::unbounded());
        t.finalize();
        let snap = t.inspect();
        let lines = panel_lines(&snap, node).unwrap();
        assert!(lines.iter().any(|l| l.contains('∞')), "{lines:?}");
    }
}
