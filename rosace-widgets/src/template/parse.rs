//! The runtime `view!` parser (D103 / D102 Tier 1 — rollout step 4).
//!
//! Turns edited `view!` **source text** into a [`Template`] at runtime, without
//! the compiler — the piece that lets a dev watcher pick up an edit and diff it
//! against the running shape. It parses through the SAME grammar crate
//! (`rosace-view-syntax`) the compile-time macro uses, then converts the AST to
//! a `Template` with the SAME rules the macro's descriptor codegen uses
//! (literals → [`StaticValue`], non-literals → positional [`PropValue::Hole`],
//! props before children). Sharing the grammar is what guarantees the runtime
//! template matches what the binary was compiled with (see the equivalence
//! test in `rosace/tests/view_template.rs`).

use rosace_view_syntax::{parse_str, scan_file, ViewElement, ViewLiteral};

use super::{StaticValue, Template, TemplateKey, TemplateNode};

/// Why parsing a `view!` body failed at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The body did not parse as `view!` syntax (the message is syn's).
    Syntax(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Syntax(m) => write!(f, "view! parse error: {m}"),
        }
    }
}
impl std::error::Error for ParseError {}

/// Parse a `view!` body (the text inside `view! { … }`) into a [`Template`]
/// keyed by `key`. The hole indices it assigns match the compile-time macro's,
/// so the running binary's compiled hole array lines up slot-for-slot.
pub fn parse_template(body_src: &str, key: TemplateKey) -> Result<Template, ParseError> {
    let ast = parse_str(body_src).map_err(|e| ParseError::Syntax(e.to_string()))?;
    let mut hole = 0usize;
    let root = to_node(&ast, &mut hole);
    Ok(Template::new(key, root))
}

/// Parse EVERY `view!` in a source file into a keyed [`Template`] — what a dev
/// watcher calls on a changed `.rs`. Each template's key is `(file, line, col)`
/// of its `view!` site, so the runtime can match it to the running template.
///
/// Matching note: `col` here is the `view` token's 0-based column (proc-macro2),
/// while the macro's `TemplateKey` column comes from `column!()`; match
/// primarily on `(file, line)` and treat column as a tiebreaker.
pub fn parse_file_templates(src: &str, file: &str) -> Result<Vec<Template>, ParseError> {
    let sites = scan_file(src).map_err(|e| ParseError::Syntax(e.to_string()))?;
    let mut out = Vec::with_capacity(sites.len());
    for site in sites {
        let key = TemplateKey::new(file, site.line as u32, site.column as u32);
        let mut hole = 0usize;
        let root = to_node(&site.element, &mut hole);
        out.push(Template::new(key, root));
    }
    Ok(out)
}

fn to_node(el: &ViewElement, hole: &mut usize) -> TemplateNode {
    let mut node = TemplateNode::new(el.name_str());
    // Positional constructor args first (they take the earliest hole slots),
    // matching the compile-time macro's traversal order.
    for arg in &el.args {
        match &arg.literal {
            Some(lit) => node = node.with_arg_static(to_static(lit)),
            None => {
                let idx = *hole;
                *hole += 1;
                node = node.with_arg_hole(idx);
            }
        }
    }
    for prop in &el.props {
        match &prop.literal {
            Some(lit) => node = node.with_static(prop.name_str(), to_static(lit)),
            None => {
                let idx = *hole;
                *hole += 1;
                node = node.with_hole(prop.name_str(), idx);
            }
        }
    }
    for child in &el.children {
        node = node.with_child(to_node(child, hole));
    }
    node
}

fn to_static(lit: &ViewLiteral) -> StaticValue {
    match lit {
        ViewLiteral::Bool(b) => StaticValue::Bool(*b),
        ViewLiteral::Int(i) => StaticValue::Int(*i),
        ViewLiteral::Float(f) => StaticValue::Float(*f),
        ViewLiteral::Str(s) => StaticValue::Str(s.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::PropValue;

    fn key() -> TemplateKey {
        TemplateKey::new("src/edited.rs", 3, 5)
    }

    #[test]
    fn parses_a_static_tree() {
        let t = parse_template("Column { spacing: 8.0 Text { content: \"Hi\" } }", key()).unwrap();
        assert_eq!(t.root.widget, "Column");
        assert_eq!(t.hole_count, 0);
        assert_eq!(t.root.props[0], ("spacing".into(), PropValue::Static(StaticValue::Float(8.0))));
        assert_eq!(t.root.children[0].widget, "Text");
        assert_eq!(
            t.root.children[0].props[0],
            ("content".into(), PropValue::Static(StaticValue::Str("Hi".into())))
        );
    }

    #[test]
    fn assigns_positional_holes_props_before_children() {
        let t = parse_template("Column { spacing: gap Text { content: name } }", key()).unwrap();
        assert_eq!(t.hole_count, 2);
        // Column.spacing is hole 0 (a prop, visited before children)...
        assert_eq!(t.root.props[0], ("spacing".into(), PropValue::Hole(0)));
        // ...Text.content is hole 1.
        assert_eq!(t.root.children[0].props[0], ("content".into(), PropValue::Hole(1)));
    }

    #[test]
    fn mixed_static_and_hole() {
        let t = parse_template("Column { spacing: 12.0 Text { content: title } }", key()).unwrap();
        assert_eq!(t.hole_count, 1);
        assert_eq!(t.root.props[0], ("spacing".into(), PropValue::Static(StaticValue::Float(12.0))));
        assert_eq!(t.root.children[0].props[0], ("content".into(), PropValue::Hole(0)));
    }

    #[test]
    fn syntax_error_is_reported_not_panicked() {
        let err = parse_template("Column { : : : }", key()).unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)));
    }

    #[test]
    fn parses_all_view_sites_in_a_file_with_keys() {
        let src = "\
fn a() { let x = view! { Row { spacing: 2.0 } }; }
fn b() { let y = view! { Column { Text { content: name } } }; }
";
        let templates = parse_file_templates(src, "src/app.rs").unwrap();
        assert_eq!(templates.len(), 2);

        let row = templates.iter().find(|t| t.root.widget == "Row").unwrap();
        assert_eq!(row.key.file, "src/app.rs");
        assert_eq!(row.key.line, 1);
        assert_eq!(row.hole_count, 0);

        let col = templates.iter().find(|t| t.root.widget == "Column").unwrap();
        assert_eq!(col.key.line, 2);
        assert_eq!(col.hole_count, 1); // `name` is a hole
        assert_eq!(col.root.children[0].props[0], ("content".into(), PropValue::Hole(0)));
    }
}
