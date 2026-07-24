//! The template descriptor data model (D103 / D102 Tier 1 — rollout step 2).
//!
//! A `view!` tree splits into two halves (see `.steering/HOT_RELOAD.md`):
//!   - **shape** — which widgets, their nesting, and literal props → travels as
//!     data ([`Template`]); hot-swappable.
//!   - **logic** — the `{expr}` bits (a `count.to_string()`, an `on_press`
//!     closure) → stays compiled machine code on the device, never travels;
//!     it fills numbered [`PropValue::Hole`]s.

/// A literal prop value that travels as **data** — the wire-friendly subset of
/// what a `view!` prop can be. Anything that is not one of these (a computed
/// expression, a closure, a struct value) is a [`PropValue::Hole`] filled by
/// compiled code, not carried in the template.
///
/// Kept intentionally primitive so the descriptor has a trivial JSON form when
/// the transport step needs one (see module docs for the serde deferral).
#[derive(Clone, Debug, PartialEq)]
pub enum StaticValue {
    Bool(bool),
    /// Integer literal. Widened to `i64` so every `view!` integer literal fits
    /// one variant; the registry narrows per-setter at inflate time.
    Int(i64),
    Float(f64),
    Str(String),
}

/// A prop's value in a template: either a compile-time literal that travels as
/// data, or a numbered **hole** filled at runtime by the already-compiled
/// `{expr}` at that slot.
///
/// Holes are **positional** (index into the frame's hole array) — that is all
/// hot reload needs, because the dev build recompiles the same source so the
/// hole order is stable. Name-based binding (what server-driven UI needs, so a
/// remote `"onSave"` can resolve to a compiled handler) is a documented future
/// extension, not built here.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Static(StaticValue),
    Hole(usize),
}

/// One node in a template tree: a widget **kind by name** (the string the
/// registry maps to a constructor), its props, and its children.
///
/// The name is a `String`, not a widget type — inflating it is the interpreter
/// + registry's job (step 3). This node knows nothing about how it paints.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateNode {
    /// Widget kind, e.g. `"Column"`, `"Button"` — the registry key.
    pub widget: String,
    /// Positional **constructor arguments** (the `("Hi")` in `Text("Hi")`), in
    /// order — they fill `Widget::new(...)`. Hole slots for args come before
    /// prop/child slots, matching the macro's traversal order.
    pub args: Vec<PropValue>,
    /// `key: value` props, in source order.
    pub props: Vec<(String, PropValue)>,
    /// Nested child nodes, in source order.
    pub children: Vec<TemplateNode>,
}

impl TemplateNode {
    /// A leaf node of the given widget kind with no args, props, or children.
    pub fn new(widget: impl Into<String>) -> Self {
        Self { widget: widget.into(), args: Vec::new(), props: Vec::new(), children: Vec::new() }
    }

    /// Add a static (literal) positional constructor arg.
    pub fn with_arg_static(mut self, value: StaticValue) -> Self {
        self.args.push(PropValue::Static(value));
        self
    }

    /// Add a positional constructor arg bound to a hole slot.
    pub fn with_arg_hole(mut self, index: usize) -> Self {
        self.args.push(PropValue::Hole(index));
        self
    }

    /// Add a static (literal) prop.
    pub fn with_static(mut self, key: impl Into<String>, value: StaticValue) -> Self {
        self.props.push((key.into(), PropValue::Static(value)));
        self
    }

    /// Add a hole prop bound to `index` in the frame's hole array.
    pub fn with_hole(mut self, key: impl Into<String>, index: usize) -> Self {
        self.props.push((key.into(), PropValue::Hole(index)));
        self
    }

    /// Add a child node.
    pub fn with_child(mut self, child: TemplateNode) -> Self {
        self.children.push(child);
        self
    }

    /// The highest hole index referenced by this node or any descendant, plus
    /// one — i.e. the number of hole slots the subtree expects. `0` when the
    /// subtree is fully static.
    fn hole_extent(&self) -> usize {
        let hole_idx = |v: &PropValue| match v {
            PropValue::Hole(i) => Some(i + 1),
            PropValue::Static(_) => None,
        };
        let in_args = self.args.iter().filter_map(hole_idx).max().unwrap_or(0);
        let in_props = self.props.iter().filter_map(|(_, v)| hole_idx(v)).max().unwrap_or(0);
        let below = self.children.iter().map(TemplateNode::hole_extent).max().unwrap_or(0);
        in_args.max(in_props).max(below)
    }
}

/// Source-location key identifying a single `view!` site, so the dev watcher
/// can match an edited template against the one currently running and diff them
/// (D103's `location!()` key). Stable across a rebuild of the same source.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TemplateKey {
    /// Source file path (from `file!()`).
    pub file: String,
    /// 1-based line (from `line!()`).
    pub line: u32,
    /// 1-based column (from `column!()`).
    pub col: u32,
}

impl TemplateKey {
    pub fn new(file: impl Into<String>, line: u32, col: u32) -> Self {
        Self { file: file.into(), line, col }
    }
}

impl std::fmt::Display for TemplateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}

/// A full template: the root [`TemplateNode`] plus the number of hole slots it
/// references, keyed by source location for diffing across hot reloads.
///
/// `hole_count` is the **slot signature's** core: a template edit that keeps
/// the same holes (reorder/wrap/retext static elements) is hot-swappable; one
/// that changes the hole count adds/removes compiled logic and must escalate
/// (Tier 2 dylib or Tier 0 restart) — D103's boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub key: TemplateKey,
    pub root: TemplateNode,
    pub hole_count: usize,
}

impl Template {
    /// Build a template from its root node, deriving `hole_count` from the
    /// holes the tree actually references.
    pub fn new(key: TemplateKey, root: TemplateNode) -> Self {
        let hole_count = root.hole_extent();
        Self { key, root, hole_count }
    }

    /// Whether `self` and `other` share the same hole signature — the cheap
    /// test for "is this edit hot-swappable, or does it need code?" A full
    /// per-slot signature comparison arrives with the diff step; hole count is
    /// the necessary first gate (a changed count is always an escalation).
    pub fn hole_signature_matches(&self, other: &Template) -> bool {
        self.hole_count == other.hole_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> TemplateKey {
        TemplateKey::new("src/app.rs", 12, 5)
    }

    #[test]
    fn leaf_node_has_no_props_or_children() {
        let n = TemplateNode::new("Text");
        assert_eq!(n.widget, "Text");
        assert!(n.props.is_empty());
        assert!(n.children.is_empty());
    }

    #[test]
    fn builder_records_statics_holes_and_children_in_order() {
        let n = TemplateNode::new("Button")
            .with_static("label", StaticValue::Str("Save".into()))
            .with_hole("on_press", 0)
            .with_child(TemplateNode::new("Icon"));
        assert_eq!(n.props.len(), 2);
        assert_eq!(n.props[0], ("label".to_string(), PropValue::Static(StaticValue::Str("Save".into()))));
        assert_eq!(n.props[1], ("on_press".to_string(), PropValue::Hole(0)));
        assert_eq!(n.children.len(), 1);
        assert_eq!(n.children[0].widget, "Icon");
    }

    #[test]
    fn hole_count_is_zero_for_a_fully_static_tree() {
        let root = TemplateNode::new("Column")
            .with_static("spacing", StaticValue::Int(12))
            .with_child(TemplateNode::new("Text").with_static("content", StaticValue::Str("Hi".into())));
        let t = Template::new(key(), root);
        assert_eq!(t.hole_count, 0);
    }

    #[test]
    fn hole_count_is_max_index_plus_one_across_the_whole_tree() {
        // Holes 0 and 1 on children, none on the root → count 2.
        let root = TemplateNode::new("Column")
            .with_child(TemplateNode::new("Button").with_hole("on_press", 0))
            .with_child(TemplateNode::new("Button").with_hole("on_press", 1));
        let t = Template::new(key(), root);
        assert_eq!(t.hole_count, 2);
    }

    #[test]
    fn hole_count_uses_the_highest_index_even_when_sparse() {
        // Only hole 3 is referenced (0..2 filled by other slots elsewhere) →
        // the subtree still expects 4 slots so index 3 is addressable.
        let root = TemplateNode::new("Text").with_hole("content", 3);
        let t = Template::new(key(), root);
        assert_eq!(t.hole_count, 4);
    }

    #[test]
    fn signature_matches_only_when_hole_counts_are_equal() {
        // Same shape, one static text differs → same hole count → swappable.
        let a = Template::new(
            key(),
            TemplateNode::new("Text").with_static("content", StaticValue::Str("A".into())),
        );
        let b = Template::new(
            key(),
            TemplateNode::new("Text").with_static("content", StaticValue::Str("B".into())),
        );
        assert!(a.hole_signature_matches(&b));

        // Adding a hole (new compiled logic) changes the count → escalation.
        let c = Template::new(key(), TemplateNode::new("Text").with_hole("content", 0));
        assert!(!a.hole_signature_matches(&c));
    }

    #[test]
    fn key_displays_as_file_line_col() {
        assert_eq!(TemplateKey::new("src/app.rs", 12, 5).to_string(), "src/app.rs:12:5");
    }

    #[test]
    fn key_equality_and_hashing_identify_a_view_site() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        seen.insert(TemplateKey::new("src/app.rs", 12, 5));
        assert!(seen.contains(&TemplateKey::new("src/app.rs", 12, 5)));
        assert!(!seen.contains(&TemplateKey::new("src/app.rs", 12, 6)));
    }
}
