//! Semantic tree export for native mobile hosts (D132, iOS/Android half).
//!
//! Desktop pushes its semantic tree into AccessKit each frame
//! (`rosace-platform::a11y_bridge`). Mobile deliberately does the opposite:
//! it **pulls**.
//!
//! Both mobile accessibility APIs are demand-driven — iOS calls
//! `accessibilityElementCount` / `accessibilityElement(at:)` on a
//! `UIAccessibilityContainer` only while VoiceOver is inspecting, and Android
//! calls `AccessibilityNodeProvider::createAccessibilityNodeInfo` only while
//! TalkBack is. So the host asks for the tree exactly when the OS asks it,
//! and an app with no assistive tech running never serializes anything at
//! all. That is strictly better than the desktop model, where AccessKit's
//! activation handshake forces a tree to be parked in advance.
//!
//! The wire format is JSON, matching the Platform Channel (D127) rather than
//! inventing a second convention. Volume is small (tens of nodes) and it is
//! only produced on demand, so a hand-rolled binary layout would buy nothing
//! and cost both hosts a parser.
//!
//! ```json
//! { "id": 0, "role": "button", "label": "Save", "value": null,
//!   "bounds": { "x": 12.0, "y": 40.0, "w": 88.0, "h": 32.0 },
//!   "children": [ … ] }
//! ```
//!
//! `bounds` are **logical** pixels, window-relative. Each host converts:
//! iOS wants screen-space `CGRect` via `UIAccessibility.convertToScreenCoordinates`,
//! Android wants physical-pixel `Rect`. Converting here would mean baking one
//! platform's convention into the shared layer — the same mistake that made
//! the desktop bridge report every element at half size until the
//! logical-vs-physical mismatch was found.

use rosace_core::{Role, SemanticNode};

/// The engine's current semantic tree as JSON. See the module doc for the
/// shape and the coordinate convention.
pub fn semantics_json(engine: &crate::Engine) -> String {
    node_json(&engine.semantics()).to_string()
}

/// Stable lowercase role names. Spelled out rather than derived from
/// `Debug`, so a rename in `Role` cannot silently change the wire format
/// that two native hosts are matching on.
fn role_name(role: &Role) -> &'static str {
    match role {
        Role::Button => "button",
        Role::Text => "text",
        Role::Image => "image",
        Role::Slider => "slider",
        Role::Alert => "alert",
        Role::Dialog => "dialog",
        Role::Checkbox => "checkbox",
        Role::Radio => "radio",
        Role::Switch => "switch",
        Role::TextInput => "textinput",
        Role::MenuItem => "menuitem",
        Role::ProgressBar => "progressbar",
        Role::Link => "link",
        Role::Heading => "heading",
        Role::List => "list",
        Role::ListItem => "listitem",
        Role::Tab => "tab",
        Role::TabPanel => "tabpanel",
        Role::Unknown => "unknown",
    }
}

fn node_json(n: &SemanticNode) -> serde_json::Value {
    serde_json::json!({
        "id": n.id,
        "role": role_name(&n.role),
        "label": n.label,
        "value": n.value,
        "bounds": n.bounds.map(|b| serde_json::json!({
            "x": b.origin.x, "y": b.origin.y,
            "w": b.size.width, "h": b.size.height,
        })),
        "children": n.children.iter().map(node_json).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::{Point, Rect, Size};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
    }

    #[test]
    fn serializes_role_label_bounds_and_nesting() {
        let tree = SemanticNode::new().child(
            SemanticNode::new()
                .id(256)
                .role(Role::Button)
                .label("Save")
                .bounds(rect(12.0, 40.0, 88.0, 32.0))
                .child(SemanticNode::new().id(512).role(Role::Text).label("Save")),
        );
        let v = node_json(&tree);
        let btn = &v["children"][0];
        assert_eq!(btn["id"], 256);
        assert_eq!(btn["role"], "button");
        assert_eq!(btn["label"], "Save");
        assert_eq!(btn["bounds"]["x"], 12.0);
        assert_eq!(btn["bounds"]["w"], 88.0);
        assert_eq!(btn["children"][0]["id"], 512, "nesting must survive the round trip");
    }

    #[test]
    fn absent_fields_are_null_not_omitted() {
        // Both hosts decode into typed structs; a missing key and an explicit
        // null are different things to a strict decoder, and `Codable`/`kotlinx`
        // both treat an absent non-optional as an error.
        let v = node_json(&SemanticNode::new());
        assert!(v["label"].is_null());
        assert!(v["value"].is_null());
        assert!(v["bounds"].is_null());
        assert!(v["id"].is_null());
    }

    #[test]
    fn role_names_are_stable_and_lowercase() {
        // Guards the wire contract two native hosts match on.
        assert_eq!(role_name(&Role::TextInput), "textinput");
        assert_eq!(role_name(&Role::ProgressBar), "progressbar");
        assert_eq!(role_name(&Role::Unknown), "unknown");
    }
}
