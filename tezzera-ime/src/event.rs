/// An IME event from the platform.
#[derive(Debug, Clone, PartialEq)]
pub enum ImeEvent {
    /// Preedit text changed. `cursor_range` is the byte range to highlight.
    Preedit {
        text: String,
        cursor_range: Option<(usize, usize)>,
    },
    /// User confirmed composition — insert this text.
    Commit(String),
    /// IME became active.
    Enabled,
    /// IME became inactive.
    Disabled,
}

impl ImeEvent {
    pub fn is_commit(&self) -> bool { matches!(self, ImeEvent::Commit(_)) }
    pub fn is_preedit(&self) -> bool { matches!(self, ImeEvent::Preedit { .. }) }
    pub fn is_enabled(&self) -> bool { matches!(self, ImeEvent::Enabled) }
    pub fn is_disabled(&self) -> bool { matches!(self, ImeEvent::Disabled) }

    pub fn committed_text(&self) -> Option<&str> {
        if let ImeEvent::Commit(s) = self { Some(s) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ime_event_commit_is_commit() {
        let e = ImeEvent::Commit("hello".to_string());
        assert!(e.is_commit());
        assert!(!e.is_preedit());
        assert!(!e.is_enabled());
        assert!(!e.is_disabled());
    }

    #[test]
    fn ime_event_preedit_is_preedit() {
        let e = ImeEvent::Preedit { text: "abc".to_string(), cursor_range: None };
        assert!(e.is_preedit());
        assert!(!e.is_commit());
        assert!(!e.is_enabled());
        assert!(!e.is_disabled());
    }

    #[test]
    fn ime_event_enabled_is_enabled() {
        let e = ImeEvent::Enabled;
        assert!(e.is_enabled());
        assert!(!e.is_commit());
        assert!(!e.is_preedit());
        assert!(!e.is_disabled());
    }

    #[test]
    fn ime_event_disabled_is_disabled() {
        let e = ImeEvent::Disabled;
        assert!(e.is_disabled());
        assert!(!e.is_commit());
        assert!(!e.is_preedit());
        assert!(!e.is_enabled());
    }

    #[test]
    fn ime_event_committed_text_when_commit() {
        let e = ImeEvent::Commit("日本".to_string());
        assert_eq!(e.committed_text(), Some("日本"));
    }

    #[test]
    fn ime_event_committed_text_when_not_commit() {
        let e = ImeEvent::Preedit { text: "abc".to_string(), cursor_range: None };
        assert_eq!(e.committed_text(), None);
        assert_eq!(ImeEvent::Enabled.committed_text(), None);
        assert_eq!(ImeEvent::Disabled.committed_text(), None);
    }

    #[test]
    fn ime_event_clone_eq() {
        let e = ImeEvent::Commit("test".to_string());
        let cloned = e.clone();
        assert_eq!(e, cloned);
    }

    #[test]
    fn ime_event_preedit_with_cursor_range() {
        let e = ImeEvent::Preedit {
            text: "にほん".to_string(),
            cursor_range: Some((0, 9)),
        };
        assert!(e.is_preedit());
        if let ImeEvent::Preedit { text, cursor_range } = &e {
            assert_eq!(text, "にほん");
            assert_eq!(*cursor_range, Some((0, 9)));
        } else {
            panic!("Expected Preedit");
        }
    }
}
