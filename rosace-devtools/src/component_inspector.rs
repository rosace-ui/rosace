/// A node in the captured layout tree, used by [`ComponentInspector`].
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// Widget or component type name (e.g. `"Button"`, `"Column"`).
    pub name: &'static str,
    /// Computed position and size in logical pixels.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub children: Vec<LayoutNode>,
}

impl LayoutNode {
    pub fn new(name: &'static str, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { name, x, y, width, height, children: Vec::new() }
    }

    pub fn child(mut self, node: LayoutNode) -> Self {
        self.children.push(node);
        self
    }
}

/// Captures successive layout-tree snapshots and renders them as an ASCII
/// box-model diagram.
///
/// Feed snapshots via [`record`] each frame (or on demand). Use [`render`]
/// to get a text dump of the most recent tree.
pub struct ComponentInspector {
    history:       Vec<LayoutNode>,
    max_snapshots: usize,
    cursor:        usize, // time-travel index into history
}

impl ComponentInspector {
    pub fn new() -> Self {
        Self { history: Vec::new(), max_snapshots: 60, cursor: 0 }
    }

    pub fn max_snapshots(mut self, n: usize) -> Self {
        self.max_snapshots = n;
        self
    }

    /// Record the current layout tree root.
    pub fn record(&mut self, root: LayoutNode) {
        if self.history.len() >= self.max_snapshots {
            self.history.remove(0);
        }
        self.history.push(root);
        self.cursor = self.history.len().saturating_sub(1);
    }

    pub fn snapshot_count(&self) -> usize { self.history.len() }
    pub fn cursor(&self) -> usize         { self.cursor }

    /// Travel to a specific snapshot index.
    pub fn travel_to(&mut self, index: usize) {
        if index < self.history.len() {
            self.cursor = index;
        }
    }

    pub fn step_back(&mut self)    { self.cursor = self.cursor.saturating_sub(1); }
    pub fn step_forward(&mut self) {
        if self.cursor + 1 < self.history.len() { self.cursor += 1; }
    }

    /// Render the layout tree at the current cursor as an ASCII box-model dump.
    pub fn render(&self) -> String {
        match self.history.get(self.cursor) {
            None => "┌─ LAYOUT ──────────────────────────────────────────\n│  (no snapshot yet)\n└───────────────────────────────────────────────────\n".to_string(),
            Some(root) => {
                let mut out = format!(
                    "┌─ LAYOUT  snapshot {}/{} ─────────────────────────────\n",
                    self.cursor + 1,
                    self.history.len()
                );
                render_node(root, &mut out, 0);
                out.push_str("└───────────────────────────────────────────────────\n");
                out
            }
        }
    }
}

impl Default for ComponentInspector {
    fn default() -> Self { Self::new() }
}

fn render_node(node: &LayoutNode, out: &mut String, depth: usize) {
    let indent = "  ".repeat(depth);
    out.push_str(&format!(
        "│ {}{} {{ x:{:.0} y:{:.0} w:{:.0} h:{:.0} }}\n",
        indent, node.name, node.x, node.y, node.width, node.height
    ));
    for child in &node.children {
        render_node(child, out, depth + 1);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_tree() -> LayoutNode {
        LayoutNode::new("Column", 0.0, 0.0, 400.0, 600.0)
            .child(LayoutNode::new("Text", 0.0, 0.0, 400.0, 20.0))
            .child(LayoutNode::new("Button", 0.0, 28.0, 120.0, 40.0))
    }

    #[test]
    fn inspector_starts_empty() {
        let ci = ComponentInspector::new();
        assert_eq!(ci.snapshot_count(), 0);
        assert!(ci.render().contains("no snapshot yet"));
    }

    #[test]
    fn inspector_records_snapshot() {
        let mut ci = ComponentInspector::new();
        ci.record(simple_tree());
        assert_eq!(ci.snapshot_count(), 1);
        let out = ci.render();
        assert!(out.contains("Column"));
        assert!(out.contains("Text"));
        assert!(out.contains("Button"));
    }

    #[test]
    fn inspector_render_shows_box_model() {
        let mut ci = ComponentInspector::new();
        ci.record(LayoutNode::new("Row", 10.0, 20.0, 300.0, 50.0));
        let out = ci.render();
        assert!(out.contains("x:10"));
        assert!(out.contains("y:20"));
        assert!(out.contains("w:300"));
        assert!(out.contains("h:50"));
    }

    #[test]
    fn inspector_step_back_forward() {
        let mut ci = ComponentInspector::new();
        ci.record(LayoutNode::new("A", 0.0, 0.0, 1.0, 1.0));
        ci.record(LayoutNode::new("B", 0.0, 0.0, 2.0, 2.0));
        assert_eq!(ci.cursor(), 1);
        ci.step_back();
        assert_eq!(ci.cursor(), 0);
        assert!(ci.render().contains("w:1"));
        ci.step_forward();
        assert_eq!(ci.cursor(), 1);
        assert!(ci.render().contains("w:2"));
    }

    #[test]
    fn inspector_step_back_clamps_at_zero() {
        let mut ci = ComponentInspector::new();
        ci.record(LayoutNode::new("A", 0.0, 0.0, 1.0, 1.0));
        ci.step_back();
        ci.step_back();
        assert_eq!(ci.cursor(), 0);
    }

    #[test]
    fn inspector_travel_to() {
        let mut ci = ComponentInspector::new();
        ci.record(LayoutNode::new("A", 0.0, 0.0, 1.0, 1.0));
        ci.record(LayoutNode::new("B", 0.0, 0.0, 2.0, 2.0));
        ci.record(LayoutNode::new("C", 0.0, 0.0, 3.0, 3.0));
        ci.travel_to(0);
        assert!(ci.render().contains("w:1"));
        ci.travel_to(2);
        assert!(ci.render().contains("w:3"));
    }

    #[test]
    fn inspector_max_snapshots_evicts_oldest() {
        let mut ci = ComponentInspector::new().max_snapshots(3);
        for i in 0..5 {
            ci.record(LayoutNode::new("N", 0.0, 0.0, i as f32, 1.0));
        }
        assert_eq!(ci.snapshot_count(), 3);
    }

    #[test]
    fn inspector_children_indented() {
        let mut ci = ComponentInspector::new();
        ci.record(simple_tree());
        let out = ci.render();
        // Child nodes should appear after parent
        let col_pos  = out.find("Column").unwrap();
        let text_pos = out.find("Text").unwrap();
        assert!(text_pos > col_pos, "child should appear after parent");
    }
}
