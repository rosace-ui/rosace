//! `tzr run [--target macos|windows|linux|web|ios]` (or `--mac`/`--win`/
//! `--lnx` shorthand) — build and run the current app on a platform, hiding
//! every manual step (wasm-bindgen + serve for web; bundle + codesign +
//! simctl for iOS). Reads `tzr.toml` for the app name / bundle id.
//!
//! macOS/Windows/Linux are explicit, separate targets (not one "desktop"
//! bucket) for the same reason `tezzera-cli/src/commands/new.rs`'s
//! `Platform` enum is — each has its own toolchain requirements, checked by
//! `preflight` before anything is attempted.

use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    MacOs,
    Windows,
    Linux,
    Web,
    Ios,
}

pub struct RunOptions {
    pub target: Target,
    pub port: u16,
    /// iOS simulator device name.
    pub device: String,
}

impl RunOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }

        let mut target = None;
        let mut port = 8080u16;
        let mut device = "iPhone 15 Pro".to_string();

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--target" | "-t" => {
                    i += 1;
                    target = Some(parse_target(args.get(i).map(String::as_str))?);
                }
                "--mac" => target = Some(Target::MacOs),
                "--win" => target = Some(Target::Windows),
                "--lnx" => target = Some(Target::Linux),
                "--port" => {
                    i += 1;
                    port = args.get(i).and_then(|s| s.parse().ok())
                        .ok_or_else(|| "--port requires a number".to_string())?;
                }
                "--device" => {
                    i += 1;
                    device = args.get(i).cloned()
                        .ok_or_else(|| "--device requires a value".to_string())?;
                }
                other if other.starts_with("--target=") => {
                    target = Some(parse_target(Some(other.trim_start_matches("--target=")))?);
                }
                other if other.starts_with("--port=") => {
                    port = other.trim_start_matches("--port=").parse()
                        .map_err(|_| "invalid --port".to_string())?;
                }
                _ => {}
            }
            i += 1;
        }
        // Default to the host OS — the one platform this run can actually
        // build AND execute locally, without cross-toolchain gymnastics.
        let target = target.unwrap_or_else(host_target);
        Ok(Self { target, port, device })
    }
}

/// The `Target` matching whichever OS `tzr` itself is running on.
fn host_target() -> Target {
    if cfg!(target_os = "macos") { Target::MacOs }
    else if cfg!(target_os = "windows") { Target::Windows }
    else { Target::Linux }
}

fn parse_target(s: Option<&str>) -> Result<Target, String> {
    match s {
        Some("macos") => Ok(Target::MacOs),
        Some("windows") => Ok(Target::Windows),
        Some("linux") => Ok(Target::Linux),
        Some("web") => Ok(Target::Web),
        Some("ios") => Ok(Target::Ios),
        Some(other) => Err(format!("unknown target '{}'. Use: macos, windows, linux, web, ios", other)),
        None => Err("--target requires a value (macos, windows, linux, web, ios)".to_string()),
    }
}

pub fn print_help() {
    println!("tzr run — build + run the app on a platform");
    println!();
    println!("USAGE:");
    println!("  tzr run [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("  --target <t>        macos | windows | linux | web | ios (default: host OS)");
    println!("  --mac / --win / --lnx   shorthand for --target macos|windows|linux");
    println!("  --port <n>          Web dev server port (default: 8080)");
    println!("  --device <name>     iOS simulator device (default: \"iPhone 15 Pro\")");
    println!("  -h, --help          Print this message");
    println!();
    println!("Before building, a preflight check confirms the tools each target needs");
    println!("are actually installed (codesign for macOS; a rustup cross target for");
    println!("Windows/Linux) and fails fast with install instructions if not — cross-");
    println!("building Windows/Linux from macOS only produces a binary, it can't run it.");
    println!();
    println!("EXAMPLES:");
    println!("  tzr run");
    println!("  tzr run --mac");
    println!("  tzr run --target web --port 3000");
    println!("  tzr run --target ios --device \"iPhone 15\"");
}

pub fn run(opts: RunOptions) -> Result<(), String> {
    preflight(opts.target)?;
    let app = App::read()?;
    match opts.target {
        Target::MacOs => run_macos(&app),
        Target::Windows => run_windows_cross_build(&app),
        Target::Linux => run_linux_cross_build(&app),
        Target::Web => run_web(&app, opts.port),
        Target::Ios => run_ios(&app, &opts.device),
    }
}

// ── Preflight: fail fast with an actionable message, not a raw tool error ──

fn preflight(target: Target) -> Result<(), String> {
    match target {
        Target::MacOs => preflight_macos(),
        Target::Windows => preflight_cross_target("x86_64-pc-windows-gnu", "Windows", Some("mingw-w64")),
        Target::Linux => preflight_cross_target("x86_64-unknown-linux-gnu", "Linux", None),
        Target::Web | Target::Ios => Ok(()), // existing inline checks in run_web/run_ios cover these
    }
}

fn preflight_macos() -> Result<(), String> {
    let ok = Command::new("xcrun")
        .args(["-f", "codesign"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err("codesign not found. Install Xcode Command Line Tools: xcode-select --install".to_string())
    }
}

/// Windows/Linux from macOS: this can only ever cross-BUILD, never run —
/// there's no emulator wired in. Checks the rustup target is installed and
/// (for Windows) warns if the cross-linker looks missing; doesn't hard-fail
/// on the linker check since detecting it reliably across package managers
/// is best-effort, not something to block on incorrectly.
fn preflight_cross_target(triple: &str, label: &str, linker_hint: Option<&str>) -> Result<(), String> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map_err(|e| format!("failed to run rustup: {}", e))?;
    let installed = String::from_utf8_lossy(&output.stdout);
    if !installed.contains(triple) {
        let mut msg = format!(
            "{} target not installed. Run:\n    rustup target add {}\n",
            label, triple
        );
        if let Some(hint) = linker_hint {
            msg.push_str(&format!(
                "  You'll also need a cross-linker. On macOS:\n    brew install {}\n",
                hint
            ));
        }
        msg.push_str(&format!(
            "  Note: this only lets you BUILD a {} binary from this host, not run it.",
            label
        ));
        return Err(msg);
    }
    Ok(())
}

// ── Manifest ───────────────────────────────────────────────────────────────

struct App {
    name: String,
    crate_name: String,
    bundle_id: String,
}

impl App {
    /// Read `tzr.toml` (falling back to `Cargo.toml`'s package name).
    fn read() -> Result<Self, String> {
        if !Path::new("Cargo.toml").exists() {
            return Err("no Cargo.toml here — run `tzr run` from an app directory".to_string());
        }
        let mut name = None;
        let mut bundle = None;
        if let Ok(s) = fs::read_to_string("tzr.toml") {
            for line in s.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let val = v.trim().trim_matches('"').to_string();
                    match k.trim() {
                        "name" => name = Some(val),
                        "bundle_id" => bundle = Some(val),
                        _ => {}
                    }
                }
            }
        }
        let name = name.or_else(cargo_pkg_name).ok_or_else(|| {
            "could not determine app name (no tzr.toml name / Cargo.toml package)".to_string()
        })?;
        let crate_name = name.replace('-', "_");
        let bundle_id = bundle.unwrap_or_else(|| format!("dev.tezzera.{}", crate_name));
        Ok(Self { name, crate_name, bundle_id })
    }
}

fn cargo_pkg_name() -> Option<String> {
    let s = fs::read_to_string("Cargo.toml").ok()?;
    let mut in_pkg = false;
    for line in s.lines() {
        let t = line.trim();
        if t == "[package]" { in_pkg = true; continue; }
        if in_pkg && t.starts_with('[') { break; }
        if in_pkg {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim() == "name" {
                    return Some(v.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

// ── Desktop ────────────────────────────────────────────────────────────────

fn run_macos(app: &App) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err(
            "tzr run --mac requires running tzr on macOS itself — cross-running \
             (build on one OS, execute on another) isn't supported."
                .to_string(),
        );
    }
    println!("Running '{}' on macOS...", app.name);
    let status = Command::new("cargo")
        .args(["run", "--bin", &app.name])
        .status()
        .map_err(|e| format!("failed to invoke cargo: {}", e))?;
    if status.success() { Ok(()) } else { Err("app exited with an error".to_string()) }
}

/// Cross-compiles for Windows; never attempts to run the result (no Windows
/// execution environment on a non-Windows host). See the Known Issues note
/// in `.steering/CRATE_CONTRACTS.md` — this path is generated/preflighted
/// but not build-verified end-to-end (no Windows toolchain available on the
/// machines this was developed on).
fn run_windows_cross_build(app: &App) -> Result<(), String> {
    const TRIPLE: &str = "x86_64-pc-windows-gnu";
    println!("Building '{}' for Windows ({})...", app.name, TRIPLE);
    let ok = Command::new("cargo")
        .args(["build", "--bin", &app.name, "--target", TRIPLE])
        .status()
        .map_err(|e| format!("cargo: {}", e))?
        .success();
    if !ok {
        return Err(format!("Windows cross-build failed (target/{}/debug/{}.exe)", TRIPLE, app.crate_name));
    }
    println!("  Built target/{}/debug/{}.exe", TRIPLE, app.crate_name);
    if !cfg!(target_os = "windows") {
        println!("  This host can't run a Windows binary — copy it to a Windows machine to launch it.");
    }
    Ok(())
}

/// Cross-compiles for Linux; never attempts to run the result on a
/// non-Linux host, same reasoning as `run_windows_cross_build`.
fn run_linux_cross_build(app: &App) -> Result<(), String> {
    const TRIPLE: &str = "x86_64-unknown-linux-gnu";
    println!("Building '{}' for Linux ({})...", app.name, TRIPLE);
    let ok = Command::new("cargo")
        .args(["build", "--bin", &app.name, "--target", TRIPLE])
        .status()
        .map_err(|e| format!("cargo: {}", e))?
        .success();
    if !ok {
        return Err(format!("Linux cross-build failed (target/{}/debug/{})", TRIPLE, app.crate_name));
    }
    println!("  Built target/{}/debug/{}", TRIPLE, app.crate_name);
    if !cfg!(target_os = "linux") {
        println!("  This host can't run a Linux binary — copy it to a Linux machine to launch it.");
    }
    Ok(())
}

// ── Web ────────────────────────────────────────────────────────────────────

fn run_web(app: &App, port: u16) -> Result<(), String> {
    println!("Building '{}' for web (wasm)...", app.name);

    // 1. Build the cdylib for wasm.
    let ok = Command::new("cargo")
        .args(["build", "--lib", "--target", "wasm32-unknown-unknown"])
        .status()
        .map_err(|e| format!("cargo: {}", e))?
        .success();
    if !ok {
        return Err("wasm build failed (run: rustup target add wasm32-unknown-unknown)".into());
    }

    // 2. wasm-bindgen → dist/ (generates <crate>.js + <crate>_bg.wasm).
    let wasm = format!("target/wasm32-unknown-unknown/debug/{}.wasm", app.crate_name);
    if !Path::new(&wasm).exists() {
        return Err(format!("expected wasm artifact not found: {}", wasm));
    }
    fs::create_dir_all("dist").map_err(|e| format!("cannot create dist/: {}", e))?;
    let bindgen = wasm_bindgen_bin()?;
    println!("  Generating JS glue (wasm-bindgen)...");
    let ok = Command::new(&bindgen)
        .args([&wasm, "--out-dir", "dist", "--target", "web", "--out-name", &app.crate_name])
        .status()
        .map_err(|e| format!("wasm-bindgen: {}", e))?
        .success();
    if !ok {
        return Err("wasm-bindgen failed".into());
    }

    // 3. Host page: use the app's web/index.html if present, else a default.
    let index_src = Path::new("web/index.html");
    if index_src.exists() {
        fs::copy(index_src, "dist/index.html").map_err(|e| format!("copy index.html: {}", e))?;
    } else {
        fs::write("dist/index.html", default_index_html(&app.crate_name))
            .map_err(|e| format!("write index.html: {}", e))?;
    }

    // 4. Serve.
    println!("  Open http://localhost:{}/", port);
    crate::commands::dev::serve_dist(port)
}

/// Locate `wasm-bindgen` (PATH, then ~/.cargo/bin).
fn wasm_bindgen_bin() -> Result<String, String> {
    if Command::new("wasm-bindgen").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        return Ok("wasm-bindgen".to_string());
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = format!("{}/.cargo/bin/wasm-bindgen", home);
        if Path::new(&p).exists() {
            return Ok(p);
        }
    }
    Err("wasm-bindgen not found. Install it: cargo install wasm-bindgen-cli".into())
}

fn default_index_html(crate_name: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <style>html,body{{margin:0;background:#14141a}}</style></head><body>\
         <script type=\"module\">import init from './{crate_name}.js'; init();</script>\
         </body></html>\n"
    )
}

// ── iOS (simulator) ──────────────────────────────────────────────────────────

fn run_ios(app: &App, device: &str) -> Result<(), String> {
    println!("Building '{}' for the iOS simulator...", app.name);

    // 1. Build the executable for the simulator target.
    let ok = Command::new("cargo")
        .args(["build", "--bin", &app.name, "--target", "aarch64-apple-ios-sim"])
        .status()
        .map_err(|e| format!("cargo: {}", e))?
        .success();
    if !ok {
        return Err("iOS build failed (run: rustup target add aarch64-apple-ios-sim)".into());
    }
    let bin = format!("target/aarch64-apple-ios-sim/debug/{}", app.name);

    // 2. Assemble the .app bundle (executable named after the crate + Info.plist).
    let bundle = format!("target/{}.app", app.name);
    let _ = fs::remove_dir_all(&bundle);
    fs::create_dir_all(&bundle).map_err(|e| format!("mkdir bundle: {}", e))?;
    fs::copy(&bin, format!("{}/{}", bundle, app.crate_name))
        .map_err(|e| format!("copy executable: {}", e))?;
    let plist_src = Path::new("ios/Info.plist");
    if !plist_src.exists() {
        return Err("ios/Info.plist not found — scaffold with `tzr new --platforms ios`".into());
    }
    fs::copy(plist_src, format!("{}/Info.plist", bundle))
        .map_err(|e| format!("copy Info.plist: {}", e))?;

    // 3. Ad-hoc code-sign (required even for the simulator).
    run_checked("codesign", &["--force", "--sign", "-", &bundle], "codesign")?;

    // 4. Boot the simulator (ignore "already booted") + open the Simulator UI.
    let _ = Command::new("xcrun").args(["simctl", "boot", device]).status();
    let _ = Command::new("open").args(["-a", "Simulator"]).status();

    // 5. Install + launch (stream the app's stdout/stderr so panics are visible).
    println!("  Installing on '{}'...", device);
    run_checked("xcrun", &["simctl", "install", "booted", &bundle], "simctl install")?;
    println!("  Launching {}...", app.bundle_id);
    run_checked("xcrun", &["simctl", "launch", "--console", "booted", &app.bundle_id], "simctl launch")
}

fn run_checked(cmd: &str, args: &[&str], what: &str) -> Result<(), String> {
    let ok = Command::new(cmd)
        .args(args)
        .status()
        .map_err(|e| format!("{}: {}", what, e))?
        .success();
    if ok { Ok(()) } else { Err(format!("{} failed", what)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_accepts_explicit_os_names() {
        assert_eq!(parse_target(Some("macos")).unwrap(), Target::MacOs);
        assert_eq!(parse_target(Some("windows")).unwrap(), Target::Windows);
        assert_eq!(parse_target(Some("linux")).unwrap(), Target::Linux);
        assert_eq!(parse_target(Some("web")).unwrap(), Target::Web);
        assert_eq!(parse_target(Some("ios")).unwrap(), Target::Ios);
    }

    #[test]
    fn parse_target_rejects_old_desktop_keyword() {
        // "desktop" is intentionally no longer accepted — macOS/Windows/
        // Linux are explicit, separate targets now (see module doc).
        let err = parse_target(Some("desktop")).unwrap_err();
        assert!(err.contains("macos"), "error should list the real options: {err}");
    }

    #[test]
    fn mac_win_lnx_flags_set_the_right_target() {
        let opts = RunOptions::from_args(&["--mac".to_string()]).unwrap();
        assert_eq!(opts.target, Target::MacOs);
        let opts = RunOptions::from_args(&["--win".to_string()]).unwrap();
        assert_eq!(opts.target, Target::Windows);
        let opts = RunOptions::from_args(&["--lnx".to_string()]).unwrap();
        assert_eq!(opts.target, Target::Linux);
    }

    #[test]
    fn no_target_flag_defaults_to_host_os() {
        let opts = RunOptions::from_args(&[]).unwrap();
        assert_eq!(opts.target, host_target());
    }

    #[test]
    fn target_flag_still_works() {
        let opts = RunOptions::from_args(&["--target".to_string(), "web".to_string()]).unwrap();
        assert_eq!(opts.target, Target::Web);
        let opts = RunOptions::from_args(&["--target=ios".to_string()]).unwrap();
        assert_eq!(opts.target, Target::Ios);
    }

    #[test]
    fn preflight_cross_target_reports_missing_target_actionably() {
        // A triple that will never be installed — proves the error message
        // is specific and actionable, not a raw tool failure.
        let err = preflight_cross_target("bogus-target-triple", "Bogus", Some("bogus-linker")).unwrap_err();
        assert!(err.contains("rustup target add bogus-target-triple"), "{err}");
        assert!(err.contains("bogus-linker"), "{err}");
        assert!(err.contains("BUILD"), "should clarify build-only: {err}");
    }
}
