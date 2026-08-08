//! OS file/save picker dialogs — the user-initiated counterpart to
//! [`crate::fs`]'s app-local reads/writes. Non-blocking by the same
//! `std::thread` + `mpsc` + poll-once-per-frame convention `rosace-net`'s
//! `HttpClient::fetch`/`HttpHandle` already establish (a modal native
//! dialog blocks the thread it runs on for as long as the user takes to
//! decide, which must never be the UI thread).
//!
//! # Platform coverage
//! - **macOS / Windows / Linux**: real native dialogs (AppKit / Win32
//!   common dialogs / GTK or the XDG portal, via the `rfd` crate).
//! - **iOS / Android**: a NAMED GAP. A real picker needs
//!   `UIDocumentPickerViewController` (iOS) or the Storage Access
//!   Framework (Android) — genuinely native UI on the Swift/Kotlin side of
//!   the existing Platform Channel bridge (`rosace-ffi`), not something a
//!   pure-Rust crate can provide. Every call resolves to `None`/empty
//!   immediately with a logged reason instead of hanging or panicking.
//!   Wiring the native side is real follow-up work, not a silent omission.
//! - **wasm32 (web)**: also a named gap for now — `std::thread::spawn`
//!   panics on this target (the same constraint `rosace-net`'s HTTP client
//!   documents), so even a `<input type=file>`-backed implementation needs
//!   a differently-shaped (non-thread) async path than this module uses.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use std::thread;

/// A named extension filter for a picker dialog, e.g.
/// `FileFilter::new("Images", &["png", "jpg", "jpeg"])`.
#[derive(Debug, Clone)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

impl FileFilter {
    pub fn new(name: impl Into<String>, extensions: &[&str]) -> Self {
        Self { name: name.into(), extensions: extensions.iter().map(|s| s.to_string()).collect() }
    }
}

/// A pending picker result — poll once per frame, same convention as
/// `rosace_net::HttpHandle`. Dropping the handle before it resolves just
/// discards the eventual result; the OS dialog (if one is open) is
/// unaffected since it's driven by its own native event loop, not this
/// handle.
pub struct PickerHandle<T> {
    rx: Receiver<T>,
    done: bool,
}

impl<T> PickerHandle<T> {
    /// Returns `Some` exactly once, when the user closes the dialog
    /// (whether they picked something or cancelled); `None` while it's
    /// still open (or after the result was already taken).
    pub fn poll(&mut self) -> Option<T> {
        if self.done {
            return None;
        }
        match self.rx.try_recv() {
            Ok(result) => {
                self.done = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.done = true;
                None
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn dialog(filters: &[FileFilter]) -> rfd::FileDialog {
    let mut d = rfd::FileDialog::new();
    for f in filters {
        d = d.add_filter(&f.name, &f.extensions.iter().map(String::as_str).collect::<Vec<_>>());
    }
    d
}

/// Open a native "choose one file" dialog. `None` if the user cancelled.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub fn pick_file(filters: &[FileFilter]) -> PickerHandle<Option<PathBuf>> {
    let d = dialog(filters);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || { let _ = tx.send(d.pick_file()); });
    PickerHandle { rx, done: false }
}

/// Open a native "choose one or more files" dialog. Empty if cancelled.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub fn pick_files(filters: &[FileFilter]) -> PickerHandle<Vec<PathBuf>> {
    let d = dialog(filters);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || { let _ = tx.send(d.pick_files().unwrap_or_default()); });
    PickerHandle { rx, done: false }
}

/// Open a native "choose a folder" dialog. `None` if cancelled.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub fn pick_folder() -> PickerHandle<Option<PathBuf>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || { let _ = tx.send(rfd::FileDialog::new().pick_folder()); });
    PickerHandle { rx, done: false }
}

/// Open a native "save as" dialog — returns the chosen destination path
/// WITHOUT writing anything; pass it to [`crate::fs::write`] yourself.
/// `None` if cancelled.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub fn save_file(default_name: &str, filters: &[FileFilter]) -> PickerHandle<Option<PathBuf>> {
    let d = dialog(filters).set_file_name(default_name);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || { let _ = tx.send(d.save_file()); });
    PickerHandle { rx, done: false }
}

// iOS / Android / wasm32: named gap (see module doc). Every call resolves
// immediately to an empty result instead of hanging or panicking
// (`std::thread::spawn` itself panics at runtime on wasm32).
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod gap {
    use super::*;

    fn handle_from<T: Send + 'static>(value: T) -> PickerHandle<T> {
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(value);
        PickerHandle { rx, done: false }
    }

    fn log_gap(fn_name: &str) {
        rosace_trace::warn!(
            "rosace-file::picker::{fn_name} has no native picker on this platform yet \
             (iOS/Android need a Swift/Kotlin picker through rosace-ffi; wasm32 needs a \
             non-thread-based path) — resolving to an empty result"
        );
    }

    pub fn pick_file(_filters: &[FileFilter]) -> PickerHandle<Option<PathBuf>> {
        log_gap("pick_file");
        handle_from(None)
    }
    pub fn pick_files(_filters: &[FileFilter]) -> PickerHandle<Vec<PathBuf>> {
        log_gap("pick_files");
        handle_from(Vec::new())
    }
    pub fn pick_folder() -> PickerHandle<Option<PathBuf>> {
        log_gap("pick_folder");
        handle_from(None)
    }
    pub fn save_file(_default_name: &str, _filters: &[FileFilter]) -> PickerHandle<Option<PathBuf>> {
        log_gap("save_file");
        handle_from(None)
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub use gap::*;
