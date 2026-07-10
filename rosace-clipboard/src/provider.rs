/// Platform-agnostic clipboard access.
pub trait ClipboardProvider {
    /// Read current clipboard text. Returns None if empty or unavailable.
    fn read(&self) -> Option<String>;

    /// Write text to the clipboard.
    fn write(&self, text: &str) -> Result<(), ClipboardError>;

    /// Clear the clipboard.
    fn clear(&self) { let _ = self.write(""); }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardError {
    Unavailable,
    CommandFailed(String),
    Unsupported,
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ClipboardError::Unavailable => write!(f, "clipboard unavailable"),
            ClipboardError::CommandFailed(e) => write!(f, "clipboard command failed: {e}"),
            ClipboardError::Unsupported => write!(f, "clipboard not supported on this platform"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_error_display_unavailable() {
        assert_eq!(format!("{}", ClipboardError::Unavailable), "clipboard unavailable");
    }

    #[test]
    fn clipboard_error_display_command_failed() {
        let e = ClipboardError::CommandFailed("no such command".to_string());
        assert_eq!(format!("{}", e), "clipboard command failed: no such command");
    }

    #[test]
    fn clipboard_error_display_unsupported() {
        assert_eq!(format!("{}", ClipboardError::Unsupported), "clipboard not supported on this platform");
    }

    #[test]
    fn clipboard_error_clone_eq() {
        let a = ClipboardError::CommandFailed("err".to_string());
        let b = a.clone();
        assert_eq!(a, b);
    }
}
