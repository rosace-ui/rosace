//! Filesystem access for ROSACE (D125): app-local storage (read/write in
//! Documents/Cache/Temp) plus native OS file/save picker dialogs — the
//! `rosace-net` of the filesystem, same "direct platform access, no
//! Swift/Kotlin layer to cross where we can help it" philosophy.
//!
//! ```rust,ignore
//! use rosace_storage::file::{app_dir, fs, picker, AppDir, FileFilter};
//!
//! // App-local storage:
//! let dir = app_dir("My App", AppDir::Documents)?;
//! fs::write(dir.join("notes.txt"), "hello")?;
//! let text = fs::read_to_string(dir.join("notes.txt"))?;
//!
//! // User-picked file (poll once per frame, same shape as rosace_net's HttpHandle):
//! let mut handle = picker::pick_file(&[FileFilter::new("Images", &["png", "jpg"])]);
//! // each frame:
//! if let Some(path) = handle.poll() { /* Some(Some(path)) picked, Some(None) cancelled */ }
//! ```

pub mod app_dir;
pub mod fs;
pub mod picker;

pub use app_dir::{app_dir, AppDir};
pub use picker::{FileFilter, PickerHandle};
