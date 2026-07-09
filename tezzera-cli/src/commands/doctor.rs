//! `tzr doctor` — environment diagnostics, Flutter-`flutter doctor`-style.
//! Checks every platform's toolchain and prints an actionable fix for
//! anything missing, instead of letting `tzr new`/`tzr run`/`tzr package`
//! fail deep into a build with a confusing raw tool error. Read-only —
//! never installs or modifies anything itself.

use std::process::Command;

pub struct DoctorOptions;

impl DoctorOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }
        Ok(Self)
    }
}

pub fn print_help() {
    println!("tzr doctor — check this machine's toolchains for every TEZZERA target");
    println!();
    println!("USAGE:");
    println!("  tzr doctor");
    println!();
    println!("Read-only — reports what's installed and what's missing, with the exact");
    println!("command to fix it, for: the Rust toolchain, macOS/Xcode/iOS, Android");
    println!("(SDK/NDK/Gradle/adb), Windows/Linux cross-compilation, and web (wasm).");
}

struct Check {
    label: String,
    ok: bool,
    detail: String,
}

pub fn run() {
    let installed_targets = rustup_installed_targets();

    let checks = vec![
        check_rust_toolchain(),
        check_macos(),
        check_ios(),
        check_android(),
        check_windows(&installed_targets),
        check_linux(&installed_targets),
        check_web(&installed_targets),
    ];

    let mut any_missing = false;
    for group in &checks {
        for c in group {
            let mark = if c.ok { "\u{2713}" } else { "\u{2717}" };
            println!("[{}] {} — {}", mark, c.label, c.detail);
            if !c.ok { any_missing = true; }
        }
        println!();
    }

    if any_missing {
        println!("Some tools are missing — see the [\u{2717}] lines above for exact fixes.");
    } else {
        println!("Everything checked is installed. tzr new / tzr run should work for every target.");
    }
}

fn ok(label: &str, detail: impl Into<String>) -> Check {
    Check { label: label.to_string(), ok: true, detail: detail.into() }
}
fn missing(label: &str, detail: impl Into<String>) -> Check {
    Check { label: label.to_string(), ok: false, detail: detail.into() }
}

fn tool_version(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd).args(args).output().ok()?;
    if !output.status.success() { return None; }
    String::from_utf8(output.stdout).ok().map(|s| s.lines().next().unwrap_or("").trim().to_string())
}

fn rustup_installed_targets() -> String {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn check_rust_toolchain() -> Vec<Check> {
    let mut out = Vec::new();
    match tool_version("rustc", &["--version"]) {
        Some(v) => out.push(ok("Rust toolchain", v)),
        None => out.push(missing("Rust toolchain", "rustc not found. Install: https://rustup.rs")),
    }
    match tool_version("cargo", &["--version"]) {
        Some(v) => out.push(ok("Cargo", v)),
        None => out.push(missing("Cargo", "cargo not found (should ship with rustup)")),
    }
    out
}

fn check_macos() -> Vec<Check> {
    let mut out = Vec::new();
    let clt = Command::new("xcode-select").arg("-p").output().map(|o| o.status.success()).unwrap_or(false);
    if clt {
        out.push(ok("macOS — Xcode Command Line Tools", "installed"));
    } else {
        out.push(missing("macOS — Xcode Command Line Tools", "not installed. Run: xcode-select --install"));
    }
    let codesign = Command::new("xcrun").args(["-f", "codesign"]).output().map(|o| o.status.success()).unwrap_or(false);
    if codesign {
        out.push(ok("macOS — codesign", "found (needed for `tzr run --mac` / `tzr package`)"));
    } else {
        out.push(missing("macOS — codesign", "not found. Run: xcode-select --install"));
    }
    out
}

fn check_ios() -> Vec<Check> {
    let mut out = Vec::new();
    match tool_version("xcodebuild", &["-version"]) {
        Some(v) => out.push(ok("iOS — Xcode", v)),
        None => out.push(missing(
            "iOS — Xcode",
            "xcodebuild not found or not runnable. Install Xcode from the App Store, then: \
             sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer",
        )),
    }
    let sim_count = crate::commands::devices::list_devices()
        .iter()
        .filter(|d| d.platform == "ios")
        .count();
    if sim_count > 0 {
        out.push(ok("iOS — Simulators", format!("{} available (see `tzr devices`)", sim_count)));
    } else {
        out.push(missing("iOS — Simulators", "none found. Open Xcode > Settings > Platforms to install a simulator runtime"));
    }
    out
}

fn check_android() -> Vec<Check> {
    let mut out = Vec::new();

    let sdk_home = std::env::var("ANDROID_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("ANDROID_SDK_ROOT").ok().filter(|s| !s.is_empty()))
        .or_else(|| {
            let home = std::env::var("HOME").ok()?;
            let default = format!("{}/Library/Android/sdk", home);
            std::path::Path::new(&default).exists().then_some(default)
        });
    match &sdk_home {
        Some(path) => out.push(ok("Android — SDK", path.clone())),
        None => out.push(missing(
            "Android — SDK",
            "not found. Set ANDROID_HOME, or install via Android Studio (default: ~/Library/Android/sdk)",
        )),
    }

    // Filter to directories only — `~/Library/Android/sdk/ndk/` can contain
    // stray non-version entries (confirmed: macOS drops a `.DS_Store` FILE
    // in there just from Finder having opened it once) that aren't real
    // NDK installs.
    let ndk_versions: Vec<String> = sdk_home
        .as_ref()
        .and_then(|home| std::fs::read_dir(format!("{}/ndk", home)).ok())
        .map(|entries| {
            let mut v: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            v.sort();
            v
        })
        .unwrap_or_default();
    if !ndk_versions.is_empty() {
        out.push(ok("Android — NDK", ndk_versions.join(", ")));
    } else {
        out.push(missing(
            "Android — NDK",
            "not found. Install via Android Studio > SDK Manager > SDK Tools > NDK, or `sdkmanager --install \"ndk;28.2.13676358\"`",
        ));
    }

    let adb_ok = Command::new("adb").arg("version").output().map(|o| o.status.success()).unwrap_or(false);
    if adb_ok {
        out.push(ok("Android — adb", "found"));
    } else {
        out.push(missing("Android — adb", "not found. Install Android platform-tools, or `brew install android-platform-tools`"));
    }

    match tool_version("gradle", &["-v"]) {
        Some(_) => out.push(ok("Android — Gradle", "found on PATH (only needed to generate the gradlew wrapper at `tzr new` time)")),
        None => out.push(missing(
            "Android — Gradle",
            "not found on PATH. `tzr new --platforms android` won't generate a gradlew wrapper without it — \
             install via `brew install gradle`, or run `gradle wrapper` yourself inside android/ later",
        )),
    }

    let android_target_ok = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("aarch64-linux-android"))
        .unwrap_or(false);
    if android_target_ok {
        out.push(ok("Android — Rust target", "aarch64-linux-android installed"));
    } else {
        out.push(missing("Android — Rust target", "not installed. Run: rustup target add aarch64-linux-android"));
    }

    out
}

fn check_windows(installed_targets: &str) -> Vec<Check> {
    let mut out = Vec::new();
    if installed_targets.contains("x86_64-pc-windows-gnu") {
        out.push(ok("Windows — Rust target", "x86_64-pc-windows-gnu installed"));
    } else {
        out.push(missing("Windows — Rust target", "not installed. Run: rustup target add x86_64-pc-windows-gnu"));
    }
    let linker_ok = Command::new("x86_64-w64-mingw32-gcc").arg("--version").output().is_ok();
    if linker_ok {
        out.push(ok("Windows — cross-linker", "mingw-w64 found"));
    } else {
        out.push(missing("Windows — cross-linker", "mingw-w64 not found. Run: brew install mingw-w64"));
    }
    out.push(ok(
        "Windows — execution",
        "build-only from macOS (no Windows environment to run/verify on this host — see .steering/CRATE_CONTRACTS.md Known Issues)",
    ));
    out
}

fn check_linux(installed_targets: &str) -> Vec<Check> {
    let mut out = Vec::new();
    if installed_targets.contains("x86_64-unknown-linux-gnu") {
        out.push(ok("Linux — Rust target", "x86_64-unknown-linux-gnu installed"));
    } else {
        out.push(missing("Linux — Rust target", "not installed. Run: rustup target add x86_64-unknown-linux-gnu"));
    }
    out
}

fn check_web(installed_targets: &str) -> Vec<Check> {
    let mut out = Vec::new();
    if installed_targets.contains("wasm32-unknown-unknown") {
        out.push(ok("Web — Rust target", "wasm32-unknown-unknown installed"));
    } else {
        out.push(missing("Web — Rust target", "not installed. Run: rustup target add wasm32-unknown-unknown"));
    }
    let wasm_bindgen_ok = Command::new("wasm-bindgen").arg("--version").output().is_ok();
    if wasm_bindgen_ok {
        out.push(ok("Web — wasm-bindgen", "found"));
    } else {
        out.push(missing("Web — wasm-bindgen", "not found (optional — `tzr build --target web` falls back to a raw .wasm without it). Install: cargo install wasm-bindgen-cli"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_rust_toolchain_finds_a_real_rustc_on_this_dev_machine() {
        let checks = check_rust_toolchain();
        assert!(checks.iter().any(|c| c.label == "Rust toolchain" && c.ok));
    }

    #[test]
    fn tool_version_returns_none_for_a_nonexistent_binary() {
        assert_eq!(tool_version("definitely-not-a-real-binary-xyz", &["--version"]), None);
    }
}
