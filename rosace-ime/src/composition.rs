/// Tracks the in-progress IME preedit text and cursor within it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImeComposition {
    pub text: String,
    pub cursor: usize,         // byte offset within `text`
    pub highlight: Option<(usize, usize)>,  // byte range to underline
    pub active: bool,
}

impl ImeComposition {
    pub fn new() -> Self { Self::default() }

    /// Update composition from a Preedit event.
    pub fn update(&mut self, text: impl Into<String>, cursor_range: Option<(usize, usize)>) {
        self.text = text.into();
        self.highlight = cursor_range;
        self.cursor = cursor_range.map(|(s, _)| s).unwrap_or(self.text.len());
        self.active = !self.text.is_empty();
    }

    /// Clear composition (after commit or cancel).
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.highlight = None;
        self.active = false;
    }

    /// Preedit text length in bytes.
    pub fn len(&self) -> usize { self.text.len() }

    pub fn is_empty(&self) -> bool { self.text.is_empty() }

    /// The highlighted portion of preedit text.
    pub fn highlighted_text(&self) -> &str {
        match self.highlight {
            Some((s, e)) => {
                let s = s.min(self.text.len());
                let e = e.min(self.text.len());
                &self.text[s..e]
            }
            None => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_new_empty() {
        let c = ImeComposition::new();
        assert!(c.text.is_empty());
        assert_eq!(c.cursor, 0);
        assert_eq!(c.highlight, None);
        assert!(!c.active);
    }

    #[test]
    fn composition_update_sets_text() {
        let mut c = ImeComposition::new();
        c.update("にほん", None);
        assert_eq!(c.text, "にほん");
        assert!(c.active);
    }

    #[test]
    fn composition_update_sets_cursor() {
        let mut c = ImeComposition::new();
        c.update("abc", Some((1, 3)));
        assert_eq!(c.cursor, 1);
    }

    #[test]
    fn composition_update_sets_highlight() {
        let mut c = ImeComposition::new();
        c.update("hello", Some((1, 4)));
        assert_eq!(c.highlight, Some((1, 4)));
    }

    #[test]
    fn composition_clear_resets() {
        let mut c = ImeComposition::new();
        c.update("text", Some((0, 4)));
        c.clear();
        assert!(c.text.is_empty());
        assert_eq!(c.cursor, 0);
        assert_eq!(c.highlight, None);
        assert!(!c.active);
    }

    #[test]
    fn composition_is_active_when_non_empty() {
        let mut c = ImeComposition::new();
        c.update("a", None);
        assert!(c.active);
    }

    #[test]
    fn composition_is_inactive_when_empty() {
        let mut c = ImeComposition::new();
        c.update("", None);
        assert!(!c.active);
    }

    #[test]
    fn composition_highlighted_text() {
        let mut c = ImeComposition::new();
        c.update("hello world", Some((6, 11)));
        assert_eq!(c.highlighted_text(), "world");
    }

    #[test]
    fn composition_len() {
        let mut c = ImeComposition::new();
        c.update("abc", None);
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn composition_is_empty() {
        let c = ImeComposition::new();
        assert!(c.is_empty());
        let mut c2 = ImeComposition::new();
        c2.update("x", None);
        assert!(!c2.is_empty());
    }
}
