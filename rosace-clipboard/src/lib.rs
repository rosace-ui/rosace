//! OS clipboard integration for ROSACE.
//!
//! Uses `pbcopy`/`pbpaste` on macOS and `xclip`/`xsel` on Linux.
//! `NoopClipboard` is available for testing and WASM targets.
//!
//! # Example
//! ```rust,ignore
//! use rosace_clipboard::{SystemClipboard, ClipboardProvider};
//!
//! let cb = SystemClipboard::new();
//! cb.write("Hello from ROSACE").unwrap();
//! let text = cb.read(); // Some("Hello from ROSACE") on macOS/Linux
//! ```

pub mod noop;
pub mod provider;
pub mod system;

pub use noop::NoopClipboard;
pub use provider::{ClipboardError, ClipboardProvider};
pub use system::SystemClipboard;

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn noop_round_trip() {
        let cb = NoopClipboard::new();
        cb.write("hello").unwrap();
        assert_eq!(cb.read().as_deref(), Some("hello"));
    }

    #[test]
    fn noop_multiple_writes() {
        let cb = NoopClipboard::new();
        cb.write("first").unwrap();
        cb.write("second").unwrap();
        assert_eq!(cb.read().as_deref(), Some("second"));
    }

    #[test]
    fn provider_trait_object() {
        let cb: Box<dyn ClipboardProvider> = Box::new(NoopClipboard::new());
        cb.write("via trait").unwrap();
        assert_eq!(cb.read().as_deref(), Some("via trait"));
    }

    #[test]
    fn clear_via_trait() {
        let cb: Box<dyn ClipboardProvider> = Box::new(NoopClipboard::new());
        cb.write("data").unwrap();
        cb.clear();
        assert_eq!(cb.read().as_deref(), Some(""));
    }
}
