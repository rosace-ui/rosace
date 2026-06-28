use crate::role::Role;

/// A node in the accessibility tree.
#[derive(Debug, Clone)]
pub struct A11yNode {
    pub id: u64,
    pub role: Role,
    pub label: Option<String>,
    pub description: Option<String>,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
    pub focusable: bool,
    pub checked: Option<bool>,
    pub disabled: bool,
    pub expanded: Option<bool>,
    pub value: Option<String>,
}

impl A11yNode {
    pub fn new(id: u64, role: Role) -> Self {
        Self {
            id,
            role,
            label: None,
            description: None,
            children: Vec::new(),
            parent: None,
            focusable: role.is_interactive(),
            checked: None,
            disabled: false,
            expanded: None,
            value: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn add_child(&mut self, child_id: u64) {
        if !self.children.contains(&child_id) {
            self.children.push(child_id);
        }
    }

    pub fn is_focusable(&self) -> bool {
        self.focusable && !self.disabled
    }

    pub fn accessible_name(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_new_button_is_focusable() {
        let n = A11yNode::new(1, Role::Button);
        assert!(n.is_focusable());
    }

    #[test]
    fn node_new_text_not_focusable() {
        let n = A11yNode::new(2, Role::Text);
        assert!(!n.is_focusable());
    }

    #[test]
    fn node_with_label() {
        let n = A11yNode::new(1, Role::Button).with_label("Save");
        assert_eq!(n.label.as_deref(), Some("Save"));
    }

    #[test]
    fn node_with_checked() {
        let n = A11yNode::new(1, Role::Checkbox).with_checked(true);
        assert_eq!(n.checked, Some(true));
    }

    #[test]
    fn node_with_disabled_blocks_focus() {
        let n = A11yNode::new(1, Role::Button).with_disabled(true);
        assert!(!n.is_focusable());
    }

    #[test]
    fn node_with_value() {
        let n = A11yNode::new(1, Role::Slider).with_value("50");
        assert_eq!(n.value.as_deref(), Some("50"));
    }

    #[test]
    fn node_add_child_no_duplicate() {
        let mut n = A11yNode::new(1, Role::Dialog);
        n.add_child(2);
        n.add_child(2);
        assert_eq!(n.children.len(), 1);
    }

    #[test]
    fn node_accessible_name_some() {
        let n = A11yNode::new(1, Role::Button).with_label("OK");
        assert_eq!(n.accessible_name(), Some("OK"));
    }

    #[test]
    fn node_accessible_name_none() {
        let n = A11yNode::new(1, Role::Button);
        assert_eq!(n.accessible_name(), None);
    }

    #[test]
    fn node_with_description() {
        let n = A11yNode::new(1, Role::Image).with_description("A sunset photo");
        assert_eq!(n.description.as_deref(), Some("A sunset photo"));
    }
}
