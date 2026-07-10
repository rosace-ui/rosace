use std::process::Command;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct PackageOptions {
    /// App name (defaults to Cargo.toml [package].name)
    pub app_name: Option<String>,
    /// App version (defaults to Cargo.toml [package].version)
    pub app_version: Option<String>,
    /// Output directory (default: dist/)
    pub out_dir: String,
    /// macOS code-signing identity (e.g. `"Developer ID Application: Jane
    /// Doe (TEAMID)"`). Defaults to ad-hoc (`-`) when not given — enough to
    /// run locally, not enough to distribute past Gatekeeper.
    pub identity: Option<String>,
}

impl PackageOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }

        let mut app_name = None;
        let mut app_version = None;
        let mut out_dir = "dist".to_string();
        let mut identity = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--name" if i + 1 < args.len() => {
                    app_name = Some(args[i + 1].clone());
                    i += 2;
                }
                "--version" if i + 1 < args.len() => {
                    app_version = Some(args[i + 1].clone());
                    i += 2;
                }
                "--out" if i + 1 < args.len() => {
                    out_dir = args[i + 1].clone();
                    i += 2;
                }
                "--identity" if i + 1 < args.len() => {
                    identity = Some(args[i + 1].clone());
                    i += 2;
                }
                other if other.starts_with("--name=") => {
                    app_name = Some(other.trim_start_matches("--name=").to_string());
                    i += 1;
                }
                other if other.starts_with("--version=") => {
                    app_version = Some(other.trim_start_matches("--version=").to_string());
                    i += 1;
                }
                other if other.starts_with("--out=") => {
                    out_dir = other.trim_start_matches("--out=").to_string();
                    i += 1;
                }
                other if other.starts_with("--identity=") => {
                    identity = Some(other.trim_start_matches("--identity=").to_string());
                    i += 1;
                }
                other => return Err(format!("unknown flag: {}", other)),
            }
        }

        Ok(Self { app_name, app_version, out_dir, identity })
    }
}

pub fn print_help() {
    println!("rsc package — bundle the release build for distribution");
    println!();
    println!("USAGE:");
    println!("  rsc package [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("  --name <name>       App name (default: from Cargo.toml)");
    println!("  --version <ver>     App version (default: from Cargo.toml)");
    println!("  --out <dir>         Output directory (default: dist/)");
    println!("  --identity <name>   macOS codesign identity, e.g. \"Developer ID Application: ...\"");
    println!("                      (default: ad-hoc — runs locally, not notarized/distributable)");
    println!("  -h, --help          Print this message");
    println!();
    println!("Consumes macos/Info.plist + macos/icon.icns + macos/entitlements.plist,");
    println!("windows/icon.ico + windows/app.manifest, linux/icon.png + linux/app.desktop —");
    println!("whichever `rsc new` generated — rather than rebuilding them from scratch.");
}

pub fn run(opts: PackageOptions) -> Result<(), String> {
    // Determine app name + version + bundle id from rsc.toml/Cargo.toml if not specified
    let (name, version, bundle_id) = resolve_app_meta(&opts)?;

    println!("Packaging '{}' v{} ({}) for {}...", name, version, bundle_id, current_platform());
    println!();

    // Step 1: release build
    println!("  Building release binary...");
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .status()
        .map_err(|e| format!("failed to invoke cargo: {}", e))?;
    if !status.success() {
        return Err("cargo build --release failed".to_string());
    }

    // Step 2: platform-specific bundle
    fs::create_dir_all(&opts.out_dir)
        .map_err(|e| format!("cannot create {}: {}", opts.out_dir, e))?;

    #[cfg(target_os = "macos")]
    bundle_macos(&name, &version, &bundle_id, &opts.out_dir, opts.identity.as_deref())?;

    #[cfg(target_os = "linux")]
    bundle_linux(&name, &version, &opts.out_dir)?;

    #[cfg(target_os = "windows")]
    bundle_windows(&name, &version, &opts.out_dir)?;

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return Err("unsupported platform".to_string());

    println!();
    println!("  Package ready in '{}'", opts.out_dir);
    Ok(())
}

fn resolve_app_meta(opts: &PackageOptions) -> Result<(String, String, String), String> {
    let name = if let Some(n) = &opts.app_name {
        n.clone()
    } else {
        read_cargo_field("name")
            .unwrap_or_else(|| "app".to_string())
    };
    let version = if let Some(v) = &opts.app_version {
        v.clone()
    } else {
        read_cargo_field("version")
            .unwrap_or_else(|| "0.1.0".to_string())
    };
    // Read from rsc.toml (the single source of truth `rsc new`/`rsc
    // bundle-id` maintain) rather than inventing a separate id here — this
    // used to independently derive `com.rosace.<name>`, which silently
    // diverged from whatever `rsc new` had actually written everywhere else.
    let bundle_id = read_tzr_toml_field("bundle_id")
        .unwrap_or_else(|| format!("dev.rosace.{}", name.replace(['-', ' '], "_")));
    Ok((name, version, bundle_id))
}

fn read_cargo_field(field: &str) -> Option<String> {
    let content = fs::read_to_string("Cargo.toml").ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with(field) {
            let val = line.split_once('=')?.1
                .trim()
                .trim_matches('"')
                .to_string();
            return Some(val);
        }
    }
    None
}

fn read_tzr_toml_field(field: &str) -> Option<String> {
    let content = fs::read_to_string("rsc.toml").ok()?;
    for line in content.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == field {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

fn current_platform() -> &'static str {
    #[cfg(target_os = "macos")]   { "macOS" }
    #[cfg(target_os = "linux")]   { "Linux" }
    #[cfg(target_os = "windows")] { "Windows" }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    { "unknown" }
}

// ── macOS .app bundle ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn bundle_macos(
    name: &str,
    _version: &str,
    bundle_id: &str,
    out_dir: &str,
    identity: Option<&str>,
) -> Result<(), String> {
    let bin_name = name.to_lowercase().replace([' ', '-'], "_");
    let src_bin = format!("target/release/{}", bin_name);
    let sign_identity = identity.unwrap_or("-");
    assemble_macos_app(name, &bin_name, out_dir, Path::new(&src_bin), identity)?;
    println!("  Created {}/{}.app", out_dir, name);
    println!("  Bundle ID: {}", bundle_id);
    println!(
        "  Signed: {}",
        if identity.is_some() { sign_identity } else { "ad-hoc (local use only — pass --identity to distribute)" }
    );
    Ok(())
}

/// Assembles a real `.app` bundle around an already-built binary —
/// `Contents/MacOS/<crate_name>` (the executable — must match
/// `macos/Info.plist`'s `CFBundleExecutable`, which `rsc new` always
/// writes as the crate name, NOT the possibly-hyphenated display name),
/// `Contents/Info.plist`, `Contents/Resources/icon.icns`, ad-hoc (or
/// `identity`) code-signed.
///
/// Factored out of `bundle_macos` (D108-adjacent CLI fix, 2026-07-09) so
/// `rsc run --mac` can reuse the SAME real bundle assembly `rsc package`
/// already used, instead of `cargo run`-ing the bare binary directly —
/// which is why `rsc run --mac` showed a generic Dock icon instead of the
/// app's `macos/icon.icns`: a bare executable run as a plain Unix process
/// has no `.app` bundle for `NSBundle.mainBundle` to resolve, so AppKit
/// falls back to a generic icon regardless of what's in `macos/`. Returns
/// the assembled `.app` directory's path.
#[cfg(target_os = "macos")]
pub(crate) fn assemble_macos_app(
    name: &str,
    crate_name: &str,
    out_dir: &str,
    bin_src: &Path,
    identity: Option<&str>,
) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;

    let app_dir = format!("{}/{}.app", out_dir, name);
    let contents = format!("{}/Contents", app_dir);
    let macos_dir = format!("{}/MacOS", contents);
    let resources_dir = format!("{}/Resources", contents);

    // Clean previous bundle
    if Path::new(&app_dir).exists() {
        fs::remove_dir_all(&app_dir)
            .map_err(|e| format!("cannot remove old .app: {}", e))?;
    }

    fs::create_dir_all(&macos_dir)
        .map_err(|e| format!("cannot create MacOS dir: {}", e))?;
    fs::create_dir_all(&resources_dir)
        .map_err(|e| format!("cannot create Resources dir: {}", e))?;

    // Copy binary — filename must match Info.plist's CFBundleExecutable.
    let dst_bin = format!("{}/{}", macos_dir, crate_name);
    fs::copy(bin_src, &dst_bin)
        .map_err(|e| format!("cannot copy binary {} → {}: {}", bin_src.display(), dst_bin, e))?;

    // Make executable
    let mut perms = fs::metadata(&dst_bin)
        .map_err(|e| format!("metadata: {}", e))?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dst_bin, perms)
        .map_err(|e| format!("chmod: {}", e))?;

    // Copy Info.plist — generated ONCE by `rsc new` (see new.rs's
    // `macos_info_plist`) and left alone since; consuming it here (instead
    // of rebuilding it from scratch every package, like this function used
    // to) means a user's manual edits actually persist.
    let plist_src = Path::new("macos/Info.plist");
    if !plist_src.exists() {
        return Err(
            "macos/Info.plist not found — scaffold with `rsc new --platforms macos`, \
             or a project created before this file existed needs one written by hand"
                .to_string(),
        );
    }
    fs::copy(plist_src, format!("{}/Info.plist", contents))
        .map_err(|e| format!("cannot copy macos/Info.plist: {}", e))?;

    // Copy the icon — Info.plist's CFBundleIconFile is "icon" (no
    // extension), so macOS looks for exactly "icon.icns" in Resources/.
    let icon_src = Path::new("macos/icon.icns");
    if icon_src.exists() {
        fs::copy(icon_src, format!("{}/icon.icns", resources_dir))
            .map_err(|e| format!("cannot copy macos/icon.icns: {}", e))?;
    } else {
        println!("  Note: macos/icon.icns not found — bundle will use the default OS icon");
    }

    // Code-sign — even a local .app needs SOME signature to run on Apple
    // Silicon. Ad-hoc (`-`) by default; pass `--identity` for a real
    // Developer ID certificate + `macos/entitlements.plist` if present.
    let sign_identity = identity.unwrap_or("-");
    let mut codesign_args = vec!["--force", "--sign", sign_identity];
    let entitlements_path = "macos/entitlements.plist";
    if Path::new(entitlements_path).exists() {
        codesign_args.push("--entitlements");
        codesign_args.push(entitlements_path);
    }
    codesign_args.push(&app_dir);
    let status = Command::new("codesign")
        .args(&codesign_args)
        .status()
        .map_err(|e| format!("failed to invoke codesign: {}", e))?;
    if !status.success() {
        return Err("codesign failed".to_string());
    }

    Ok(app_dir)
}

// ── Linux .deb + binary ────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn bundle_linux(name: &str, version: &str, out_dir: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let bin_name = name.to_lowercase().replace([' ', '-'], "_");
    let src_bin = format!("target/release/{}", bin_name);

    // 1. Copy standalone binary
    let dst_bin = format!("{}/{}", out_dir, bin_name);
    fs::copy(&src_bin, &dst_bin)
        .map_err(|e| format!("cannot copy binary: {}", e))?;
    let mut perms = fs::metadata(&dst_bin).map_err(|e| format!("metadata: {}", e))?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dst_bin, perms).map_err(|e| format!("chmod: {}", e))?;
    println!("  Copied binary → {}", dst_bin);

    // 2. Build .deb structure
    let deb_root = format!("{}/{}_{}_{}", out_dir, bin_name, version, "amd64");
    let deb_bin_dir = format!("{}/usr/local/bin", deb_root);
    let deb_control_dir = format!("{}/DEBIAN", deb_root);

    fs::create_dir_all(&deb_bin_dir)
        .map_err(|e| format!("cannot create deb bin dir: {}", e))?;
    fs::create_dir_all(&deb_control_dir)
        .map_err(|e| format!("cannot create DEBIAN dir: {}", e))?;

    // Copy binary into deb tree
    let deb_dst = format!("{}/{}", deb_bin_dir, bin_name);
    fs::copy(&src_bin, &deb_dst)
        .map_err(|e| format!("cannot copy binary to deb tree: {}", e))?;
    let mut p = fs::metadata(&deb_dst).map_err(|e| format!("metadata: {}", e))?.permissions();
    p.set_mode(0o755);
    fs::set_permissions(&deb_dst, p).map_err(|e| format!("chmod: {}", e))?;

    // Write DEBIAN/control
    let control = format!(
        "Package: {}\nVersion: {}\nArchitecture: amd64\nMaintainer: ROSACE <noreply@rosace.dev>\nDescription: A ROSACE application\n Built with the ROSACE UI framework.\n",
        bin_name, version
    );
    fs::write(format!("{}/control", deb_control_dir), control)
        .map_err(|e| format!("cannot write control: {}", e))?;

    // Icon + freedesktop .desktop entry — generated ONCE by `rsc new` (see
    // new.rs's `linux_desktop_entry`); `{exec}`/`{icon}` placeholders get
    // filled in here since they depend on the install path, which `rsc
    // new` can't know in advance.
    let icon_src = Path::new("linux/icon.png");
    if icon_src.exists() {
        let icon_dir = format!("{}/usr/share/icons/hicolor/256x256/apps", deb_root);
        fs::create_dir_all(&icon_dir).map_err(|e| format!("cannot create icon dir: {}", e))?;
        fs::copy(icon_src, format!("{}/{}.png", icon_dir, bin_name))
            .map_err(|e| format!("cannot copy linux/icon.png: {}", e))?;
        // Also drop a loose copy next to the standalone binary for the
        // non-.deb case (someone just runs the raw executable).
        fs::copy(icon_src, format!("{}/{}.png", out_dir, bin_name)).ok();
    }
    let desktop_src = Path::new("linux/app.desktop");
    if desktop_src.exists() {
        let template = fs::read_to_string(desktop_src)
            .map_err(|e| format!("cannot read linux/app.desktop: {}", e))?;
        let filled = template
            .replace("{exec}", &format!("/usr/local/bin/{}", bin_name))
            .replace("{icon}", &bin_name);
        let apps_dir = format!("{}/usr/share/applications", deb_root);
        fs::create_dir_all(&apps_dir).map_err(|e| format!("cannot create applications dir: {}", e))?;
        fs::write(format!("{}/{}.desktop", apps_dir, bin_name), filled)
            .map_err(|e| format!("cannot write .desktop entry: {}", e))?;
    }

    // Try dpkg-deb if available
    let deb_output = format!("{}/{}_{}_amd64.deb", out_dir, bin_name, version);
    let dpkg_available = Command::new("dpkg-deb").arg("--version").output().is_ok();
    if dpkg_available {
        let status = Command::new("dpkg-deb")
            .args(["--build", &deb_root, &deb_output])
            .status()
            .map_err(|e| format!("dpkg-deb failed: {}", e))?;
        if status.success() {
            println!("  Created .deb → {}", deb_output);
            fs::remove_dir_all(&deb_root).ok();
        } else {
            println!("  Note: dpkg-deb failed; .deb tree left at {}", deb_root);
        }
    } else {
        println!("  Note: dpkg-deb not found; .deb tree at {} (run dpkg-deb --build manually)", deb_root);
    }

    Ok(())
}

// ── Windows .exe (copy + metadata stub) ───────────────────────────────────

#[cfg(target_os = "windows")]
fn bundle_windows(name: &str, version: &str, out_dir: &str) -> Result<(), String> {
    let bin_name = name.to_lowercase().replace([' ', '-'], "_");
    let src_bin = format!("target/release/{}.exe", bin_name);
    let dst_bin = format!("{}\\{}-{}.exe", out_dir, bin_name, version);

    fs::copy(&src_bin, &dst_bin)
        .map_err(|e| format!("cannot copy exe: {}", e))?;
    println!("  Copied → {}", dst_bin);

    // Icon, alongside the .exe — real icon-in-exe embedding needs `rc.exe`
    // resource compilation, deliberately not attempted here (see the
    // Windows note in .steering/CRATE_CONTRACTS.md's Known Issues: no
    // Windows toolchain existed on the machines this was developed on to
    // verify it). This is a plain loose file instead.
    let icon_src = Path::new("windows/icon.ico");
    if icon_src.exists() {
        fs::copy(icon_src, format!("{}\\{}-{}.ico", out_dir, bin_name, version))
            .map_err(|e| format!("cannot copy windows/icon.ico: {}", e))?;
    }

    // Side-by-side manifest (DPI awareness + execution level) — Windows
    // loads `<exe>.exe.manifest` automatically if present next to the
    // binary, no resource compiler needed. Also generated ONCE by `rsc
    // new` (see new.rs's `windows_app_manifest`).
    let manifest_src = Path::new("windows/app.manifest");
    if manifest_src.exists() {
        fs::copy(manifest_src, format!("{}\\{}-{}.exe.manifest", out_dir, bin_name, version))
            .map_err(|e| format!("cannot copy windows/app.manifest: {}", e))?;
    }

    println!("  Note: sign with signtool.exe before distribution");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_opts_defaults() {
        let opts = PackageOptions::from_args(&[]).unwrap();
        assert!(opts.app_name.is_none());
        assert!(opts.app_version.is_none());
        assert_eq!(opts.out_dir, "dist");
        assert!(opts.identity.is_none());
    }

    #[test]
    fn package_opts_identity_flag() {
        let args = vec!["--identity".to_string(), "Developer ID Application: Jane Doe (TEAMID)".to_string()];
        let opts = PackageOptions::from_args(&args).unwrap();
        assert_eq!(opts.identity.unwrap(), "Developer ID Application: Jane Doe (TEAMID)");
    }

    #[test]
    fn package_opts_identity_eq_syntax() {
        let args = vec!["--identity=Developer ID Application: X".to_string()];
        let opts = PackageOptions::from_args(&args).unwrap();
        assert_eq!(opts.identity.unwrap(), "Developer ID Application: X");
    }

    #[test]
    fn package_opts_name_flag() {
        let args = vec!["--name".to_string(), "MyApp".to_string()];
        let opts = PackageOptions::from_args(&args).unwrap();
        assert_eq!(opts.app_name.unwrap(), "MyApp");
    }

    #[test]
    fn package_opts_version_flag() {
        let args = vec!["--version".to_string(), "1.2.3".to_string()];
        let opts = PackageOptions::from_args(&args).unwrap();
        assert_eq!(opts.app_version.unwrap(), "1.2.3");
    }

    #[test]
    fn package_opts_out_flag() {
        let args = vec!["--out".to_string(), "release-out".to_string()];
        let opts = PackageOptions::from_args(&args).unwrap();
        assert_eq!(opts.out_dir, "release-out");
    }

    #[test]
    fn package_opts_equals_syntax() {
        let args = vec!["--name=TestApp".to_string(), "--version=2.0.0".to_string()];
        let opts = PackageOptions::from_args(&args).unwrap();
        assert_eq!(opts.app_name.unwrap(), "TestApp");
        assert_eq!(opts.app_version.unwrap(), "2.0.0");
    }

    #[test]
    fn package_opts_unknown_flag_errors() {
        let args = vec!["--foo".to_string()];
        assert!(PackageOptions::from_args(&args).is_err());
    }

    #[test]
    fn resolve_meta_uses_explicit_values() {
        let opts = PackageOptions {
            app_name: Some("Explicit".to_string()),
            app_version: Some("3.0.0".to_string()),
            out_dir: "dist".to_string(),
            identity: None,
        };
        let (name, ver, _bundle_id) = resolve_app_meta(&opts).unwrap();
        assert_eq!(name, "Explicit");
        assert_eq!(ver, "3.0.0");
    }

    #[test]
    fn resolve_meta_falls_back_to_dev_rosace_prefix_without_tzr_toml() {
        // No rsc.toml in the test's cwd (the workspace root, which has no
        // rsc.toml of its own) — should derive dev.rosace.<name>, not the
        // old hardcoded com.rosace.<name> this function used to invent
        // independently of rsc.toml.
        let _guard = crate::test_support::CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("tzr_pkg_meta_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();

        let opts = PackageOptions {
            app_name: Some("my-app".to_string()),
            app_version: Some("1.0.0".to_string()),
            out_dir: "dist".to_string(),
            identity: None,
        };
        let (_, _, bundle_id) = resolve_app_meta(&opts).unwrap();
        assert_eq!(bundle_id, "dev.rosace.my_app");

        std::env::set_current_dir(cwd).unwrap();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_meta_reads_bundle_id_from_tzr_toml_when_present() {
        let _guard = crate::test_support::CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("tzr_pkg_meta_toml_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();

        fs::write("rsc.toml", "name = \"myapp\"\nbundle_id = \"com.example.myapp\"\nplatforms = [\"macos\"]\n").unwrap();
        let opts = PackageOptions {
            app_name: Some("myapp".to_string()),
            app_version: Some("1.0.0".to_string()),
            out_dir: "dist".to_string(),
            identity: None,
        };
        let (_, _, bundle_id) = resolve_app_meta(&opts).unwrap();
        assert_eq!(bundle_id, "com.example.myapp");

        std::env::set_current_dir(cwd).unwrap();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn current_platform_is_non_empty() {
        assert!(!current_platform().is_empty());
    }
}
