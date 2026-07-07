//! `tzr run [--target desktop|web|ios]` — build and run the current app on a
//! platform, hiding every manual step (wasm-bindgen + serve for web; bundle +
//! codesign + simctl for iOS). Reads `tzr.toml` for the app name / bundle id.

use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Desktop,
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
        let mut target = Target::Desktop;
        let mut port = 8080u16;
        let mut device = "iPhone 15 Pro".to_string();

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--target" | "-t" => {
                    i += 1;
                    target = parse_target(args.get(i).map(String::as_str))?;
                }
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
                    target = parse_target(Some(other.trim_start_matches("--target=")))?;
                }
                other if other.starts_with("--port=") => {
                    port = other.trim_start_matches("--port=").parse()
                        .map_err(|_| "invalid --port".to_string())?;
                }
                _ => {}
            }
            i += 1;
        }
        Ok(Self { target, port, device })
    }
}

fn parse_target(s: Option<&str>) -> Result<Target, String> {
    match s {
        Some("desktop") => Ok(Target::Desktop),
        Some("web") => Ok(Target::Web),
        Some("ios") => Ok(Target::Ios),
        Some(other) => Err(format!("unknown target '{}'. Use: desktop, web, ios", other)),
        None => Err("--target requires a value (desktop, web, ios)".to_string()),
    }
}

pub fn run(opts: RunOptions) -> Result<(), String> {
    let app = App::read()?;
    match opts.target {
        Target::Desktop => run_desktop(&app),
        Target::Web => run_web(&app, opts.port),
        Target::Ios => run_ios(&app, &opts.device),
    }
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

fn run_desktop(app: &App) -> Result<(), String> {
    println!("Running '{}' on desktop...", app.name);
    let status = Command::new("cargo")
        .args(["run", "--bin", &app.name])
        .status()
        .map_err(|e| format!("failed to invoke cargo: {}", e))?;
    if status.success() { Ok(()) } else { Err("app exited with an error".to_string()) }
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
