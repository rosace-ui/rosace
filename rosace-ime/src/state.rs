use crate::event::ImeEvent;

/// State machine for IME composition flow.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ImeState {
    /// No active IME session.
    #[default]
    Idle,
    /// IME is active, composing preedit text.
    Composing { preedit: String },
    /// Composition committed — text is ready to insert.
    Committed { text: String },
    /// IME is enabled but not yet composing.
    Enabled,
}

impl ImeState {
    pub fn is_idle(&self) -> bool { matches!(self, ImeState::Idle) }
    pub fn is_composing(&self) -> bool { matches!(self, ImeState::Composing { .. }) }
    pub fn is_committed(&self) -> bool { matches!(self, ImeState::Committed { .. }) }
    pub fn is_enabled(&self) -> bool { matches!(self, ImeState::Enabled) }

    pub fn committed_text(&self) -> Option<&str> {
        if let ImeState::Committed { text } = self { Some(text) } else { None }
    }

    /// Advance the state machine based on an ImeEvent.
    pub fn transition(&mut self, event: &ImeEvent) {
        *self = match (&*self, event) {
            (_, ImeEvent::Enabled)  => ImeState::Enabled,
            (_, ImeEvent::Disabled) => ImeState::Idle,
            (_, ImeEvent::Preedit { text, .. }) if text.is_empty() => {
                if matches!(self, ImeState::Composing { .. }) {
                    ImeState::Enabled
                } else {
                    self.clone()
                }
            }
            (_, ImeEvent::Preedit { text, .. }) => {
                ImeState::Composing { preedit: text.clone() }
            }
            (_, ImeEvent::Commit(text)) => {
                ImeState::Committed { text: text.clone() }
            }
        };
    }

    /// After consuming the committed text, return to Enabled.
    pub fn consume_commit(&mut self) {
        if self.is_committed() {
            *self = ImeState::Enabled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_default_is_idle() {
        let s = ImeState::default();
        assert!(s.is_idle());
    }

    #[test]
    fn state_enabled_event_transitions_to_enabled() {
        let mut s = ImeState::Idle;
        s.transition(&ImeEvent::Enabled);
        assert!(s.is_enabled());
    }

    #[test]
    fn state_disabled_event_transitions_to_idle() {
        let mut s = ImeState::Enabled;
        s.transition(&ImeEvent::Disabled);
        assert!(s.is_idle());
    }

    #[test]
    fn state_preedit_transitions_to_composing() {
        let mut s = ImeState::Enabled;
        s.transition(&ImeEvent::Preedit { text: "abc".to_string(), cursor_range: None });
        assert!(s.is_composing());
        if let ImeState::Composing { preedit } = &s {
            assert_eq!(preedit, "abc");
        }
    }

    #[test]
    fn state_commit_transitions_to_committed() {
        let mut s = ImeState::Composing { preedit: "abc".to_string() };
        s.transition(&ImeEvent::Commit("日本".to_string()));
        assert!(s.is_committed());
    }

    #[test]
    fn state_committed_text() {
        let s = ImeState::Committed { text: "hello".to_string() };
        assert_eq!(s.committed_text(), Some("hello"));
        assert_eq!(ImeState::Idle.committed_text(), None);
    }

    #[test]
    fn state_consume_commit_returns_to_enabled() {
        let mut s = ImeState::Committed { text: "ok".to_string() };
        s.consume_commit();
        assert!(s.is_enabled());
    }

    #[test]
    fn state_is_idle() {
        assert!(ImeState::Idle.is_idle());
        assert!(!ImeState::Enabled.is_idle());
    }

    #[test]
    fn state_is_composing() {
        assert!(ImeState::Composing { preedit: "x".to_string() }.is_composing());
        assert!(!ImeState::Idle.is_composing());
    }

    #[test]
    fn state_is_committed() {
        assert!(ImeState::Committed { text: "y".to_string() }.is_committed());
        assert!(!ImeState::Idle.is_committed());
    }

    #[test]
    fn state_empty_preedit_from_composing_goes_to_enabled() {
        let mut s = ImeState::Composing { preedit: "abc".to_string() };
        s.transition(&ImeEvent::Preedit { text: "".to_string(), cursor_range: None });
        assert!(s.is_enabled());
    }

    #[test]
    fn state_clone() {
        let s = ImeState::Composing { preedit: "test".to_string() };
        let c = s.clone();
        assert_eq!(s, c);
    }
}
