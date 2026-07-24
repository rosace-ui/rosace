//! Build-time asset codegen (A6, layer 2). Scans an app's `assets/` dir and
//! emits a typed `assets` module of `const Asset` handles, so
//! `Image::asset(assets::LOGO)` is typo-proof and autocompletes — with the
//! folder itself as the declaration (no hand-maintained manifest).
//!
//! Apps call this from `build.rs`:
//! ```ignore
//! fn main() {
//!     rosace_asset_codegen::generate("assets");
//! }
//! ```
//! and include the result once:
//! ```ignore
//! pub mod assets {
//!     include!(concat!(env!("OUT_DIR"), "/rosace_assets.rs"));
//! }
//! ```
//!
//! It re-runs only when the asset tree changes (`cargo:rerun-if-changed`), so it
//! costs nothing on ordinary code edits, and the generated code is a flat tree
//! of consts — dead simple, so it never slows the compiler down.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Scan `assets_dir` and write the typed `assets` module to
/// `$OUT_DIR/rosace_assets.rs`. Call from `build.rs`. Uses the default handle
/// type path `rosace::asset::Asset`; see [`generate_with`] to override it.
pub fn generate(assets_dir: impl AsRef<Path>) {
    generate_with(assets_dir, "rosace::asset::Asset");
}

/// Like [`generate`] but lets you name the `Asset` type path (for apps that use
/// `rosace_core` directly rather than the `rosace` umbrella).
pub fn generate_with(assets_dir: impl AsRef<Path>, asset_type_path: &str) {
    let dir = assets_dir.as_ref();
    println!("cargo:rerun-if-changed={}", dir.display());

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by cargo"))
        .join("rosace_assets.rs");

    let mut body = String::new();
    if dir.is_dir() {
        emit_dir(dir, dir, asset_type_path, 0, &mut body);
    }
    // An empty tree still produces a valid (empty) module.
    fs::write(&out, body).expect("write generated assets module");
}

/// Emit the handles + nested modules for one directory, recursing depth-first.
/// `root` is the asset root (for building logical names); `dir` is the current
/// directory; `depth` drives indentation.
fn emit_dir(root: &Path, dir: &Path, ty: &str, depth: usize, body: &mut String) {
    let indent = "    ".repeat(depth);

    // Deterministic order → stable generated output (clean diffs, no churn).
    let mut entries: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(rd) => rd.flatten().map(|e| e.path()).collect(),
        Err(_) => return,
    };
    entries.sort();

    let mut used_consts: HashSet<String> = HashSet::new();
    let mut used_mods: HashSet<String> = HashSet::new();

    for path in &entries {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        // Skip hidden housekeeping files (.gitkeep, .DS_Store, …).
        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            let mod_name = unique(&mod_ident(file_name), &mut used_mods);
            let _ = writeln!(body, "{indent}#[allow(non_snake_case)]");
            let _ = writeln!(body, "{indent}pub mod {mod_name} {{");
            emit_dir(root, path, ty, depth + 1, body);
            let _ = writeln!(body, "{indent}}}");
        } else {
            // Logical name = path relative to the asset root, forward slashes.
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let const_name = unique(&const_ident(file_name), &mut used_consts);
            let _ = writeln!(
                body,
                "{indent}/// `{rel}`\n{indent}pub const {const_name}: {ty} = {ty}::new(\"{rel}\");"
            );
        }
    }
}

/// Turn a filename into a SCREAMING_SNAKE const name, dropping the extension
/// (`home-icon.png` → `HOME_ICON`). Non-alphanumerics become `_`; a leading
/// digit is prefixed with `_` so it's a valid identifier.
fn const_ident(file_name: &str) -> String {
    let stem = file_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(file_name);
    sanitize(stem, true)
}

/// Turn a directory name into a valid module identifier (lowercased).
fn mod_ident(dir_name: &str) -> String {
    sanitize(dir_name, false)
}

fn sanitize(s: &str, upper: bool) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(if upper { ch.to_ascii_uppercase() } else { ch.to_ascii_lowercase() });
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    if out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        out.insert(0, '_');
    }
    out
}

/// Ensure a name is unique within its scope by suffixing `_2`, `_3`, … on
/// collision (e.g. `logo.png` and `logo.svg` both want `LOGO`).
fn unique(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}
