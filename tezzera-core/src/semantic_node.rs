/// The ARIA-style role of a semantic node in the accessibility tree.
///
/// This is the one source of truth for role data flowing through
/// `RenderTree::collect_semantics()` — used both for assistive tech (D099)
/// and, from D107/Phase 25 on, for mapping to real HTML tags (`<h1>`-`<h6>`,
/// `<a>`, `<ul>`/`<li>`, ...) for SEO/crawler-facing output. Deliberately
/// NOT unified with the separate, richer `tezzera_a11y::role::Role` — that
/// one drives `tezzera-a11y`'s own internal focus-management tree, a
/// different concern (focus navigation, not HTML/SEO structure); merging
/// them would touch already-working, unrelated code for no benefit this
/// phase actually needs. `Link`/`Heading`/`List`/`ListItem`/`Tab`/
/// `TabPanel`/`Radio` added here specifically for the HTML mapping Phase 25
/// needs (a heading's level and a link's href live on `SemanticNode`/
/// `Semantics` directly, not on the enum, since they're per-instance data,
/// not part of what kind of role it is). `Radio` is distinct from
/// `Checkbox` — real ARIA/HTML (`role="radio"` vs `role="checkbox"`)
/// distinguishes mutually-exclusive single-select from independent
/// toggles; approximating one as the other would be wrong, not just
/// imprecise, so it earns its own variant rather than reusing `Checkbox`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Button,
    Text,
    Image,
    Slider,
    Alert,
    Dialog,
    Checkbox,
    Radio,
    Switch,
    TextInput,
    MenuItem,
    ProgressBar,
    Link,
    Heading,
    List,
    ListItem,
    Tab,
    TabPanel,
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
    /// The node's current value, if any (a `TextInput`'s typed text, a
    /// `Slider`/`ProgressBar`'s numeric value as a string, ...) — distinct
    /// from `label`, which is the node's accessible NAME, not its content.
    pub value: Option<String>,
    /// `1..=6` for `Role::Heading` (`<h1>`-`<h6>`); `None` for every other
    /// role, including a heading whose level genuinely isn't known (falls
    /// back to `<h2>` at the HTML-mapping step, not here).
    pub heading_level: Option<u8>,
    /// The link target for `Role::Link` (`<a href="...">`); `None` for
    /// every other role.
    pub href: Option<String>,
    /// Child semantic nodes.
    pub children: Vec<SemanticNode>,
}

impl SemanticNode {
    /// Creates a new `SemanticNode` with no label, `Role::Unknown`, and no children.
    pub fn new() -> Self {
        SemanticNode {
            label: None,
            role: Role::Unknown,
            value: None,
            heading_level: None,
            href: None,
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

    /// Sets the node's current value (see the field doc for how this
    /// differs from `label`).
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Sets the heading level (`1..=6`) — meaningful only for `Role::Heading`.
    pub fn heading_level(mut self, level: u8) -> Self {
        self.heading_level = Some(level);
        self
    }

    /// Sets the link target — meaningful only for `Role::Link`.
    pub fn href(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_semantic_node_has_unknown_role_and_no_optional_fields() {
        let node = SemanticNode::new();
        assert_eq!(node.role, Role::Unknown);
        assert!(node.label.is_none());
        assert!(node.value.is_none());
        assert!(node.heading_level.is_none());
        assert!(node.href.is_none());
        assert!(node.children.is_empty());
    }

    #[test]
    fn builder_methods_set_the_expected_fields() {
        let node = SemanticNode::new()
            .role(Role::Heading)
            .label("Section title")
            .heading_level(2)
            .value("current value")
            .child(SemanticNode::new().role(Role::Text).label("child"));
        assert_eq!(node.role, Role::Heading);
        assert_eq!(node.label.as_deref(), Some("Section title"));
        assert_eq!(node.heading_level, Some(2));
        assert_eq!(node.value.as_deref(), Some("current value"));
        assert_eq!(node.children.len(), 1);
    }

    #[test]
    fn href_only_meaningful_for_link_but_settable_regardless() {
        let node = SemanticNode::new().role(Role::Link).href("https://example.com");
        assert_eq!(node.href.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(SemanticNode::default().role, SemanticNode::new().role);
    }
}
