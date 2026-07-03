/// The ARIA-style role of a semantic node in the accessibility tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Button,
    Text,
    Image,
    Slider,
    Alert,
    Dialog,
    Checkbox,
    Switch,
    TextInput,
    MenuItem,
    ProgressBar,
    Unknown,
}

/// A node in the accessibility (semantics) tree.
///
/// The semantics tree mirrors the visual element tree but carries only the
/// information assistive technologies need. It is rebuilt alongside the render
/// tree and diffed separately.
#[derive(Clone, Debug)]
pub struct SemanticNode {
    /// Human-readable label announced by screen readers.
    pub label: Option<String>,
    /// The ARIA role of this node.
    pub role: Role,
    /// Child semantic nodes.
    pub children: Vec<SemanticNode>,
}

impl SemanticNode {
    /// Creates a new `SemanticNode` with no label, `Role::Unknown`, and no children.
    pub fn new() -> Self {
        SemanticNode {
            label: None,
            role: Role::Unknown,
            children: Vec::new(),
        }
    }

    /// Sets the accessible label for this node.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the ARIA role for this node.
    pub fn role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    /// Appends a child semantic node.
    pub fn child(mut self, node: SemanticNode) -> Self {
        self.children.push(node);
        self
    }
}

impl Default for SemanticNode {
    fn default() -> Self {
        SemanticNode::new()
    }
}
