use crate::provider::{ClipboardError, ClipboardProvider};

/// A clipboard provider that does nothing. Useful for testing and WASM.
#[derive(Debug, Default, Clone)]
pub struct NoopClipboard {
    inner: std::cell::RefCell<Option<String>>,
}

impl NoopClipboard {
    pub fn new() -> Self { Self { inner: std::cell::RefCell::new(None) } }
}

impl ClipboardProvider for NoopClipboard {
    fn read(&self) -> Option<String> { self.inner.borrow().clone() }
    fn write(&self, text: &str) -> Result<(), ClipboardError> {
        *self.inner.borrow_mut() = Some(text.to_string());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ClipboardProvider;

    #[test]
    fn noop_read_empty_initially() {
        let cb = NoopClipboard::new();
        assert!(cb.read().is_none());
    }

    #[test]
    fn noop_write_stores_value() {
        let cb = NoopClipboard::new();
        cb.write("hello").unwrap();
        assert_eq!(cb.read().as_deref(), Some("hello"));
    }

    #[test]
    fn noop_read_after_write() {
        let cb = NoopClipboard::new();
        cb.write("world").unwrap();
        assert_eq!(cb.read().as_deref(), Some("world"));
    }

    #[test]
    fn noop_write_overwrites() {
        let cb = NoopClipboard::new();
        cb.write("first").unwrap();
        cb.write("second").unwrap();
        assert_eq!(cb.read().as_deref(), Some("second"));
    }

    #[test]
    fn noop_clear_removes_value() {
        let cb = NoopClipboard::new();
        cb.write("data").unwrap();
        cb.clear();
        // clear() writes "" — so read returns Some("")
        assert_eq!(cb.read().as_deref(), Some(""));
    }

    #[test]
    fn noop_write_empty_string() {
        let cb = NoopClipboard::new();
        cb.write("").unwrap();
        assert_eq!(cb.read().as_deref(), Some(""));
    }

    #[test]
    fn noop_clone() {
        let cb = NoopClipboard::new();
        cb.write("abc").unwrap();
        let cb2 = cb.clone();
        assert_eq!(cb2.read().as_deref(), Some("abc"));
    }

    #[test]
    fn noop_is_object_safe() {
        let cb = NoopClipboard::new();
        cb.write("test").unwrap();
        let boxed: Box<dyn ClipboardProvider> = Box::new(cb);
        assert_eq!(boxed.read().as_deref(), Some("test"));
    }
}
