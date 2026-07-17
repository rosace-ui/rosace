// The CLI-tool clipboard is desktop-only (pbcopy/xclip); on other targets
// (Android, iOS, …) these std imports would be unused, so gate them to the
// platforms whose impls actually use them.
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::io::Write;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::{Command, Stdio};
use crate::provider::{ClipboardError, ClipboardProvider};

/// System clipboard using platform CLI tools.
///
/// - macOS: `pbpaste` / `pbcopy`
/// - Linux: `xclip -selection clipboard` (falls back to `xsel`)
/// - Other: returns `ClipboardError::Unsupported`
pub struct SystemClipboard;

impl SystemClipboard {
    pub fn new() -> Self { Self }

    #[cfg(target_os = "macos")]
    fn read_impl(&self) -> Option<String> {
        Command::new("pbpaste")
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(o.stdout) } else { None })
            .and_then(|b| String::from_utf8(b).ok())
    }

    #[cfg(target_os = "macos")]
    fn write_impl(&self, text: &str) -> Result<(), ClipboardError> {
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| ClipboardError::CommandFailed(e.to_string()))?;
        child.stdin.as_mut().unwrap()
            .write_all(text.as_bytes())
            .map_err(|e| ClipboardError::CommandFailed(e.to_string()))?;
        child.wait().map_err(|e| ClipboardError::CommandFailed(e.to_string()))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn read_impl(&self) -> Option<String> {
        Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(o.stdout) } else { None })
            .or_else(|| {
                Command::new("xsel")
                    .args(["--clipboard", "--output"])
                    .output()
                    .ok()
                    .and_then(|o| if o.status.success() { Some(o.stdout) } else { None })
            })
            .and_then(|b| String::from_utf8(b).ok())
    }

    #[cfg(target_os = "linux")]
    fn write_impl(&self, text: &str) -> Result<(), ClipboardError> {
        let mut child = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| ClipboardError::CommandFailed(e.to_string()))?;
        child.stdin.as_mut().unwrap()
            .write_all(text.as_bytes())
            .map_err(|e| ClipboardError::CommandFailed(e.to_string()))?;
        child.wait().map_err(|e| ClipboardError::CommandFailed(e.to_string()))?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn read_impl(&self) -> Option<String> { None }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn write_impl(&self, _text: &str) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unsupported)
    }
}

impl Default for SystemClipboard { fn default() -> Self { Self::new() } }

impl ClipboardProvider for SystemClipboard {
    fn read(&self) -> Option<String> { self.read_impl() }
    fn write(&self, text: &str) -> Result<(), ClipboardError> { self.write_impl(text) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ClipboardProvider;

    #[test]
    fn system_clipboard_new() {
        let _cb = SystemClipboard::new();
    }

    #[test]
    fn system_clipboard_default() {
        let _cb = SystemClipboard;
    }

    #[test]
    fn system_clipboard_write_no_panic() {
        let cb = SystemClipboard::new();
        let _ = cb.write("rosace test");
    }

    #[test]
    fn system_clipboard_read_returns_option() {
        let cb = SystemClipboard::new();
        let _ = cb.read(); // may be Some or None — must not panic
    }

    #[test]
    fn system_clipboard_clear_no_panic() {
        let cb = SystemClipboard::new();
        cb.clear();
    }

    #[test]
    fn system_clipboard_is_object_safe() {
        let _boxed: Box<dyn ClipboardProvider> = Box::new(SystemClipboard::new());
    }
}
