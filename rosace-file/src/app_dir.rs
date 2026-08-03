//! Per-platform app-local directories — generalizes the directory
//! resolution `rosace/src/lib.rs`'s `persist_db_path` already does for the
//! SQLite persistence store, so both features agree on where "this app's
//! own storage" lives instead of drifting apart.

use std::path::PathBuf;

/// Which app-local directory to resolve. Mirrors the three buckets every
/// mobile OS already distinguishes (and desktop OSes have equivalents
/// for): durable user data, OS-evictable cache, and scratch space that
/// may not survive a relaunch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppDir {
    /// Durable, user-relevant files — survives app updates, backed up by
    /// the OS where that's a platform convention (e.g. iCloud/iOS).
    /// `~/Library/Application Support/<app>` (macOS), `%APPDATA%\<app>`
    /// (Windows), `$XDG_DATA_HOME|~/.local/share/<app>` (Linux),
    /// `$HOME/Documents` (iOS, sandboxed to the app by construction).
    Documents,
    /// OS-evictable cache — appropriate for anything regenerable
    /// (thumbnails, downloaded-but-refetchable data). The OS may clear
    /// this under storage pressure; never put data here you can't afford
    /// to lose. `~/Library/Caches/<app>` (macOS), `%LOCALAPPDATA%\<app>\Cache`
    /// (Windows), `$XDG_CACHE_HOME|~/.cache/<app>` (Linux).
    Cache,
    /// Scratch space for the current run — may not survive even this
    /// session on some platforms; never rely on it past immediate use.
    /// The OS temp dir, namespaced per-app (`std::env::temp_dir()/<app>`
    /// on every desktop platform).
    Temp,
}

/// Resolve `kind`'s directory for `app_title`, creating it if absent. The
/// app title is sanitized to a filesystem-safe directory name — same
/// sanitization `persist_db_path` uses, so both land in the same parent
/// folder per app.
///
/// # Platform gaps (named, not silent)
/// - **Android**: the files/cache dirs must come from the JNI host
///   (`Context.getFilesDir()`/`getCacheDir()`), not an env var — same
///   deferral `persist_db_path` already documents. Returns `Err`.
/// - **wasm32 (web)**: no host filesystem to resolve a path into at all
///   — a real backend needs OPFS/IndexedDB, future work. Returns `Err`.
#[cfg(not(target_arch = "wasm32"))]
pub fn app_dir(app_title: &str, kind: AppDir) -> Result<PathBuf, String> {
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux", target_os = "ios")))]
    {
        let _ = (app_title, kind);
        return Err("rosace-file: no app-directory convention for this platform yet \
                     (Android needs Context.getFilesDir()/getCacheDir() from the JNI \
                     host — see the same deferral on persist_db_path in rosace/src/lib.rs)"
            .to_string());
    }

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux", target_os = "ios"))]
    {
        let app_dir_name: String = app_title
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();

        if matches!(kind, AppDir::Temp) {
            let dir = std::env::temp_dir().join(&app_dir_name);
            std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {}", dir.display(), e))?;
            return Ok(dir);
        }

        let home = || std::env::var("HOME").map_err(|_| "no HOME env var".to_string());

        #[cfg(target_os = "macos")]
        let base = {
            let lib = home().map(|h| PathBuf::from(h).join("Library"))?;
            match kind {
                AppDir::Documents => lib.join("Application Support"),
                AppDir::Cache => lib.join("Caches"),
                AppDir::Temp => unreachable!(),
            }
        };
        #[cfg(target_os = "windows")]
        let base = match kind {
            AppDir::Documents => std::env::var("APPDATA")
                .map(PathBuf::from)
                .map_err(|_| "no APPDATA env var".to_string())?,
            AppDir::Cache => std::env::var("LOCALAPPDATA")
                .map(|p| PathBuf::from(p).join("Cache"))
                .map_err(|_| "no LOCALAPPDATA env var".to_string())?,
            AppDir::Temp => unreachable!(),
        };
        #[cfg(target_os = "linux")]
        let base = match kind {
            AppDir::Documents => match std::env::var("XDG_DATA_HOME") {
                Ok(x) => PathBuf::from(x),
                Err(_) => PathBuf::from(home()?).join(".local/share"),
            },
            AppDir::Cache => match std::env::var("XDG_CACHE_HOME") {
                Ok(x) => PathBuf::from(x),
                Err(_) => PathBuf::from(home()?).join(".cache"),
            },
            AppDir::Temp => unreachable!(),
        };
        #[cfg(target_os = "ios")]
        let base = {
            // iOS sandboxes the whole home dir to this app already — no
            // separate Caches convention needed the way desktop has one;
            // `Library/Caches` is still the OS-evictable bucket there too.
            let home = PathBuf::from(home()?);
            match kind {
                AppDir::Documents => home.join("Documents"),
                AppDir::Cache => home.join("Library/Caches"),
                AppDir::Temp => unreachable!(),
            }
        };

        let dir = base.join(&app_dir_name);
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {}", dir.display(), e))?;
        Ok(dir)
    }
}

/// wasm32: named gap — see this function's own doc comment above.
#[cfg(target_arch = "wasm32")]
pub fn app_dir(_app_title: &str, _kind: AppDir) -> Result<PathBuf, String> {
    Err("rosace-file: app-local storage is not yet implemented on web (wasm32) — \
         needs an OPFS/IndexedDB-backed store, future work"
        .to_string())
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows", target_os = "linux", target_os = "ios")))]
mod tests {
    use super::*;

    #[test]
    fn each_kind_resolves_to_an_existing_directory() {
        for kind in [AppDir::Documents, AppDir::Cache, AppDir::Temp] {
            let dir = app_dir("Rosace File Test", kind).unwrap();
            assert!(dir.is_dir(), "{kind:?} -> {dir:?} was not created as a directory");
        }
    }

    #[test]
    fn different_kinds_resolve_to_different_directories() {
        let docs = app_dir("Rosace File Test", AppDir::Documents).unwrap();
        let cache = app_dir("Rosace File Test", AppDir::Cache).unwrap();
        let temp = app_dir("Rosace File Test", AppDir::Temp).unwrap();
        assert_ne!(docs, cache);
        assert_ne!(docs, temp);
        assert_ne!(cache, temp);
    }

    #[test]
    fn app_title_is_sanitized_into_a_safe_directory_name() {
        let dir = app_dir("My/Weird App: Name!", AppDir::Temp).unwrap();
        let name = dir.file_name().unwrap().to_str().unwrap();
        assert!(!name.contains('/'), "path separator must not survive sanitization: {name}");
        assert!(dir.is_dir());
    }

    #[test]
    fn resolving_the_same_kind_twice_is_idempotent() {
        let a = app_dir("Rosace File Test", AppDir::Documents).unwrap();
        let b = app_dir("Rosace File Test", AppDir::Documents).unwrap();
        assert_eq!(a, b);
    }
}
