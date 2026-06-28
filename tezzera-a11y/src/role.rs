/// ARIA-inspired role for accessibility tree nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Button,
    Checkbox,
    Link,
    Heading,
    Image,
    Text,
    TextInput,
    List,
    ListItem,
    Dialog,
    Menu,
    MenuItem,
    ProgressBar,
    Slider,
    Tab,
    TabPanel,
    None,
}

impl Role {
    pub fn is_interactive(&self) -> bool {
        matches!(self,
            Role::Button | Role::Checkbox | Role::Link | Role::TextInput
            | Role::MenuItem | Role::Slider | Role::Tab
        )
    }

    pub fn is_container(&self) -> bool {
        matches!(self, Role::List | Role::Dialog | Role::Menu | Role::TabPanel)
    }

    pub fn name(&self) -> &'static str {
        match self {
            Role::Button      => "button",
            Role::Checkbox    => "checkbox",
            Role::Link        => "link",
            Role::Heading     => "heading",
            Role::Image       => "img",
            Role::Text        => "text",
            Role::TextInput   => "textbox",
            Role::List        => "list",
            Role::ListItem    => "listitem",
            Role::Dialog      => "dialog",
            Role::Menu        => "menu",
            Role::MenuItem    => "menuitem",
            Role::ProgressBar => "progressbar",
            Role::Slider      => "slider",
            Role::Tab         => "tab",
            Role::TabPanel    => "tabpanel",
            Role::None        => "none",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_is_interactive_button() {
        assert!(Role::Button.is_interactive());
    }

    #[test]
    fn role_is_interactive_text_is_false() {
        assert!(!Role::Text.is_interactive());
    }

    #[test]
    fn role_is_container_list() {
        assert!(Role::List.is_container());
    }

    #[test]
    fn role_is_container_button_is_false() {
        assert!(!Role::Button.is_container());
    }

    #[test]
    fn role_name_button() {
        assert_eq!(Role::Button.name(), "button");
    }

    #[test]
    fn role_name_textbox() {
        assert_eq!(Role::TextInput.name(), "textbox");
    }

    #[test]
    fn role_name_none() {
        assert_eq!(Role::None.name(), "none");
    }

    #[test]
    fn role_debug() {
        let s = format!("{:?}", Role::Button);
        assert_eq!(s, "Button");
    }
}
