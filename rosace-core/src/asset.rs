//! Cross-platform asset resolution (A6). One place that maps a logical asset
//! name (`"logo.png"`) to loadable bytes, so `ImageWidget::asset`,
//! `FontCache::from_asset`, and any future theme-from-asset loader all agree on
//! *where assets live* — and so hot-reload has a single cache to invalidate.
//!
//! The **API is identical on every platform**; only the *root* differs, and the
//! host sets it once at launch via [`set_root`]:
//!   - desktop dev / `rsc dev` / `rsc run`: the project's `assets/` dir
//!     (cwd-relative — the default, so nothing to set);
//!   - desktop release: `assets/` beside the executable (host may override);
//!   - iOS / Android: the app bundle's resources dir (FFI host sets it);
//!   - web: served under `/assets/` — the wasm loader fetches bytes (wired with
//!     the web asset step; the path API still resolves for URL building).
//!
//! `rsc.toml`'s `[assets] dirs = ["assets"]` declares what gets bundled; this
//! module is the runtime that reads them back.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// A compile-time asset **handle** — the typed, typo-proof way to refer to a
/// bundled asset. The `assets` module generated from your `assets/` dir (by
/// `rosace-asset-codegen` in `build.rs`) is full of `const Asset`s:
/// `assets::LOGO`, `assets::icons::HOME`. Passing `assets::LGO` won't compile,
/// and your editor autocompletes the real names.
///
/// It's a thin newtype over the logical name, so it costs nothing at runtime
/// and interops with the raw-string escape hatch through [`AssetRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Asset {
    name: &'static str,
}

impl Asset {
    /// Build a handle from a logical name. `const` so generated code can make
    /// these as `const` items.
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }

    /// The logical name this handle points at (e.g. `"icons/home.png"`).
    pub const fn name(&self) -> &str {
        self.name
    }
}

/// Anything usable as an asset reference: a typed [`Asset`] handle (the blessed,
/// checked form) **or** a raw `&str`/`String` (the escape hatch, for names only
/// known at runtime — e.g. a user-picked file). Every loader takes
/// `impl AssetRef`, so both forms work at the same call site:
///
/// ```ignore
/// Image::asset(assets::LOGO)   // typed, typo-proof
/// Image::asset("logo.png")     // dynamic escape hatch
/// ```
pub trait AssetRef {
    /// The logical asset name to resolve.
    fn asset_name(&self) -> &str;
}

impl AssetRef for Asset {
    fn asset_name(&self) -> &str { self.name }
}
impl AssetRef for &Asset {
    fn asset_name(&self) -> &str { self.name }
}
impl AssetRef for &str {
    fn asset_name(&self) -> &str { self }
}
impl AssetRef for String {
    fn asset_name(&self) -> &str { self.as_str() }
}
impl AssetRef for &String {
    fn asset_name(&self) -> &str { self.as_str() }
}

fn root_slot() -> &'static Mutex<Option<PathBuf>> {
    static ROOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    ROOT.get_or_init(|| Mutex::new(None))
}

/// Point asset resolution at a directory. Mobile FFI hosts call this at launch
/// with the app bundle's resources path; desktop release can point it beside
/// the executable. Desktop dev needs no call — the default (`assets/`) is right.
pub fn set_root(path: impl Into<PathBuf>) {
    *root_slot().lock().unwrap() = Some(path.into());
}

/// The directory assets resolve against. Resolution order:
/// 1. an explicit [`set_root`] override (mobile FFI hosts set the bundle path);
/// 2. `./assets` if it exists — the dev case (`rsc dev`/`rsc run` from the
///    project root);
/// 3. release bundle locations relative to the executable, so a Finder-launched
///    `.app` (whose cwd is `/`) or an installed binary still finds its assets:
///    - macOS `.app`: `<exe>/../Resources/assets`,
///    - Windows/Linux: `assets/` beside the executable;
/// 4. otherwise the cwd-relative `assets` default (nothing bundled yet).
///
/// The bundlers in `rsc package`/`rsc run` copy `assets/` into exactly these
/// locations, so the copy side and this resolve side stay in lockstep.
pub fn root() -> PathBuf {
    if let Some(p) = root_slot().lock().unwrap().clone() {
        return p;
    }

    let cwd_assets = PathBuf::from("assets");
    if cwd_assets.is_dir() {
        return cwd_assets;
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidates = [dir.join("../Resources/assets"), dir.join("assets")];
            for c in candidates {
                if c.is_dir() {
                    return c;
                }
            }
        }
    }

    cwd_assets
}

/// Resolve an asset (typed handle or raw name) to a filesystem path under the
/// asset root. `resolve(assets::icons::HOME)` → `<root>/icons/home.png`.
pub fn resolve(asset: impl AssetRef) -> PathBuf {
    root().join(asset.asset_name())
}

/// Read an asset's bytes, or `None` if it can't be found or read. This is the
/// single load primitive every typed loader (image, font, data) builds on, so
/// they all share one resolution + one hot-reload story.
pub fn bytes(asset: impl AssetRef) -> Option<Vec<u8>> {
    std::fs::read(resolve(asset)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_under_the_root_and_honours_an_override() {
        // Default root is cwd-relative `assets/`.
        assert_eq!(resolve("logo.png"), PathBuf::from("assets").join("logo.png"));

        // A host override (mobile bundle path) redirects resolution.
        set_root("/bundle/Resources");
        assert_eq!(resolve("logo.png"), PathBuf::from("/bundle/Resources/logo.png"));
        assert_eq!(resolve("f/x.ttf"), PathBuf::from("/bundle/Resources/f/x.ttf"));

        // Restore the default so other tests see cwd-relative resolution.
        *root_slot().lock().unwrap() = None;
    }
}
