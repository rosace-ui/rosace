use crate::composition::ImeComposition;
use crate::event::ImeEvent;
use crate::state::ImeState;

/// Handles IME events and maintains composition + state.
pub trait ImeHandler {
    fn on_ime_event(&mut self, event: &ImeEvent);
    fn composition(&self) -> &ImeComposition;
    fn state(&self) -> &ImeState;
}

/// A simple IME handler that converts commit events to plain strings.
/// Suitable for non-CJK input or testing.
#[derive(Debug, Default)]
pub struct NoopIme {
    composition: ImeComposition,
    state: ImeState,
    committed: Vec<String>,
}

impl NoopIme {
    pub fn new() -> Self { Self::default() }

    /// All committed strings (in order received).
    pub fn committed_texts(&self) -> &[String] { &self.committed }

    /// The most recently committed text, if any.
    pub fn last_committed(&self) -> Option<&str> {
        self.committed.last().map(|s| s.as_str())
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        self.composition.clear();
        self.state = ImeState::Idle;
        self.committed.clear();
    }
}

impl ImeHandler for NoopIme {
    fn on_ime_event(&mut self, event: &ImeEvent) {
        match event {
            ImeEvent::Preedit { text, cursor_range } => {
                self.composition.update(text.clone(), *cursor_range);
            }
            ImeEvent::Commit(text) => {
                self.committed.push(text.clone());
                self.composition.clear();
            }
            ImeEvent::Enabled | ImeEvent::Disabled => {
                self.composition.clear();
            }
        }
        self.state.transition(event);
    }

    fn composition(&self) -> &ImeComposition { &self.composition }
    fn state(&self) -> &ImeState { &self.state }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_ime_new() {
        let ime = NoopIme::new();
        assert!(ime.composition().is_empty());
        assert!(ime.state().is_idle());
        assert!(ime.committed_texts().is_empty());
    }

    #[test]
    fn noop_ime_on_enabled() {
        let mut ime = NoopIme::new();
        ime.on_ime_event(&ImeEvent::Enabled);
        assert!(ime.state().is_enabled());
        assert!(ime.composition().is_empty());
    }

    #[test]
    fn noop_ime_on_preedit() {
        let mut ime = NoopIme::new();
        ime.on_ime_event(&ImeEvent::Enabled);
        ime.on_ime_event(&ImeEvent::Preedit {
            text: "にほん".to_string(),
            cursor_range: Some((0, 9)),
        });
        assert!(ime.state().is_composing());
        assert_eq!(ime.composition().text, "にほん");
        assert!(ime.composition().active);
    }

    #[test]
    fn noop_ime_on_commit() {
        let mut ime = NoopIme::new();
        ime.on_ime_event(&ImeEvent::Enabled);
        ime.on_ime_event(&ImeEvent::Preedit {
            text: "にほん".to_string(),
            cursor_range: None,
        });
        ime.on_ime_event(&ImeEvent::Commit("日本".to_string()));
        assert!(ime.state().is_committed());
        assert!(ime.composition().is_empty());
    }

    #[test]
    fn noop_ime_committed_texts() {
        let mut ime = NoopIme::new();
        ime.on_ime_event(&ImeEvent::Commit("hello".to_string()));
        ime.on_ime_event(&ImeEvent::Commit("world".to_string()));
        assert_eq!(ime.committed_texts(), &["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn noop_ime_last_committed() {
        let mut ime = NoopIme::new();
        assert_eq!(ime.last_committed(), None);
        ime.on_ime_event(&ImeEvent::Commit("first".to_string()));
        ime.on_ime_event(&ImeEvent::Commit("second".to_string()));
        assert_eq!(ime.last_committed(), Some("second"));
    }

    #[test]
    fn noop_ime_reset() {
        let mut ime = NoopIme::new();
        ime.on_ime_event(&ImeEvent::Enabled);
        ime.on_ime_event(&ImeEvent::Commit("text".to_string()));
        ime.reset();
        assert!(ime.state().is_idle());
        assert!(ime.composition().is_empty());
        assert!(ime.committed_texts().is_empty());
    }

    #[test]
    fn noop_ime_multiple_commits() {
        let mut ime = NoopIme::new();
        for word in &["alpha", "beta", "gamma"] {
            ime.on_ime_event(&ImeEvent::Commit(word.to_string()));
        }
        assert_eq!(ime.committed_texts().len(), 3);
        assert_eq!(ime.last_committed(), Some("gamma"));
    }
}
