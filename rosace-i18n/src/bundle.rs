use std::collections::HashMap;

/// A collection of translated strings for one locale.
/// Keys and values are plain strings; format is `key=value` per line.
#[derive(Debug, Clone)]
pub struct MessageBundle {
    messages: HashMap<String, String>,
    pub locale: super::locale::Locale,
}

impl MessageBundle {
    pub fn new(locale: super::locale::Locale) -> Self {
        Self { messages: HashMap::new(), locale }
    }

    /// Load from a `key=value` formatted string (one entry per line, # for comments).
    pub fn from_str(locale: super::locale::Locale, content: &str) -> Self {
        let mut bundle = Self::new(locale);
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() { continue; }
            if let Some((key, value)) = line.split_once('=') {
                bundle.messages.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
        bundle
    }

    /// Look up a key. Returns the key itself if not found (graceful fallback).
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.messages.get(key).map(|s| s.as_str()).unwrap_or(key)
    }

    /// Insert a key-value pair.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.messages.insert(key.into(), value.into());
    }

    pub fn contains(&self, key: &str) -> bool { self.messages.contains_key(key) }
    pub fn len(&self) -> usize { self.messages.len() }
    pub fn is_empty(&self) -> bool { self.messages.is_empty() }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::Locale;

    #[test]
    fn bundle_new_empty() {
        let b = MessageBundle::new(Locale::english());
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn bundle_insert_and_get() {
        let mut b = MessageBundle::new(Locale::english());
        b.insert("hello", "Hello");
        assert_eq!(b.get("hello"), "Hello");
    }

    #[test]
    fn bundle_from_str_parses_key_value() {
        let b = MessageBundle::from_str(Locale::french(), "greeting = Bonjour\nfarewell = Au revoir");
        assert_eq!(b.get("greeting"), "Bonjour");
        assert_eq!(b.get("farewell"), "Au revoir");
    }

    #[test]
    fn bundle_from_str_skips_comments() {
        let b = MessageBundle::from_str(Locale::english(), "# this is a comment\nhello = Hi");
        assert!(!b.contains("# this is a comment"));
        assert_eq!(b.get("hello"), "Hi");
    }

    #[test]
    fn bundle_from_str_skips_empty_lines() {
        let content = "\nhello = Hi\n\nworld = World\n";
        let b = MessageBundle::from_str(Locale::english(), content);
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn bundle_get_missing_returns_key() {
        let b = MessageBundle::new(Locale::english());
        assert_eq!(b.get("nonexistent_key"), "nonexistent_key");
    }

    #[test]
    fn bundle_contains() {
        let mut b = MessageBundle::new(Locale::english());
        b.insert("title", "My App");
        assert!(b.contains("title"));
        assert!(!b.contains("subtitle"));
    }

    #[test]
    fn bundle_len() {
        let mut b = MessageBundle::new(Locale::english());
        b.insert("a", "A");
        b.insert("b", "B");
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn bundle_is_empty() {
        let b = MessageBundle::new(Locale::english());
        assert!(b.is_empty());
        let mut b2 = MessageBundle::new(Locale::english());
        b2.insert("k", "v");
        assert!(!b2.is_empty());
    }

    #[test]
    fn bundle_from_str_trims_whitespace() {
        let b = MessageBundle::from_str(Locale::english(), "  key  =  value  ");
        assert_eq!(b.get("key"), "value");
    }
}
