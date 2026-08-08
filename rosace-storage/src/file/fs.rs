//! Read/write for app-local files — plain `std::fs` under a small,
//! consistent `Result<_, String>` surface (matching `rosace-storage`'s and
//! `rosace-net`'s own error shape), so callers don't need `std::io::Error`
//! in scope just to report a failure to the user.
//!
//! Works on any path, not just ones from [`crate::app_dir`] — including a
//! path a user picked via [`crate::picker`], which is exactly why these
//! aren't folded into `app_dir` itself.

use std::path::Path;

/// Read a whole file into memory.
#[cfg(not(target_arch = "wasm32"))]
pub fn read(path: impl AsRef<Path>) -> Result<Vec<u8>, String> {
    let path = path.as_ref();
    std::fs::read(path).map_err(|e| format!("read {}: {}", path.display(), e))
}

/// Read a whole file as UTF-8 text.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_to_string(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();
    std::fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))
}

/// Write `data`, replacing the file if it already exists — creates the
/// file but NOT its parent directories (call [`create_dir_all`] first if
/// they might not exist, same as `std::fs::write`'s own contract).
#[cfg(not(target_arch = "wasm32"))]
pub fn write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> Result<(), String> {
    let path = path.as_ref();
    std::fs::write(path, data).map_err(|e| format!("write {}: {}", path.display(), e))
}

/// Append to a file, creating it if absent.
#[cfg(not(target_arch = "wasm32"))]
pub fn append(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> Result<(), String> {
    use std::io::Write;
    let path = path.as_ref();
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(data.as_ref()))
        .map_err(|e| format!("append {}: {}", path.display(), e))
}

/// Delete a file. NOT recursive and NOT for directories — see
/// [`remove_dir_all`] for that.
#[cfg(not(target_arch = "wasm32"))]
pub fn delete(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    std::fs::remove_file(path).map_err(|e| format!("delete {}: {}", path.display(), e))
}

/// Delete a directory and everything under it.
#[cfg(not(target_arch = "wasm32"))]
pub fn remove_dir_all(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    std::fs::remove_dir_all(path).map_err(|e| format!("remove {}: {}", path.display(), e))
}

/// Whether a path exists (file or directory).
#[cfg(not(target_arch = "wasm32"))]
pub fn exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

/// Create a directory and any missing parent directories — a no-op if it
/// already exists.
#[cfg(not(target_arch = "wasm32"))]
pub fn create_dir_all(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    std::fs::create_dir_all(path).map_err(|e| format!("create {}: {}", path.display(), e))
}

/// Every entry directly inside a directory (not recursive).
#[cfg(not(target_arch = "wasm32"))]
pub fn list_dir(path: impl AsRef<Path>) -> Result<Vec<std::path::PathBuf>, String> {
    let path = path.as_ref();
    std::fs::read_dir(path)
        .map_err(|e| format!("list {}: {}", path.display(), e))?
        .map(|entry| entry.map(|e| e.path()).map_err(|e| format!("list {}: {}", path.display(), e)))
        .collect()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rosace_file_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir = temp_dir("round_trip");
        let file = dir.join("hello.txt");
        write(&file, "hello world").unwrap();
        assert_eq!(read_to_string(&file).unwrap(), "hello world");
        assert_eq!(read(&file).unwrap(), b"hello world");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_adds_without_overwriting() {
        let dir = temp_dir("append");
        let file = dir.join("log.txt");
        write(&file, "a").unwrap();
        append(&file, "b").unwrap();
        append(&file, "c").unwrap();
        assert_eq!(read_to_string(&file).unwrap(), "abc");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_the_file_and_exists_reflects_it() {
        let dir = temp_dir("delete");
        let file = dir.join("gone.txt");
        write(&file, "x").unwrap();
        assert!(exists(&file));
        delete(&file).unwrap();
        assert!(!exists(&file));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_dir_finds_every_direct_entry() {
        let dir = temp_dir("list");
        write(dir.join("a.txt"), "1").unwrap();
        write(dir.join("b.txt"), "2").unwrap();
        create_dir_all(dir.join("subdir")).unwrap();
        let entries = list_dir(&dir).unwrap();
        assert_eq!(entries.len(), 3, "expected a.txt, b.txt, subdir, got {entries:?}");
        assert!(entries.iter().any(|p| p.file_name().unwrap() == "a.txt"));
        assert!(entries.iter().any(|p| p.file_name().unwrap() == "subdir"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_a_missing_file_is_a_clear_error_not_a_panic() {
        let dir = temp_dir("missing");
        let err = read(dir.join("nope.txt")).unwrap_err();
        assert!(err.contains("nope.txt"), "error should name the path, got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// wasm32: named gap — no host filesystem to operate on (see this crate's
// `app_dir` module doc). Every function returns the same clear error
// instead of a cryptic OS-level one std::fs would otherwise produce.
#[cfg(target_arch = "wasm32")]
mod wasm_gap {
    const MSG: &str = "rosace-file: file read/write is not yet implemented on web (wasm32) — \
                        needs an OPFS/IndexedDB-backed store, future work";

    pub fn read(_path: impl AsRef<std::path::Path>) -> Result<Vec<u8>, String> { Err(MSG.to_string()) }
    pub fn read_to_string(_path: impl AsRef<std::path::Path>) -> Result<String, String> { Err(MSG.to_string()) }
    pub fn write(_path: impl AsRef<std::path::Path>, _data: impl AsRef<[u8]>) -> Result<(), String> { Err(MSG.to_string()) }
    pub fn append(_path: impl AsRef<std::path::Path>, _data: impl AsRef<[u8]>) -> Result<(), String> { Err(MSG.to_string()) }
    pub fn delete(_path: impl AsRef<std::path::Path>) -> Result<(), String> { Err(MSG.to_string()) }
    pub fn remove_dir_all(_path: impl AsRef<std::path::Path>) -> Result<(), String> { Err(MSG.to_string()) }
    pub fn exists(_path: impl AsRef<std::path::Path>) -> bool { false }
    pub fn create_dir_all(_path: impl AsRef<std::path::Path>) -> Result<(), String> { Err(MSG.to_string()) }
    pub fn list_dir(_path: impl AsRef<std::path::Path>) -> Result<Vec<std::path::PathBuf>, String> { Err(MSG.to_string()) }
}
#[cfg(target_arch = "wasm32")]
pub use wasm_gap::*;
