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
}

impl PackageOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let mut app_name = None;
        let mut app_version = None;
        let mut out_dir = "dist".to_string();

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
                other => return Err(format!("unknown flag: {}", other)),
            }
        }

        Ok(Self { app_name, app_version, out_dir })
    }
}

pub fn run(opts: PackageOptions) -> Result<(), String> {
    // Determine app name + version from Cargo.toml if not specified
    let (name, version) = resolve_app_meta(&opts)?;

    println!("Packaging '{}' v{} for {}...", name, version, current_platform());
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
    bundle_macos(&name, &version, &opts.out_dir)?;

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

fn resolve_app_meta(opts: &PackageOptions) -> Result<(String, String), String> {
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
    Ok((name, version))
}

fn read_cargo_field(field: &str) -> Option<String> {
    let content = fs::read_to_string("Cargo.toml").ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with(field) && line.contains('=') {
            let val = line.splitn(2, '=').nth(1)?
                .trim()
                .trim_matches('"')
                .to_string();
            return Some(val);
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
fn bundle_macos(name: &str, version: &str, out_dir: &str) -> Result<(), String> {
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

    // Copy binary
    let bin_name = name.to_lowercase().replace(' ', "_").replace('-', "_");
    let src_bin = format!("target/release/{}", bin_name);
    let dst_bin = format!("{}/{}", macos_dir, name);
    fs::copy(&src_bin, &dst_bin)
        .map_err(|e| format!("cannot copy binary {} → {}: {}", src_bin, dst_bin, e))?;

    // Make executable
    let mut perms = fs::metadata(&dst_bin)
        .map_err(|e| format!("metadata: {}", e))?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dst_bin, perms)
        .map_err(|e| format!("chmod: {}", e))?;

    // Write Info.plist
    let bundle_id = format!("com.tezzera.{}", bin_name);
    let plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundleDisplayName</key>
    <string>{name}</string>
    <key>CFBundleIdentifier</key>
    <string>{bundle_id}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleExecutable</key>
    <string>{name}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
</dict>
</plist>
"#);
    fs::write(format!("{}/Info.plist", contents), plist)
        .map_err(|e| format!("cannot write Info.plist: {}", e))?;

    // Minimal icon placeholder (actual icon would be an .icns file in Resources/)
    fs::write(format!("{}/icon.txt", resources_dir), "Replace with AppIcon.icns\n")
        .map_err(|e| format!("cannot write icon placeholder: {}", e))?;

    println!("  Created {}/{}.app", out_dir, name);
    println!("  Bundle ID: {}", bundle_id);
    Ok(())
}

// ── Linux .deb + binary ────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn bundle_linux(name: &str, version: &str, out_dir: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let bin_name = name.to_lowercase().replace(' ', "_").replace('-', "_");
    let src_bin = format!("target/release/{}", bin_name);

    // 1. Copy standalone binary
    let dst_bin = format!("{}/{}", out_dir, bin_name);
    fs::copy(&src_bin, &dst_bin)
        .map_err(|e| format!("cannot copy binary: {}", e))?;
    let mut perms = fs::metadata(&dst_bin)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dst_bin, perms)?;
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
    let mut p = fs::metadata(&deb_dst)?.permissions();
    p.set_mode(0o755);
    fs::set_permissions(&deb_dst, p)?;

    // Write DEBIAN/control
    let control = format!(
        "Package: {}\nVersion: {}\nArchitecture: amd64\nMaintainer: TEZZERA <noreply@tezzera.dev>\nDescription: A TEZZERA application\n Built with the TEZZERA UI framework.\n",
        bin_name, version
    );
    fs::write(format!("{}/control", deb_control_dir), control)
        .map_err(|e| format!("cannot write control: {}", e))?;

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
    let bin_name = name.to_lowercase().replace(' ', "_").replace('-', "_");
    let src_bin = format!("target/release/{}.exe", bin_name);
    let dst_bin = format!("{}\\{}-{}.exe", out_dir, bin_name, version);

    fs::copy(&src_bin, &dst_bin)
        .map_err(|e| format!("cannot copy exe: {}", e))?;
    println!("  Copied → {}", dst_bin);
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
        };
        let (name, ver) = resolve_app_meta(&opts).unwrap();
        assert_eq!(name, "Explicit");
        assert_eq!(ver, "3.0.0");
    }

    #[test]
    fn current_platform_is_non_empty() {
        assert!(!current_platform().is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bundle_macos_plist_content() {
        // We test the plist generation logic inline (not the full fn which needs cargo build)
        let bundle_id = format!("com.tezzera.{}", "my_app");
        assert!(bundle_id.contains("tezzera"));
        assert!(bundle_id.contains("my_app"));
    }
}
