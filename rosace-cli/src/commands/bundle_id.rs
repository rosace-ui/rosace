//! `rsc bundle-id [<new-id>]` — read or update the app's bundle/package
//! identifier.
//!
//! `rsc new` writes the bundle id into `rsc.toml` (the single source of
//! truth) plus every platform file that embeds a copy of it at generation
//! time (`ios/Info.plist`, `ios/App.xcodeproj/project.pbxproj`,
//! `macos/Info.plist`). Editing `rsc.toml` by hand would silently desync
//! those copies, so this command updates all of them together instead of
//! requiring a manual find-and-replace across 3+ files.

use std::fs;
use std::path::Path;

use super::new::validate_bundle_id;

pub struct BundleIdOptions {
    /// `None` = print the current id; `Some` = set a new one.
    pub new_id: Option<String>,
}

impl BundleIdOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }
        let new_id = match args.first() {
            Some(id) if !id.starts_with("--") => {
                validate_bundle_id(id)?;
                Some(id.clone())
            }
            Some(other) => return Err(format!("unexpected flag '{}'. usage: rsc bundle-id [<new-id>]", other)),
            None => None,
        };
        Ok(Self { new_id })
    }
}

pub fn print_help() {
    println!("rsc bundle-id [<new-id>] — read or update the app's bundle identifier");
    println!();
    println!("USAGE:");
    println!("  rsc bundle-id              Print the current bundle id");
    println!("  rsc bundle-id <new-id>     Set a new bundle id everywhere it's embedded");
    println!();
    println!("Updates rsc.toml plus any of these that already exist:");
    println!("  ios/Info.plist");
    println!("  ios/App.xcodeproj/project.pbxproj");
    println!("  macos/Info.plist");
    println!();
    println!("EXAMPLES:");
    println!("  rsc bundle-id");
    println!("  rsc bundle-id com.example.myapp");
}

pub fn run(opts: BundleIdOptions) -> Result<(), String> {
    if !Path::new("rsc.toml").exists() {
        return Err("no rsc.toml here — run `rsc bundle-id` from an app directory".to_string());
    }
    let current = read_rsc_toml_bundle_id()?;

    let Some(new_id) = opts.new_id else {
        println!("{}", current);
        return Ok(());
    };

    let mut updated = Vec::new();

    if write_rsc_toml_bundle_id(&new_id)? {
        updated.push("rsc.toml");
    }
    if update_plist_bundle_id(Path::new("ios/Info.plist"), &new_id)? {
        updated.push("ios/Info.plist");
    }
    if update_pbxproj_bundle_id(Path::new("ios/App.xcodeproj/project.pbxproj"), &new_id)? {
        updated.push("ios/App.xcodeproj/project.pbxproj");
    }
    if update_plist_bundle_id(Path::new("macos/Info.plist"), &new_id)? {
        updated.push("macos/Info.plist");
    }

    // Android's manifest package name isn't re-stamped — Phase 24 Step 3
    // (real Android project generation) doesn't exist yet, so there's
    // nothing there to update.

    println!("Bundle id: {} → {}", current, new_id);
    if updated.is_empty() {
        println!("  (no generated platform files found to update — only rsc.toml would change, but see error above)");
    } else {
        for f in &updated {
            println!("  updated {}", f);
        }
    }
    Ok(())
}

fn read_rsc_toml_bundle_id() -> Result<String, String> {
    let s = fs::read_to_string("rsc.toml").map_err(|e| format!("cannot read rsc.toml: {}", e))?;
    for line in s.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "bundle_id" {
                return Ok(v.trim().trim_matches('"').to_string());
            }
        }
    }
    Err("rsc.toml has no bundle_id line".to_string())
}

fn write_rsc_toml_bundle_id(new_id: &str) -> Result<bool, String> {
    let content = fs::read_to_string("rsc.toml").map_err(|e| format!("cannot read rsc.toml: {}", e))?;
    let mut out = String::with_capacity(content.len());
    let mut found = false;
    for line in content.lines() {
        if let Some((k, _)) = line.split_once('=') {
            if k.trim() == "bundle_id" {
                out.push_str(&format!("bundle_id = \"{}\"", new_id));
                out.push('\n');
                found = true;
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if !found {
        return Err("rsc.toml has no bundle_id line to update".to_string());
    }
    fs::write("rsc.toml", out).map_err(|e| format!("cannot write rsc.toml: {}", e))?;
    Ok(true)
}

/// Re-stamps `<key>CFBundleIdentifier</key><string>...</string>` in a
/// `.plist` file, if it exists. Returns `false` (not an error) when the
/// file doesn't exist — not every project has every platform.
fn update_plist_bundle_id(path: &Path, new_id: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let (updated, count) = replace_between(
        &content,
        "<key>CFBundleIdentifier</key><string>",
        "</string>",
        new_id,
    );
    if count == 0 {
        return Err(format!("{}: no CFBundleIdentifier found to update", path.display()));
    }
    fs::write(path, updated).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
    Ok(true)
}

/// Re-stamps both `PRODUCT_BUNDLE_IDENTIFIER = "...";` occurrences (Debug +
/// Release configs) in a generated `project.pbxproj`, if it exists.
fn update_pbxproj_bundle_id(path: &Path, new_id: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let (updated, count) = replace_between(
        &content,
        "PRODUCT_BUNDLE_IDENTIFIER = \"",
        "\";",
        new_id,
    );
    if count == 0 {
        return Err(format!("{}: no PRODUCT_BUNDLE_IDENTIFIER found to update", path.display()));
    }
    fs::write(path, updated).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
    Ok(true)
}

/// Replaces the text between every `prefix`/`suffix` pair with `new_value`,
/// leaving everything else (including `prefix`/`suffix` themselves)
/// untouched. Returns the rewritten content and how many replacements were
/// made. A tiny hand-rolled scanner rather than a regex dependency — the
/// exact literal prefixes/suffixes above are unambiguous in both file
/// formats (OpenStep plist, pbxproj) this is used against.
fn replace_between(content: &str, prefix: &str, suffix: &str, new_value: &str) -> (String, usize) {
    let mut result = String::with_capacity(content.len());
    let mut count = 0;
    let mut rest = content;
    while let Some(start) = rest.find(prefix) {
        let after_prefix_idx = start + prefix.len();
        let after_prefix = &rest[after_prefix_idx..];
        match after_prefix.find(suffix) {
            Some(end) => {
                result.push_str(&rest[..after_prefix_idx]);
                result.push_str(new_value);
                result.push_str(suffix);
                rest = &after_prefix[end + suffix.len()..];
                count += 1;
            }
            None => break,
        }
    }
    result.push_str(rest);
    (result, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_between_single_occurrence() {
        let input = "<key>CFBundleIdentifier</key><string>dev.rosace.old</string>";
        let (out, count) = replace_between(
            input,
            "<key>CFBundleIdentifier</key><string>",
            "</string>",
            "com.example.new",
        );
        assert_eq!(count, 1);
        assert_eq!(out, "<key>CFBundleIdentifier</key><string>com.example.new</string>");
    }

    #[test]
    fn replace_between_multiple_occurrences() {
        let input = r#"PRODUCT_BUNDLE_IDENTIFIER = "dev.rosace.old";
some other line;
PRODUCT_BUNDLE_IDENTIFIER = "dev.rosace.old";"#;
        let (out, count) = replace_between(input, "PRODUCT_BUNDLE_IDENTIFIER = \"", "\";", "com.example.new");
        assert_eq!(count, 2);
        assert_eq!(out.matches("com.example.new").count(), 2);
        assert!(out.contains("some other line;"));
        assert!(!out.contains("dev.rosace.old"));
    }

    #[test]
    fn replace_between_no_match_returns_unchanged() {
        let input = "nothing to see here";
        let (out, count) = replace_between(input, "PREFIX", "SUFFIX", "new");
        assert_eq!(count, 0);
        assert_eq!(out, input);
    }

    #[test]
    fn write_rsc_toml_bundle_id_updates_only_that_line() {
        let _guard = crate::test_support::CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("rsc_bundleid_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();

        fs::write("rsc.toml", "name = \"myapp\"\nbundle_id = \"dev.rosace.myapp\"\nplatforms = [\"macos\"]\n").unwrap();
        assert_eq!(read_rsc_toml_bundle_id().unwrap(), "dev.rosace.myapp");
        write_rsc_toml_bundle_id("com.example.myapp").unwrap();
        assert_eq!(read_rsc_toml_bundle_id().unwrap(), "com.example.myapp");
        let content = fs::read_to_string("rsc.toml").unwrap();
        assert!(content.contains("name = \"myapp\""));
        assert!(content.contains("platforms = [\"macos\"]"));

        std::env::set_current_dir(cwd).unwrap();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_plist_bundle_id_returns_false_when_file_missing() {
        let dir = std::env::temp_dir().join(format!("rsc_bundleid_missing_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("nope.plist");
        assert!(!update_plist_bundle_id(&missing, "x").unwrap());
        fs::remove_dir_all(&dir).ok();
    }
}
