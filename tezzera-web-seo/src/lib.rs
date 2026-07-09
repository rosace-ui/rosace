//! `SemanticNode` → HTML mapping (D107/Phase 25, Step 3).
//!
//! Canvas stays the only visual renderer, everywhere — this crate never
//! draws anything. It exists purely to give crawlers and assistive tech
//! real HTML/text where a canvas-only page would show them nothing (no
//! text/structure in a canvas, same as a screenshot). See
//! `.steering/PHASE_25.md` for the full design and why this is a build-time
//! + runtime-fallback pair, not a second visual widget renderer.
//!
//! **Never compiled into iOS/Android/desktop builds** — see this crate's
//! `Cargo.toml` description for the two consumers (`tzr build --target
//! web`, and `tezzera-platform`'s wasm32-gated runtime module) and why
//! neither pulls this into a non-web app binary.

use tezzera_core::{Role, SemanticNode};

/// Renders a `SemanticNode` tree to an HTML fragment — the content that
/// goes inside a `<template shadowrootmode="open">` block (build-time,
/// Step 3) or gets synced into a live shadow root (runtime, Step 4). Not a
/// full HTML document; the caller wraps this in whatever document/template
/// shell it needs.
pub fn render_html(node: &SemanticNode) -> String {
    let mut out = String::new();
    render_node(node, &mut out);
    out
}

/// Wraps `render_html`'s output in a Declarative Shadow DOM template — the
/// literal HTML fragment `tzr build --target web` embeds directly in a
/// route's build-time output (Step 3's primary deliverable): a real shadow
/// root created straight from HTML, no JavaScript required, so it reaches
/// crawlers that skip JS execution entirely.
pub fn render_shadow_dom_template(node: &SemanticNode) -> String {
    format!(
        "<template shadowrootmode=\"open\">{}</template>",
        render_html(node)
    )
}

/// Plain-text extraction — every label/value in the tree, depth-first,
/// space-separated. This is the `llms.txt`/markdown-summary source (Step
/// 3's second deliverable) and also handy for a quick sanity check of a
/// tree's real text content without reading HTML.
pub fn render_text(node: &SemanticNode) -> String {
    let mut parts = Vec::new();
    collect_text(node, &mut parts);
    parts.join(" ")
}

fn collect_text(node: &SemanticNode, out: &mut Vec<String>) {
    if let Some(label) = &node.label {
        if !label.is_empty() { out.push(label.clone()); }
    }
    if let Some(value) = &node.value {
        if !value.is_empty() { out.push(value.clone()); }
    }
    for child in &node.children {
        collect_text(child, out);
    }
}

/// Renders `node`'s children, auto-wrapping any run of consecutive
/// `Role::ListItem` siblings in a synthetic `<ul>`. Widgets like `ListTile`
/// emit `Role::ListItem` whether or not an app wraps them in an explicit
/// `Role::List` container (most don't — they're just placed in a `Column`),
/// so without this a bare `<li>` with no `<ul>` ancestor loses its implicit
/// `listitem` accessibility role and silently drops out of the a11y tree —
/// confirmed via a real Chrome accessibility-tree read during D107 Step 5,
/// not assumed from spec-reading. `Role::List`'s own branch renders its
/// children directly (not through this function) since it already supplies
/// the `<ul>` wrapper itself.
fn render_children(node: &SemanticNode, out: &mut String) {
    let children = &node.children;
    let mut i = 0;
    while i < children.len() {
        if children[i].role == Role::ListItem {
            let start = i;
            while i < children.len() && children[i].role == Role::ListItem {
                i += 1;
            }
            out.push_str("<ul>");
            for child in &children[start..i] {
                render_node(child, out);
            }
            out.push_str("</ul>");
        } else {
            render_node(&children[i], out);
            i += 1;
        }
    }
}

/// One node → one HTML element (or, for `Role::Unknown`, no wrapping
/// element at all — just its children, matching how `collect_semantics`
/// already treats non-semantic nodes as structurally transparent). Every
/// attribute value and every text node is escaped — `label`/`value`/`href`
/// are arbitrary app data, not trusted HTML.
fn render_node(node: &SemanticNode, out: &mut String) {
    let label = node.label.as_deref().unwrap_or("");
    match node.role {
        Role::Unknown => render_children(node, out),

        Role::Heading => {
            let level = node.heading_level.unwrap_or(2).clamp(1, 6);
            out.push_str(&format!("<h{level}>{}</h{level}>", esc(label)));
        }
        Role::Text => {
            out.push_str(&format!("<p>{}</p>", esc(label)));
        }
        Role::Link => {
            let href = node.href.as_deref().unwrap_or("#");
            out.push_str(&format!("<a href=\"{}\">{}</a>", esc_attr(href), esc(label)));
        }
        Role::Image => {
            out.push_str(&format!("<img alt=\"{}\">", esc_attr(label)));
        }
        Role::Button => {
            out.push_str(&format!("<button>{}</button>", esc(label)));
        }
        Role::List => {
            out.push_str("<ul>");
            for child in &node.children {
                render_node(child, out);
            }
            out.push_str("</ul>");
        }
        Role::ListItem => {
            out.push_str("<li>");
            out.push_str(&esc(label));
            render_children(node, out);
            out.push_str("</li>");
        }
        Role::MenuItem => {
            out.push_str(&format!("<li role=\"menuitem\">{}</li>", esc(label)));
        }
        Role::Checkbox => {
            let checked = node.value.as_deref() == Some("checked") || node.value.as_deref() == Some("selected");
            out.push_str(&format!(
                "<input type=\"checkbox\" aria-label=\"{}\"{}>",
                esc_attr(label), if checked { " checked" } else { "" }
            ));
        }
        Role::Radio => {
            let checked = node.value.as_deref() == Some("selected");
            out.push_str(&format!(
                "<input type=\"radio\" aria-label=\"{}\"{}>",
                esc_attr(label), if checked { " checked" } else { "" }
            ));
        }
        Role::Switch => {
            let on = node.value.as_deref() == Some("checked") || node.value.as_deref() == Some("on");
            out.push_str(&format!(
                "<input type=\"checkbox\" role=\"switch\" aria-label=\"{}\"{}>",
                esc_attr(label), if on { " checked" } else { "" }
            ));
        }
        Role::TextInput => {
            let value = node.value.as_deref().unwrap_or("");
            out.push_str(&format!(
                "<input type=\"text\" aria-label=\"{}\" value=\"{}\">",
                esc_attr(label), esc_attr(value)
            ));
        }
        Role::Slider => {
            let value = node.value.as_deref().unwrap_or("");
            out.push_str(&format!(
                "<input type=\"range\" aria-label=\"{}\" aria-valuenow=\"{}\">",
                esc_attr(label), esc_attr(value)
            ));
        }
        Role::ProgressBar => {
            let value = node.value.as_deref().unwrap_or("");
            out.push_str(&format!(
                "<progress aria-label=\"{}\" value=\"{}\"></progress>",
                esc_attr(label), esc_attr(value)
            ));
        }
        Role::Alert => {
            out.push_str(&format!("<div role=\"alert\">{}</div>", esc(label)));
        }
        Role::Dialog => {
            out.push_str("<div role=\"dialog\"");
            if !label.is_empty() {
                out.push_str(&format!(" aria-label=\"{}\"", esc_attr(label)));
            }
            out.push('>');
            render_children(node, out);
            out.push_str("</div>");
        }
        Role::Tab => {
            let selected = node.value.as_deref() == Some("selected");
            out.push_str(&format!(
                "<button role=\"tab\" aria-selected=\"{}\">{}</button>",
                if selected { "true" } else { "false" }, esc(label)
            ));
        }
        Role::TabPanel => {
            out.push_str("<div role=\"tabpanel\">");
            render_children(node, out);
            out.push_str("</div>");
        }
    }
}

/// Escapes text content (goes between tags — `&`/`<`/`>` matter; quotes don't).
fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Escapes an attribute value (goes inside `"..."` — quotes matter too).
fn esc_attr(s: &str) -> String {
    esc(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(role: Role, label: &str) -> SemanticNode {
        SemanticNode::new().role(role).label(label)
    }

    #[test]
    fn unknown_role_renders_only_children_no_wrapper() {
        let tree = SemanticNode::new().child(node(Role::Text, "hello"));
        assert_eq!(render_html(&tree), "<p>hello</p>");
    }

    #[test]
    fn heading_uses_the_declared_level() {
        let n = SemanticNode::new().role(Role::Heading).label("Title").heading_level(3);
        assert_eq!(render_html(&n), "<h3>Title</h3>");
    }

    #[test]
    fn heading_defaults_to_level_2_when_unset() {
        let n = node(Role::Heading, "Title");
        assert_eq!(render_html(&n), "<h2>Title</h2>");
    }

    #[test]
    fn link_uses_href_and_falls_back_to_hash() {
        let with_href = SemanticNode::new().role(Role::Link).label("Docs").href("https://example.com");
        assert_eq!(render_html(&with_href), "<a href=\"https://example.com\">Docs</a>");
        let without_href = node(Role::Link, "Docs");
        assert_eq!(render_html(&without_href), "<a href=\"#\">Docs</a>");
    }

    #[test]
    fn image_uses_label_as_alt() {
        let n = node(Role::Image, "A photo of a cat");
        assert_eq!(render_html(&n), "<img alt=\"A photo of a cat\">");
    }

    #[test]
    fn list_wraps_list_items() {
        let tree = SemanticNode::new().role(Role::List).child(node(Role::ListItem, "One")).child(node(Role::ListItem, "Two"));
        assert_eq!(render_html(&tree), "<ul><li>One</li><li>Two</li></ul>");
    }

    #[test]
    fn button_and_nested_structure_matches_a_real_appbar_plus_listtile_shape() {
        // Mirrors what AppBar (Heading) + ListTile (ListItem) actually
        // produce via collect_semantics, per Step 2.
        let tree = SemanticNode::new()
            .child(SemanticNode::new().role(Role::Heading).label("theme_preview").heading_level(1))
            .child(node(Role::ListItem, "Counter, A simple counter with + / \u{2212}"));
        let html = render_html(&tree);
        assert!(html.contains("<h1>theme_preview</h1>"));
        assert!(html.contains("<ul><li>Counter, A simple counter with"));
    }

    #[test]
    fn orphan_list_items_get_a_synthetic_ul_wrapper() {
        // A ListTile placed directly in a Column (no explicit List
        // container) is the common case, not the exception — without this,
        // a bare <li> with no <ul> ancestor loses its implicit listitem
        // role in the accessibility tree (confirmed in a real browser).
        let tree = SemanticNode::new().child(node(Role::ListItem, "Only item"));
        assert_eq!(render_html(&tree), "<ul><li>Only item</li></ul>");
    }

    #[test]
    fn consecutive_orphan_list_items_share_one_ul_non_list_items_dont_join_it() {
        let tree = SemanticNode::new()
            .child(node(Role::ListItem, "One"))
            .child(node(Role::ListItem, "Two"))
            .child(node(Role::Text, "Not a list item"))
            .child(node(Role::ListItem, "Three"));
        assert_eq!(
            render_html(&tree),
            "<ul><li>One</li><li>Two</li></ul><p>Not a list item</p><ul><li>Three</li></ul>"
        );
    }

    #[test]
    fn checkbox_reflects_checked_state() {
        let checked = SemanticNode::new().role(Role::Checkbox).label("Agree").value("checked");
        assert!(render_html(&checked).contains("checked"));
        let unchecked = SemanticNode::new().role(Role::Checkbox).label("Agree").value("unchecked");
        assert!(!render_html(&unchecked).contains(" checked"));
    }

    #[test]
    fn escapes_html_special_characters_in_label_and_attributes() {
        let n = node(Role::Text, "<script>alert('x')</script> & \"quotes\"");
        let html = render_html(&n);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&amp;"));

        let link = SemanticNode::new().role(Role::Link).label("go").href("\"onmouseover=\"alert(1)");
        let link_html = render_html(&link);
        assert!(!link_html.contains("\"onmouseover=\""));
        assert!(link_html.contains("&quot;onmouseover=&quot;"));
    }

    #[test]
    fn shadow_dom_template_wraps_the_html() {
        let n = node(Role::Text, "hi");
        let wrapped = render_shadow_dom_template(&n);
        assert_eq!(wrapped, "<template shadowrootmode=\"open\"><p>hi</p></template>");
    }

    #[test]
    fn render_text_extracts_labels_and_values_depth_first() {
        let tree = SemanticNode::new()
            .child(node(Role::Heading, "Title"))
            .child(SemanticNode::new().role(Role::TextInput).label("Name").value("Ada"));
        assert_eq!(render_text(&tree), "Title Name Ada");
    }
}
