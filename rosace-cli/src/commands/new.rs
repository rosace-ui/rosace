//! `rsc new` — scaffold a new ROSACE app.
//!
//! Generates a well-structured, multi-file project (not everything in one
//! file): a root component with routing, a theme module, and a `screens/`
//! folder — plus the per-platform boilerplate (`web/index.html`,
//! `ios/Info.plist`, feature-gated `Cargo.toml`) for the platforms the user
//! selects. `rsc run --target <platform>` then builds/runs each without the
//! developer touching wasm-bindgen, simctl, or Info.plist by hand.

use std::fs;
use std::io::Write;
use std::path::Path;

/// A target platform the scaffolder can wire up.
///
/// No "Desktop" catch-all — macOS/Windows/Linux each need their own icon +
/// config file (`Info.plist`+entitlements, a manifest, a `.desktop` entry
/// respectively), so lumping them into one bucket would mean generating
/// files for OSes the user never asked for. Mirrors the flat style
/// `rosace_core::Platform` already uses for the same reason (D105).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
    Web,
    Ios,
    Android,
}

impl Platform {
    fn key(&self) -> &'static str {
        match self {
            Platform::MacOs => "macos",
            Platform::Windows => "windows",
            Platform::Linux => "linux",
            Platform::Web => "web",
            Platform::Ios => "ios",
            Platform::Android => "android",
        }
    }
    fn from_key(s: &str) -> Option<Self> {
        match s {
            "macos" => Some(Platform::MacOs),
            "windows" => Some(Platform::Windows),
            "linux" => Some(Platform::Linux),
            "web" => Some(Platform::Web),
            "ios" => Some(Platform::Ios),
            "android" => Some(Platform::Android),
            _ => None,
        }
    }
    /// The host OS `rsc` itself is running on, as a `Platform` — used to
    /// auto-include a sensible desktop default instead of forcing all three.
    fn host_os() -> Option<Self> {
        if cfg!(target_os = "macos") { Some(Platform::MacOs) }
        else if cfg!(target_os = "windows") { Some(Platform::Windows) }
        else if cfg!(target_os = "linux") { Some(Platform::Linux) }
        else { None }
    }
}

pub struct NewOptions {
    pub name: String,
    /// Selected platforms.
    pub platforms: Vec<Platform>,
    /// App bundle/package identifier (e.g. `dev.rosace.myapp`) — shared by
    /// iOS `CFBundleIdentifier`, the Xcode `PRODUCT_BUNDLE_IDENTIFIER`, and
    /// macOS `Info.plist`. Updatable later via `rsc bundle-id <id>`.
    pub bundle_id: String,
}

impl NewOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }

        let name = args
            .first()
            .ok_or_else(|| "usage: rsc new <name> [--platforms macos,windows,linux,web,ios,android] [--all] [--bundle-id <id>]".to_string())?
            .clone();
        if name.starts_with("--") {
            return Err("usage: rsc new <name> [--platforms ...]".to_string());
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(format!("invalid project name '{}': use letters, numbers, - or _", name));
        }
        let crate_name = name.replace('-', "_");

        // Parse flags. `--platforms a,b,c` or `--all` skip the interactive
        // platform prompt; `--bundle-id` skips the bundle-id prompt.
        let mut explicit_platforms: Option<Vec<Platform>> = None;
        let mut explicit_bundle_id: Option<String> = None;
        let mut i = 1;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--all" {
                explicit_platforms = Some(vec![
                    Platform::MacOs, Platform::Windows, Platform::Linux,
                    Platform::Web, Platform::Ios, Platform::Android,
                ]);
            } else if let Some(v) = arg.strip_prefix("--platforms=") {
                explicit_platforms = Some(parse_platforms(v)?);
            } else if arg == "--platforms" {
                i += 1;
                let v = args.get(i).ok_or_else(|| "--platforms requires a value".to_string())?;
                explicit_platforms = Some(parse_platforms(v)?);
            } else if let Some(v) = arg.strip_prefix("--bundle-id=") {
                validate_bundle_id(v)?;
                explicit_bundle_id = Some(v.to_string());
            } else if arg == "--bundle-id" {
                i += 1;
                let v = args.get(i).ok_or_else(|| "--bundle-id requires a value".to_string())?;
                validate_bundle_id(v)?;
                explicit_bundle_id = Some(v.clone());
            }
            i += 1;
        }

        let platforms = match explicit_platforms {
            Some(p) if !p.is_empty() => p,
            Some(_) => return Err("--platforms requires at least one platform".to_string()),
            None => prompt_platforms(),
        };

        let default_bundle_id = format!("dev.rosace.{}", crate_name);
        let bundle_id = match explicit_bundle_id {
            Some(b) => b,
            None => prompt_text("  Bundle/package identifier?", &default_bundle_id),
        };
        validate_bundle_id(&bundle_id)?;

        Ok(Self { name, platforms, bundle_id })
    }
}

fn parse_platforms(v: &str) -> Result<Vec<Platform>, String> {
    let mut out = Vec::new();
    for part in v.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let p = Platform::from_key(part).ok_or_else(|| {
            format!("unknown platform '{}'. Use: macos, windows, linux, web, ios, android", part)
        })?;
        if !out.contains(&p) {
            out.push(p);
        }
    }
    Ok(out)
}

/// Also used by `rsc bundle-id` to validate an id typed after project
/// creation, not just at `rsc new` time.
pub(crate) fn validate_bundle_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("bundle id cannot be empty".to_string());
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_') {
        return Err(format!(
            "invalid bundle id '{}': use letters, numbers, '.', '-', '_' (e.g. dev.rosace.myapp)",
            id
        ));
    }
    Ok(())
}

/// Interactive checkbox-style prompt. The host OS (whichever `rsc` itself is
/// running on) is auto-included without asking — a reasonable default, not
/// a forced one, since it's the one platform this run can actually build
/// and run locally. Every other platform is opt-in.
fn prompt_platforms() -> Vec<Platform> {
    let mut platforms = Vec::new();
    println!();
    if let Some(host) = Platform::host_os() {
        platforms.push(host);
        println!("  Detected host OS: {} (included automatically)", host.key());
    }
    println!("  Which other platforms should this app target?");
    println!();
    for (p, label) in [
        (Platform::MacOs, "macOS"),
        (Platform::Windows, "Windows"),
        (Platform::Linux, "Linux"),
        (Platform::Web, "Web (WebAssembly)"),
        (Platform::Ios, "iOS (simulator)"),
        (Platform::Android, "Android"),
    ] {
        if platforms.contains(&p) {
            continue; // already included as the host OS
        }
        if ask_yes_no(&format!("  Include {}?", label), false) {
            platforms.push(p);
        }
    }
    platforms
}

/// Prompt `question [y/N]` (or `[Y/n]` when `default` is true). Non-tty / EOF
/// falls back to the default so `rsc new x < /dev/null` still works.
fn ask_yes_no(question: &str, default: bool) -> bool {
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    print!("{} {} ", question, hint);
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => default, // EOF
        Ok(_) => match line.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => true,
            "n" | "no" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

/// Prompt for free text with a default shown in brackets. Non-tty / EOF /
/// an empty line all fall back to `default`, same convention as `ask_yes_no`.
fn prompt_text(question: &str, default: &str) -> String {
    print!("{} [{}] ", question, default);
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => default.to_string(), // EOF
        Ok(_) => {
            let trimmed = line.trim();
            if trimmed.is_empty() { default.to_string() } else { trimmed.to_string() }
        }
        Err(_) => default.to_string(),
    }
}

/// Prints `rsc new --help`'s focused usage.
pub fn print_help() {
    println!("rsc new <name> — scaffold a new ROSACE app");
    println!();
    println!("USAGE:");
    println!("  rsc new <name> [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("  --platforms <list>  Comma list: macos,windows,linux,web,ios,android");
    println!("                      (skips the interactive platform prompt)");
    println!("  --all               Every platform (skips the prompt)");
    println!("  --bundle-id <id>    App bundle/package id, e.g. dev.rosace.myapp");
    println!("                      (skips the bundle-id prompt; update later with `rsc bundle-id`)");
    println!("  -h, --help          Print this message");
    println!();
    println!("With no --platforms/--all, you're prompted interactively; the host OS");
    println!("(the one running `rsc`) is included automatically.");
    println!();
    println!("EXAMPLES:");
    println!("  rsc new myapp");
    println!("  rsc new myapp --platforms macos,ios --bundle-id com.example.myapp");
    println!("  rsc new myapp --all");
}

pub fn run(opts: NewOptions) -> Result<(), String> {
    let name = &opts.name;
    let crate_name = name.replace('-', "_");
    let bundle_id = opts.bundle_id.clone();
    let framework = framework_root();
    let dir = Path::new(name);

    if dir.exists() {
        return Err(format!("directory '{}' already exists", name));
    }

    let has = |p: Platform| opts.platforms.contains(&p);

    println!();
    println!(
        "Creating ROSACE app '{}' for: {}",
        name,
        opts.platforms.iter().map(|p| p.key()).collect::<Vec<_>>().join(", ")
    );

    // ── Directory tree ─────────────────────────────────────────────────────
    fs::create_dir_all(dir.join("src").join("screens"))
        .map_err(|e| format!("failed to create directories: {}", e))?;

    // ── Core project files ─────────────────────────────────────────────────
    write(dir.join("Cargo.toml"), &cargo_toml(name, &crate_name, &framework, &opts))?;
    write(dir.join("rsc.toml"), &rsc_toml(name, &bundle_id, &opts))?;
    write(
        dir.join(".gitignore"),
        "# Rust build\n/target\n/dist\n*.app\n\n# macOS\n.DS_Store\n\n# Mobile build outputs\n/android/**/build/\n/android/.gradle/\n/android/**/jniLibs/\n/ios/build/\n**/xcuserdata/\n",
    )?;
    write(dir.join("README.md"), &readme(name, &opts))?;
    // AI-context + CLI reference docs (D129) — bundled by default, not an
    // opt-in flag, so every scaffolded app gets them.
    write(dir.join("AGENTS.md"), &agents_md(name))?;
    write(dir.join("CLI.md"), &cli_md(name, &opts))?;
    // Assets dir (declared in rsc.toml [assets]); .gitkeep so the empty dir is
    // committable in the pushable sample.
    fs::create_dir_all(dir.join("assets")).map_err(|e| format!("cannot create assets dir: {e}"))?;
    write(dir.join("assets").join(".gitkeep"), "")?;

    // ── Structured source ──────────────────────────────────────────────────
    write(dir.join("build.rs"), BUILD_RS)?;
    write(dir.join("src").join("main.rs"), &main_rs(&crate_name))?;
    write(dir.join("src").join("lib.rs"), &lib_rs(name, &opts))?;
    write(dir.join("src").join("app.rs"), &app_rs(name))?;
    write(dir.join("src").join("theme.rs"), &theme_rs(&opts))?;
    write(dir.join("src").join("screens").join("mod.rs"), SCREENS_MOD_RS)?;
    write(dir.join("src").join("screens").join("home.rs"), HOME_RS)?;
    write(dir.join("src").join("screens").join("counter.rs"), COUNTER_RS)?;

    // ── Native-bridge FFI glue (D106 Phase 24) ─────────────────────────────
    // Shared by iOS and (eventually) Android — only the host project differs.
    if has(Platform::Ios) || has(Platform::Android) {
        write(dir.join("src").join("ffi.rs"), &ffi_rs(&bundle_id))?;
    }

    // ── Per-platform scaffolding ───────────────────────────────────────────
    if has(Platform::Web) {
        fs::create_dir_all(dir.join("web")).map_err(|e| e.to_string())?;
        write(dir.join("web").join("index.html"), &web_index_html(name, &crate_name))?;

        // Build-time semantic HTML/SEO export (D107 Phase 25 Step 3) — a
        // native (non-wasm) host binary `rsc build --target web` runs to
        // extract the app's semantic tree, never shipped to browsers.
        fs::create_dir_all(dir.join("examples")).map_err(|e| e.to_string())?;
        write(dir.join("examples").join("seo_extract.rs"), &seo_extract_rs(&crate_name))?;
    }
    if has(Platform::Ios) {
        // Physical Info.plist — for the older Phase 20-22 hand-rolled
        // `rsc run --target ios` harness only (kept working per the
        // Migration Rule). The real Xcode project below synthesizes its
        // own Info.plist from build settings; the two are independent.
        fs::create_dir_all(dir.join("ios")).map_err(|e| e.to_string())?;
        write(dir.join("ios").join("Info.plist"), &ios_info_plist(name, &crate_name, &bundle_id))?;

        // Real .xcodeproj + Swift host (D106 Phase 24 Step 2) — our own
        // AppDelegate/SceneDelegate, not winit's implicit one.
        let app_dir = dir.join("ios").join("App");
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
        write(app_dir.join("AppDelegate.swift"), IOS_APP_DELEGATE_SWIFT)?;
        write(app_dir.join("SceneDelegate.swift"), IOS_SCENE_DELEGATE_SWIFT)?;
        write(app_dir.join("EngineViewController.swift"), IOS_ENGINE_VIEW_CONTROLLER_SWIFT)?;

        let xcodeproj_dir = dir.join("ios").join("App.xcodeproj");
        fs::create_dir_all(xcodeproj_dir.join("xcshareddata").join("xcschemes"))
            .map_err(|e| e.to_string())?;
        write(xcodeproj_dir.join("project.pbxproj"), &ios_pbxproj(name, &crate_name, &bundle_id))?;
        write(
            xcodeproj_dir.join("xcshareddata").join("xcschemes").join("App.xcscheme"),
            &ios_xcscheme(),
        )?;
    }
    if has(Platform::Android) {
        // Real Gradle project (D106 Phase 24 Step 3) — our own MainActivity,
        // not winit's implicit one. icons::generate() (below) fills in
        // android/app/src/main/res/mipmap-*/.
        let android_dir = dir.join("android");
        let app_dir = android_dir.join("app");
        fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
        write(android_dir.join("settings.gradle.kts"), &android_settings_gradle(name))?;
        write(android_dir.join("build.gradle.kts"), &android_root_build_gradle())?;
        write(android_dir.join("gradle.properties"), &android_gradle_properties())?;
        write(app_dir.join("build.gradle.kts"), &android_app_build_gradle(&bundle_id, &crate_name))?;

        let main_dir = app_dir.join("src").join("main");
        fs::create_dir_all(&main_dir).map_err(|e| e.to_string())?;
        write(main_dir.join("AndroidManifest.xml"), &android_manifest_xml(name))?;

        let values_dir = main_dir.join("res").join("values");
        fs::create_dir_all(&values_dir).map_err(|e| e.to_string())?;
        write(values_dir.join("strings.xml"), &android_strings_xml(name))?;

        let package_path = android_package(&bundle_id).replace('.', "/");
        let java_dir = main_dir.join("java").join(&package_path);
        fs::create_dir_all(&java_dir).map_err(|e| e.to_string())?;
        write(
            java_dir.join("MainActivity.kt"),
            &android_main_activity_kt(&bundle_id, &crate_name),
        )?;

        // Real Gradle wrapper (not hand-authored — the wrapper jar is
        // binary) generated from this machine's system `gradle`, so a
        // clone of this project doesn't need Gradle preinstalled. Soft
        // failure: `gradle` may not be on PATH (see Known Issues); note it
        // rather than failing the whole scaffold over an optional step.
        let wrapper_ok = std::process::Command::new("gradle")
            .arg("wrapper")
            .current_dir(&android_dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !wrapper_ok {
            println!("  Note: `gradle wrapper` failed or `gradle` isn't installed —");
            println!("  android/gradlew won't exist until you run `gradle wrapper` yourself.");
        }
    }
    if has(Platform::MacOs) {
        fs::create_dir_all(dir.join("macos")).map_err(|e| e.to_string())?;
        write(dir.join("macos").join("Info.plist"), &macos_info_plist(name, &crate_name, &bundle_id))?;
        write(dir.join("macos").join("entitlements.plist"), &macos_entitlements_plist())?;
    }
    if has(Platform::Windows) {
        fs::create_dir_all(dir.join("windows")).map_err(|e| e.to_string())?;
        write(dir.join("windows").join("app.manifest"), &windows_app_manifest(name))?;
    }
    if has(Platform::Linux) {
        fs::create_dir_all(dir.join("linux")).map_err(|e| e.to_string())?;
        write(dir.join("linux").join("app.desktop"), &linux_desktop_entry(name))?;
    }

    // ── App icons ───────────────────────────────────────────────────────────
    crate::commands::icons::generate(dir, &opts.platforms)?;

    println!();
    println!("  \u{2713} Created '{}'", name);
    println!();
    println!("  Structure:");
    println!("    src/main.rs        native entry");
    println!("    src/lib.rs         launch() + web entry");
    println!("    src/app.rs         root component (routing + theme)");
    if has(Platform::Ios) || has(Platform::Android) {
        println!("    src/theme.rs       light/dark theme + per-platform Themes bundle");
    } else {
        println!("    src/theme.rs       light/dark theme");
    }
    if has(Platform::Ios) || has(Platform::Android) {
        println!("    src/ffi.rs         native-host FFI glue (D106)");
    }
    println!("    src/screens/       home + counter screens");
    println!("    AGENTS.md          framework context for AI assistants (and you)");
    println!("    CLI.md             `rsc` command reference for this project");
    if has(Platform::Web) { println!("    web/index.html     web host page"); }
    if has(Platform::Ios) {
        println!("    ios/App.xcodeproj/ real Xcode project — open, build, run");
        println!("    ios/App/           AppDelegate/SceneDelegate/EngineViewController.swift");
        println!("    ios/Info.plist     legacy plist (rsc run --target ios only, superseded by App.xcodeproj)");
    }
    if has(Platform::MacOs) { println!("    macos/             icon.icns, Info.plist, entitlements.plist"); }
    if has(Platform::Windows) { println!("    windows/           icon.ico, app.manifest"); }
    if has(Platform::Linux) { println!("    linux/             icon.png, app.desktop"); }
    if has(Platform::Ios) { println!("    ios/App/Assets.xcassets/  iOS app icon"); }
    if has(Platform::Android) { println!("    android/.../mipmap-*/    Android launcher icon"); }
    if has(Platform::Web) { println!("    web/favicon.ico, icon-*.png  web/PWA icons"); }
    println!("    rsc.toml           app manifest (name, bundle id — `rsc bundle-id` to change)");
    println!();
    println!("  Run it:");
    println!("    cd {}", name);
    if has(Platform::MacOs) { println!("    rsc run --mac           # macOS"); }
    if has(Platform::Windows) { println!("    rsc run --win           # Windows (build only — see Known Issues)"); }
    if has(Platform::Linux) { println!("    rsc run --lnx           # Linux (build only on this host)"); }
    if has(Platform::Web) { println!("    rsc run --target web    # browser"); }
    if has(Platform::Ios) {
        println!("    rsc run --target ios    # iOS simulator (drives real xcodebuild, D106 Step 4)");
        println!("    open ios/App.xcodeproj  # same project, opened directly in Xcode");
    }
    println!();
    Ok(())
}

/// The framework checkout this `rsc` was built from — used for path deps so
/// generated apps build against the local crates without a published release.
fn framework_root() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".into())
}

fn write(path: impl AsRef<Path>, content: &str) -> Result<(), String> {
    fs::write(&path, content).map_err(|e| format!("failed to write {}: {}", path.as_ref().display(), e))
}

// ── Templates ────────────────────────────────────────────────────────────────

/// A dependency line for a framework crate. When the framework checkout exists
/// on the machine running `rsc` (dev / dogfooding) → a `path` dep so the app
/// builds against local crates with no publish. Otherwise (an installed /
/// published `rsc`) → a crates.io version dep pinned to `rsc`'s own version, so
/// a distributed `rsc` produces buildable apps on any machine.
fn framework_dep(crate_name: &str, framework: &str) -> String {
    if Path::new(framework).join(crate_name).join("Cargo.toml").exists() {
        format!("{crate_name} = {{ path = \"{framework}/{crate_name}\" }}")
    } else {
        format!("{crate_name} = \"{}\"", env!("CARGO_PKG_VERSION"))
    }
}

fn cargo_toml(name: &str, crate_name: &str, framework: &str, opts: &NewOptions) -> String {
    // Empty [workspace] table detaches the app from any parent Cargo workspace
    // (so it builds even when generated inside the framework checkout).
    let web = if opts.platforms.contains(&Platform::Web) {
        "\n[target.'cfg(target_arch = \"wasm32\")'.dependencies]\nwasm-bindgen = \"0.2\"\n"
    } else {
        ""
    };
    // iOS/Android link the app as a staticlib/cdylib via the native-bridge
    // FFI boundary (D106 Phase 24) instead of running through winit — see
    // src/ffi.rs. crate-type carries every kind any selected platform needs
    // at once; unused ones are simply never built.
    let native_bridge = opts.platforms.contains(&Platform::Ios) || opts.platforms.contains(&Platform::Android);
    let crate_type = if native_bridge { r#"["cdylib", "staticlib", "rlib"]"# } else { r#"["cdylib", "rlib"]"# };
    let rosace_dep = framework_dep("rosace", framework);
    // Build-time asset codegen: `build.rs` scans `assets/` → a typed `assets`
    // module of `Asset` handles. A build-dependency so it never ships in the app.
    let asset_codegen_dep = framework_dep("rosace-asset-codegen", framework);
    // `ffi.rs`'s Platform Channel section (D127) uses `serde_json::json!`/
    // `Value` directly (not just via `rosace_ffi::` calls) to encode the
    // outgoing-call queue as JSON for the native host — needs its own
    // dependency, same reasoning as `android_jni_dep` below.
    let rosace_ffi_dep = if native_bridge {
        format!("{}\nserde_json = \"1\"\n", framework_dep("rosace-ffi", framework))
    } else {
        String::new()
    };
    // `ffi.rs`'s Android section uses `jni::JNIEnv`/`JObject`/`jint` etc.
    // directly (JNI function signatures cross this crate boundary, unlike
    // the plain-C iOS path) — needs its own `jni` dependency alongside
    // `rosace-ffi`'s internal one, target-gated the same way.
    let android_jni_dep = if opts.platforms.contains(&Platform::Android) {
        "\n[target.'cfg(target_os = \"android\")'.dependencies]\njni = \"0.21\"\n"
    } else {
        ""
    };
    // `examples/seo_extract.rs` (D107 Phase 25 Step 3) needs
    // `rosace-web-seo` — as a dev-dependency specifically, not a plain
    // one: dev-dependencies are excluded from `cargo build --bin
    // <app-name>` (the real, shipped app, on ANY platform this project
    // supports, web included — the wasm32 binary itself doesn't need this
    // crate for Step 3's purely build-time extraction), only pulled in for
    // `cargo run --example ...`. Same "never ships to a binary that
    // doesn't need it" reasoning as the wasm32-target-gating in
    // `rosace/Cargo.toml`, applied via a different Cargo mechanism suited
    // to a dev-machine-only *tool* rather than a platform-exclusive
    // runtime *dependency*.
    let web_seo_dev_dep = if opts.platforms.contains(&Platform::Web) {
        format!("\n[dev-dependencies]\n{}\n", framework_dep("rosace-web-seo", framework))
    } else {
        String::new()
    };
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[workspace]

[lib]
name = "{crate_name}"
crate-type = {crate_type}
path = "src/lib.rs"

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
{rosace_dep}
{rosace_ffi_dep}{web}{android_jni_dep}
[build-dependencies]
{asset_codegen_dep}
{web_seo_dev_dep}"#
    )
}

fn rsc_toml(name: &str, bundle_id: &str, opts: &NewOptions) -> String {
    let plats = opts
        .platforms
        .iter()
        .map(|p| format!("\"{}\"", p.key()))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"# ROSACE app manifest — read by `rsc run` / `rsc build`.
name = "{name}"
bundle_id = "{bundle_id}"
platforms = [{plats}]

# Bundled assets (images, fonts, data). Directories are scanned recursively;
# reference files in code via the generated typed handles, e.g.
# `Image::asset(assets::LOGO)`. Editing an asset hot-reloads under `rsc dev`.
[assets]
dirs = ["assets"]
"#
    )
}

fn readme(name: &str, opts: &NewOptions) -> String {
    let mut runs = String::from("- `rsc run` — desktop\n");
    if opts.platforms.contains(&Platform::Web) {
        runs.push_str("- `rsc run --target web` — browser (WebAssembly)\n");
    }
    if opts.platforms.contains(&Platform::Ios) {
        runs.push_str("- `rsc run --target ios` — iOS simulator\n");
    }
    format!("# {name}\n\nA ROSACE app.\n\n## Run\n\n{runs}")
}

/// AI-context capabilities doc, bundled into every scaffolded app so a human
/// OR an AI assistant building `{name}` has ROSACE's essentials up front
/// instead of burning tokens re-deriving them from the framework source.
/// Named `AGENTS.md` — the emerging cross-tool convention (several AI coding
/// assistants already look for this filename with zero extra config).
///
/// This used to be a deferred post-release idea (`.steering/POST_RELEASE_TODO.md`);
/// promoted to shipping-by-default (D129, 2026-08-04) — every scaffolded app
/// should get this, not just ones from some future CLI version. Content is a
/// hand-maintained template (same pattern as `readme()`/`theme_rs()` below),
/// version-stamped with `CARGO_PKG_VERSION` so a stale copy is obvious rather
/// than silently misleading. Keep this in sync with the real widget catalog
/// (`rosace-widgets/src/tree/`) and `.steering/WIDGET_QUALITY_BAR.md` each
/// time either changes meaningfully — that's the "update the doc" half of
/// the deal; the CLI half is that every `rsc new` after your edit picks it
/// up automatically.
fn agents_md(name: &str) -> String {
    format!(
        r#"# AGENTS.md — {name}

Context for AI assistants (and humans) working on this ROSACE app. Generated
by `rsc new` (rosace-cli v{version}) — regenerate by hand if the framework
version you depend on has moved on since.

## What ROSACE is

A declarative, reactive UI framework in pure Rust: one codebase targets
desktop (macOS/Windows/Linux), web (WASM), iOS, and Android. No garbage
collector, no virtual-DOM diff — state changes repaint only the components
that read the state that changed.

## The core pattern

```rust
use rosace::prelude::*;

struct Counter;

impl Component for Counter {{
    fn build(&self, ctx: &mut Context) -> Element {{
        // ctx.state gives a reactive Atom; reading it subscribes THIS
        // component, so set()/update() repaint exactly this widget.
        let count = ctx.state(0i32);

        Scaffold::new(
            Column::new()
                .child(Text::new(format!("Count: {{}}", count.get())))
                .child(Button::new("Increment").on_press({{
                    let count = count.clone();
                    move || count.update(|n| n + 1)
                }})),
        )
        .into_element()
    }}
}}
```

- `Component::build` returns a description of the UI, not the UI itself —
  the framework diffs and repaints only what a changed `Atom` touched.
- `ctx.state(default)` is per-instance state; `GlobalAtom` is app-wide.
- Widgets are **theme-defaulted** (they read colors/spacing from the active
  `ThemeData` unless overridden), **animated by default** where it matters
  (state transitions ease, not snap), and **interactive-by-identity** (a
  widget's identity — not its position in a list — is what focus/animation/
  hit-testing track across rebuilds; keep stable `key`s on list items).
- This project's own `src/app.rs`, `src/theme.rs`, and `src/screens/` are a
  working example of all of the above — read them before reading the docs.

## Theming

Material 3 ships out of the box as the single design system (`rosace::prelude::material()`,
one standardized design system), plus a compile-checked token system and runtime theme
switching. `src/theme.rs` in this project wires light/dark and (if this app
targets iOS/Android) a per-platform `Themes` bundle. A pluggable third-party
skin registry (swap a widget's whole visual form, not just its colors) is
planned but not yet built — for now, custom appearance means either theme
tokens or a fully custom `Widget` impl.

## Widget catalog

One dedicated builder per widget, composed inside `Component::build`. Not
exhaustive — see the [Guide](https://github.com/rosace-ui/rosace/wiki/Guide-Home)
for the full list and every constructor's options.

| Category | Widgets |
|---|---|
| Layout | `Column`, `Row`, `Stack`, `Grid`, `Wrap`, `Padding`, `Spacer`, `AspectRatio`, `Positioned`, `ScrollView`, `ListView` |
| Structure | `Scaffold`, `AppBar`, `BottomNavigationBar`, `NavRail`, `Drawer`, `Tabs`, `Card`, `Container` |
| Text & input | `Text`, `TextInput`, `TextArea`, `Button`, `Checkbox`, `Radio`, `Switch`, `Slider`, `Dropdown`, `Autocomplete`, `SearchBar`, `DatePicker`, `TimePicker`, `Stepper`, `RatingBar` |
| Feedback | `Dialog`, `Sheet`, `Snackbar`, `Toast`, `Tooltip`, `Skeleton`, `ProgressBar`, `CircularProgress` |
| Display | `Avatar`, `Badge`, `Chip`, `Divider`, `Icon`, `Image`, `ListTile`, `DataTable`, `Table` |
| Interaction | `Pressable`, `Dismissible`, `PullToRefresh`, `InteractiveViewer`, `Carousel`, `Hero`, `Menu`, `Fab`, `Segmented` |
| Custom drawing | `CustomPaint`, `ShaderPaint` (declarative GPU shader materials — gradients, glass, custom SDF effects) |

## Where to look next

- **[Guide](https://github.com/rosace-ui/rosace/wiki/Guide-Home)** — building
  with ROSACE: components, state, layout, theming, navigation, animation,
  hot reload.
- **[Architecture](https://github.com/rosace-ui/rosace/wiki/Architecture-Home)** —
  how it works inside: frame loop, reactive substrate, render pipeline,
  widget protocol, platform layer.
- **[Glossary](https://github.com/rosace-ui/rosace/wiki/Glossary)** — every
  ROSACE term, plus a from-scratch graphics/GPU primer.
- **`CLI.md`** (next to this file) — every `rsc` command this project uses.

## Being honest about limits

This is a `0.1.0` dev-preview framework: APIs can still move, some platforms
(iOS/Android) have real native hosts but ongoing UI polish, and not every
widget has a skinning hook yet. Don't assume feature parity with a mature
framework without checking — when in doubt, check the two docs above or the
framework source over guessing.
"#,
        version = env!("CARGO_PKG_VERSION"),
    )
}

/// `rsc` CLI reference bundled alongside `AGENTS.md`. Mirrors `print_usage()`
/// in `rosace-cli/src/main.rs` by hand — there's no single source of truth
/// shared between a human-facing terminal message and a markdown file
/// checked into someone else's repo, so keep the two in sync manually when
/// commands/flags change (search this crate for `print_usage` when you do).
/// `rsc help` always has the live, authoritative version for whatever CLI
/// the reader has installed; this file is a snapshot at scaffold time.
fn cli_md(name: &str, opts: &NewOptions) -> String {
    let mut run_lines = String::new();
    if opts.platforms.contains(&Platform::MacOs) {
        run_lines.push_str("rsc run --mac           # macOS\n");
    }
    if opts.platforms.contains(&Platform::Windows) {
        run_lines.push_str("rsc run --win           # Windows\n");
    }
    if opts.platforms.contains(&Platform::Linux) {
        run_lines.push_str("rsc run --lnx           # Linux\n");
    }
    if opts.platforms.contains(&Platform::Web) {
        run_lines.push_str("rsc run --target web    # browser (WebAssembly)\n");
    }
    if opts.platforms.contains(&Platform::Ios) {
        run_lines.push_str("rsc run --target ios    # iOS simulator\n");
    }
    if opts.platforms.contains(&Platform::Android) {
        run_lines.push_str("rsc run --target android # Android emulator/device\n");
    }
    format!(
        r#"# CLI.md — `rsc` reference for {name}

The commands most relevant to this project. Run `rsc help` any time for the
full, authoritative list from the CLI you have installed — this file is a
snapshot, not a replacement.

## Day to day

```
rsc dev                 # desktop dev loop with hot reload
rsc dev --target web    # web dev server (default port 3000)
{run_lines}```

## Building & shipping

```
rsc build --target desktop   # or web
rsc package                  # bundle for distribution (.app / .deb / .exe)
```

## Project maintenance

```
rsc bundle-id            # print this app's bundle/package id
rsc bundle-id <new-id>   # change it everywhere it's embedded
rsc doctor                # check this machine's toolchains, per target
rsc devices                # list run targets (id works with `run --device`)
```

## Quality checks

```
rsc check       # cargo check --workspace
rsc test        # cargo test --workspace (optional filter arg)
rsc lint        # cargo clippy --workspace -- -D warnings
rsc fmt         # cargo fmt --workspace --check
rsc analyze     # workspace health: crate count, member list
rsc snapshot    # run an example binary, save its PNG output
```

## Scaffolding another app

```
rsc new <name> --platforms macos,web --bundle-id com.example.app
```

See `AGENTS.md` (next to this file) for the framework itself — widgets,
patterns, theming, and docs links.
"#,
    )
}

fn main_rs(crate_name: &str) -> String {
    format!(
        "//! Native entry point. The app itself lives in the library so the web\n\
         //! build can share it.\n\nfn main() {{\n    {crate_name}::launch();\n}}\n"
    )
}

fn lib_rs(name: &str, opts: &NewOptions) -> String {
    // iOS and/or Android selected → wire a platform-keyed Themes bundle so
    // each looks native-appropriate out of the box (D105 Phase 23 Step 5).
    // Desktop/web-only apps keep the simpler single-theme path.
    let wants_platform_themes = opts.platforms.contains(&Platform::Ios) || opts.platforms.contains(&Platform::Android);
    let themes_call = if wants_platform_themes { ".themes(theme::themes())\n        " } else { "" };
    // Native-bridge FFI glue (D106 Phase 24) — only meaningful when iOS
    // and/or Android is selected; `mod ffi;` is gated the same as the
    // `rosace-ffi` dependency in Cargo.toml.
    let ffi_mod = if wants_platform_themes { "mod ffi;\n" } else { "" };
    // `examples/seo_extract.rs` (D107 Phase 25 Step 3) is a separate crate
    // root (Cargo examples compile against the library as an external
    // dependency) — it needs `app`/`theme` visible from outside this
    // crate. Only widened when Web is selected; other platforms keep them
    // private, matching the existing "only expose what's needed" pattern
    // `ffi_mod` above already follows.
    let app_theme_vis = if opts.platforms.contains(&Platform::Web) { "pub " } else { "" };
    format!(
        r#"//! {name} — a ROSACE app.
//!
//! `launch()` is shared by every platform. The native binary calls it from
//! `main`; the web build calls it from a `wasm-bindgen(start)` entry.

{app_theme_vis}mod app;
{ffi_mod}mod screens;
{app_theme_vis}mod theme;

/// Typed handles for everything under `assets/`, generated at build time by
/// `build.rs`. Refer to assets as `assets::LOGO` (typo-proof, autocompletes)
/// rather than raw strings. Add a file to `assets/` and it appears here.
pub mod assets {{
    include!(concat!(env!("OUT_DIR"), "/rosace_assets.rs"));
}}

use rosace::prelude::*;

/// One-time app startup — register Platform Channel method handlers here
/// (`rosace_ffi::set_method_call_handler`), or anything else that must run
/// exactly once before the engine starts.
///
/// Called from EVERY entry point below (`launch`, and — on iOS/Android —
/// `ffi.rs`'s `rsc_engine_init`/`nativeInit`), not just this one: mobile's
/// FFI entry points construct the engine directly and never call `launch`,
/// so code that only ran here would silently never execute on iOS/Android
/// (found live: a Platform Channel handler registered only in `launch`
/// answered every call with "no handler registered" on mobile until its
/// registration moved here instead).
pub(crate) fn app_init() {{}}

/// Start the app. Runs the winit event loop on native; hands off to the
/// browser's requestAnimationFrame loop on web.
pub fn launch() {{
    app_init();
    // Window size applies on desktop; mobile is always fullscreen.
    App::new()
        .title("{name}")
        .size(960, 640)
        {themes_call}.launch(app::AppRoot);
}}

/// Web (wasm) entry — invoked automatically when the module is instantiated.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {{
    launch();
}}
"#
    )
}

fn app_rs(name: &str) -> String {
    format!(
        r#"//! The root component: owns navigation, app-wide state, and the theme.

use rosace::prelude::*;
use rosace::theme::set_theme;

use crate::screens::{{counter_screen, home_screen}};

/// Every screen in the app. Add a variant + a match arm to add a route.
#[derive(Clone, Copy, PartialEq, Hash)]
pub enum Screen {{
    Home,
    Counter,
}}

impl Screen {{
    fn title(&self) -> &'static str {{
        match self {{
            Screen::Home => "{name}",
            Screen::Counter => "Counter",
        }}
    }}
}}

pub struct AppRoot;

impl Component for AppRoot {{
    fn build(&self, ctx: &mut Context) -> Element {{
        // Hooks — declared unconditionally, in a stable order.
        let nav = ScreenNav::new(ctx, Screen::Home);
        let count = ctx.state(0i32);
        // Starts `false` to match the launch theme (light — see `theme.rs`/
        // `ffi.rs`). If this disagreed with the actual initial theme, the first
        // toggle tap would set the theme it's already showing (a no-op), so it
        // would take two taps to flip the first time.
        let is_dark = ctx.state(false);

        // Same match arms build both the current and (if mid-transition)
        // previous screen, so ScreenTransitionView can animate between
        // them — see nav.push/pop's docs (default-on, theme-governed).
        let build_screen = {{
            let nav = nav.clone();
            let count = count.clone();
            move |s: Screen| -> BoxedWidget {{
                match s {{
                    Screen::Home => Box::new(home_screen(&nav)),
                    Screen::Counter => Box::new(counter_screen(&count)),
                }}
            }}
        }};
        let screen = nav.current().unwrap_or(Screen::Home);
        let body = build_screen(screen);
        let outgoing = nav.previous().map(build_screen);
        let view = ScreenTransitionView::new(
            body, nav.current_key(), outgoing, nav.previous_key(), nav.transition_handle(), nav.stack_keys(),
        );

        // App bar: a back button appears off Home; a theme toggle on the right.
        let mut bar = AppBar::new(screen.title()).back_button(&nav);
        let label = if is_dark.get() {{ "\u{{2600}} Light" }} else {{ "\u{{263e}} Dark" }};
        let d = is_dark.clone();
        bar = bar.action(Button::new(label).on_press(move || {{
            let next = !d.get();
            d.set(next);
            set_theme(if next {{ crate::theme::dark() }} else {{ crate::theme::light() }});
        }}));

        Scaffold::new(view).app_bar(bar).into_element()
    }}
}}
"#
    )
}

/// Generates `src/theme.rs`. Always emits `dark()`/`light()` (used by the
/// in-app theme toggle in `app.rs`); when iOS and/or Android are selected it
/// also emits `themes()`, a platform-keyed `Themes` bundle wiring Material
/// for iOS and Material for Android (D105 Phase 23 Step 5) so a generated
/// app looks native-appropriate on each target with no hand-editing.
fn theme_rs(opts: &NewOptions) -> String {
    let has_ios = opts.platforms.contains(&Platform::Ios);
    let has_android = opts.platforms.contains(&Platform::Android);

    let mut out = String::from(
        r#"//! App theme. Edit these to customize colors, or build a `ThemeData` from
//! scratch — the built-ins are just a convenient starting point.

use rosace::prelude::ThemeData;

/// The app's dark theme.
pub fn dark() -> ThemeData {
    rosace::prelude::dark_theme()
}

/// The app's light theme.
pub fn light() -> ThemeData {
    rosace::prelude::light_theme()
}
"#,
    );

    if has_ios || has_android {
        out.push_str(
            r#"
/// One design system, not per-platform chrome (D133, superseding D105's
/// Cupertino half): Android keeps Material's structural bar, everything
/// else uses the base theme. Third-party themes plug in through this same
/// `Themes` bundle. Passed to `App::themes(..)` in `lib.rs`.
pub fn themes() -> rosace::prelude::Themes {
    rosace::prelude::Themes::new(light())
"#,
        );
        if has_android {
            out.push_str(
                "        .platform(rosace::prelude::Platform::Android, rosace::prelude::material())\n",
            );
        }
        out.push_str("}\n");
    }

    out
}

const BUILD_RS: &str = r#"//! Build script: scans `assets/` and generates the typed `assets` module
//! (included from `src/lib.rs`). Re-runs only when the asset tree changes.

fn main() {
    rosace_asset_codegen::generate("assets");
}
"#;

const SCREENS_MOD_RS: &str = r#"//! One file per screen. Re-export each screen's builder here.

mod counter;
mod home;

pub use counter::counter_screen;
pub use home::home_screen;
"#;

const HOME_RS: &str = r#"//! The home screen — an index of the app's routes.

use rosace::prelude::*;

use crate::app::Screen;

pub fn home_screen(nav: &ScreenNav<Screen>) -> impl Widget {
    let nav = nav.clone();
    Column::new()
        .padding(EdgeInsets::all(16.0))
        .child(
            ListTile::new("Counter")
                .subtitle("A simple counter with + / \u{2212}")
                .on_press(move || {
                    nav.push(Screen::Counter);
                }),
        )
}
"#;

const COUNTER_RS: &str = r#"//! The counter screen. `count` is app-wide state owned by the root component,
//! so it survives navigating away and back.

use rosace::prelude::*;

pub fn counter_screen(count: &Atom<i32>) -> impl Widget {
    let c = count.clone();
    Column::new()
        .spacing(16.0)
        .padding(EdgeInsets::all(24.0))
        .child(Spacer::gap(0.0, 48.0))
        .child(Text::display(count.get().to_string()).align(TextAlign::Center))
        .child(Text::new("Tap to change the count").align(TextAlign::Center))
        .child(Spacer::gap(0.0, 24.0))
        .child(
            Row::new()
                .main_axis_alignment(MainAxisAlignment::Center)
                .spacing(12.0)
                .child(
                    Button::new("\u{2212}")
                        .variant(ButtonVariant::Ghost)
                        .width(44.0)
                        .on_press({
                            let c = c.clone();
                            move || c.set(c.get() - 1)
                        }),
                )
                .child(Button::new("Increment").width(140.0).on_press({
                    let c = c.clone();
                    move || c.set(c.get() + 1)
                }))
                .child(
                    Button::new("+")
                        .variant(ButtonVariant::Ghost)
                        .width(44.0)
                        .on_press({
                            let c = c.clone();
                            move || c.set(c.get() + 1)
                        }),
                ),
        )
}
"#;

/// Build-time semantic-tree extraction (D107 Phase 25 Step 3). A native
/// (host-arch, NOT wasm32) example binary `rsc build --target web` runs via
/// `cargo run --example seo_extract` — never compiled to wasm, never
/// shipped to a browser. Does one headless `FrameEngine` build+paint pass
/// (a `SkiaCanvas` is just an in-memory CPU pixmap — no real window/GPU
/// needed) purely to populate the render tree, reads `.semantics()`, and
/// prints the Declarative Shadow DOM HTML + a plain-text (`llms.txt`)
/// extraction to stdout, separated by a marker line `rsc build` splits on.
fn seo_extract_rs(crate_name: &str) -> String {
    format!(
        r#"//! Build-time semantic HTML/SEO export (D107 Phase 25 Step 3) — run by
//! `rsc build --target web` via `cargo run --example seo_extract`, NEVER
//! compiled to wasm or shipped to a browser. See `rosace-web-seo`'s
//! module doc for why this mapping lives in its own crate rather than
//! `rosace-core` (platform isolation — verified via `cargo tree`, not
//! assumed).
//!
//! A Cargo example is its own crate root — `crate::` here would NOT reach
//! this package's own `src/lib.rs` modules, so `app`/`theme` are addressed
//! by this crate's own library name instead (`{crate_name}::...`), same as
//! any other external dependent would reach them. `lib_rs`'s codegen
//! widens `app`/`theme` to `pub mod` specifically so this resolves.

use rosace::{{FontCache, FrameEngine, SkiaCanvas}};

use {crate_name}::app::AppRoot;

/// Matches `web_index_html`'s marker comment in `build_web`.
const SPLIT_MARKER: &str = "\n---RSC-SEO-TEXT---\n";

fn main() {{
    rosace::theme::set_theme({crate_name}::theme::light());

    let font = FontCache::system_ui()
        .or_else(FontCache::system_mono)
        .unwrap_or_else(FontCache::embedded);

    let mut engine = FrameEngine::new(Box::new(AppRoot), font);

    // A representative desktop-web viewport — the semantic tree (roles/
    // labels/structure) doesn't meaningfully depend on the exact size for
    // typical layouts, so this doesn't need to match any real device.
    let mut canvas = SkiaCanvas::new_hidpi(1280, 800, 1.0);
    let mut overlay = SkiaCanvas::new_hidpi(1280, 800, 1.0);
    engine.paint(&mut canvas, &mut overlay, &[]);

    let tree = engine.semantics();
    let html = rosace_platform::web_seo::render_shadow_dom_template(&tree);
    let text = rosace_platform::web_seo::render_text(&tree);

    print!("{{html}}{{SPLIT_MARKER}}{{text}}");
}}
"#
    )
}

fn web_index_html(name: &str, crate_name: &str) -> String {
    format!(
        r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <title>{name}</title>
  <link rel="icon" href="favicon.ico" sizes="any" />
  <link rel="apple-touch-icon" href="apple-touch-icon.png" />
  <link rel="manifest" href="site.webmanifest" />
  <style>
    html, body {{ margin: 0; padding: 0; background: #14141a; }}
    /* D107 Phase 25: visually hidden but still in the accessibility tree —
       "display: none"/"visibility: hidden" would also hide this from
       screen readers, which defeats the point (crawlers AND assistive
       tech read this; only SIGHTED users see the canvas instead). */
    #rsc-seo {{
      position: absolute;
      width: 1px; height: 1px;
      overflow: hidden;
      clip: rect(0, 0, 0, 0);
      white-space: nowrap;
    }}
  </style>
</head>
<body>
  <!-- D107 Phase 25 Step 3: `rsc build --target web` replaces this comment
       with a real <template shadowrootmode="open"> block — crawlable text/
       structure baked into the raw HTML response, present whether or not
       the crawler executes JS (a plain curl sees the literal bytes; a
       shadow-DOM-aware crawler sees a real shadow root). The canvas the
       script below creates paints over it for sighted users; nothing here
       is itself rendered as a second visual layer. -->
  <div id="rsc-seo"><!--RSC_SEO_SHADOW_DOM--></div>
  <script type="module">
    import init from './{crate_name}.js';
    init().catch((e) => {{
      console.error('rosace init failed:', e);
      document.body.innerHTML = '<pre style="color:#f66">' + e + '</pre>';
    }});
  </script>
</body>
</html>
"#
    )
}

fn ios_info_plist(name: &str, crate_name: &str, bundle_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>{crate_name}</string>
  <key>CFBundleIdentifier</key><string>{bundle_id}</string>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundleDisplayName</key><string>{name}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>CFBundleIconName</key><string>AppIcon</string>
  <key>LSRequiresIPhoneOS</key><true/>
  <key>UILaunchScreen</key><dict/>
  <key>UIRequiredDeviceCapabilities</key><array><string>arm64</string></array>
  <key>MinimumOSVersion</key><string>13.0</string>
</dict>
</plist>
"#
    )
}

// ── macOS / Windows / Linux ─────────────────────────────────────────────────
//
// Each desktop OS gets its own top-level folder (parallel to ios/android/
// web) with plain, editable files — generated ONCE here, consumed (not
// regenerated) by `rsc package`. See package.rs for the consuming side.

/// `macos/Info.plist` — the real bundle plist `rsc package`'s `bundle_macos`
/// copies into `<App>.app/Contents/Info.plist` (it used to build this
/// inline from scratch on every package, throwing away any edit the user
/// made — see package.rs's history). `CFBundleIconFile` points at the
/// `macos/icon.icns` `icons.rs` writes alongside this file.
fn macos_info_plist(name: &str, crate_name: &str, bundle_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>{crate_name}</string>
  <key>CFBundleIdentifier</key><string>{bundle_id}</string>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundleDisplayName</key><string>{name}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
</dict>
</plist>
"#
    )
}

/// `macos/entitlements.plist` — a starter file with no entitlements granted.
/// Real distribution (Mac App Store sandboxing, hardened-runtime + Developer
/// ID notarization) needs specific entitlements added here by hand; `rsc`
/// can't know what a given app needs (network client? file access? camera?).
fn macos_entitlements_plist() -> String {
    // NOTE for whoever edits this template: keep the XML comment free of
    // literal angle-bracket tag examples AND "--" sequences. Apple's
    // entitlements parser (AMFIUnserializeXML, used by `codesign
    // --entitlements`) is stricter than general XML/plist readers and
    // fails on comments containing embedded "<...>"-looking text —
    // confirmed by hand: a comment quoting real entitlement keys in angle
    // brackets produced "Failed to parse entitlements: AMFIUnserializeXML:
    // syntax error", even though that's valid per the XML spec (and "--"
    // inside a comment is technically invalid XML regardless of parser).
    // Plain prose, no "<tag>" shapes, no literal "--", is the safe subset.
    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<!--
  Starts empty. Add entitlements here as your app needs them, e.g. for a
  sandboxed, Mac App Store, or notarized build: com.apple.security.app-sandbox
  and com.apple.security.network.client are common starting points. See
  Apple's entitlements reference for the full key list.
  rsc package's identity flag (see rsc package help) passes this file to
  codesign's entitlements flag when set.
-->
<dict/>
</plist>
"#
    .to_string()
}

/// `windows/app.manifest` — a side-by-side manifest (`<exe>.exe.manifest`,
/// no resource compiler needed — Windows loads it automatically if it sits
/// next to the executable). Declares DPI awareness and a normal (non-admin)
/// execution level. Icon-in-exe embedding would need `rc.exe`, which isn't
/// available to verify on the machines this was built on — see the Known
/// Issues note in `.steering/CRATE_CONTRACTS.md`.
fn windows_app_manifest(name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="{name}" version="0.1.0.0" processorArchitecture="*"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <!-- DPI awareness: render at native resolution instead of being scaled/blurred. -->
  </application>
  <asmv3:application xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
    <asmv3:windowsSettings xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">
      <dpiAware>true/PM</dpiAware>
    </asmv3:windowsSettings>
  </asmv3:application>
</assembly>
"#
    )
}

/// `linux/app.desktop` — the freedesktop.org entry that makes the app show
/// up in application menus/launchers with a real name and icon. `Exec`/
/// `Icon` are filled in at `rsc package` time (paths depend on install
/// location); this template ships placeholders `rsc package` substitutes.
fn linux_desktop_entry(name: &str) -> String {
    format!(
        r#"[Desktop Entry]
Type=Application
Name={name}
Exec={{exec}}
Icon={{icon}}
Categories=Utility;
Terminal=false
"#
    )
}

// ── Android Gradle project (D106 Phase 24 Step 3) ───────────────────────────
//
// A real Gradle project — `build.gradle.kts`, `AndroidManifest.xml`, a
// `MainActivity` — not a placeholder. Plain `Activity` + `SurfaceView` (not
// `GameActivity`/`NativeActivity`): the FFI boundary already drives the
// engine explicitly via JNI calls from Kotlin (init/resize/touch/frame), so
// there's no need for `android-activity`'s native-entrypoint machinery —
// that's for apps that want Rust/C++ to own `android_main` directly, which
// isn't this design (mirrors why iOS's Step 2 uses a thin Swift
// AppDelegate rather than letting winit's implicit one run). The Rust
// engine compiles to a `cdylib` (`libapp_lib_name.so`), loaded via
// `System.loadLibrary` and called through the `Java_*`-named JNI functions
// `ffi_rs` generates (see `jni_class_prefix`).

fn android_settings_gradle(name: &str) -> String {
    format!(
        r#"pluginManagement {{
    repositories {{
        google()
        mavenCentral()
        gradlePluginPortal()
    }}
}}
dependencyResolutionManagement {{
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {{
        google()
        mavenCentral()
    }}
}}

rootProject.name = "{name}"
include(":app")
"#
    )
}

fn android_root_build_gradle() -> String {
    r#"plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
}
"#
    .to_string()
}

/// `app/build.gradle.kts` — `jniLibs.srcDirs` points at a directory the
/// Cargo build (a `PreBuild`-wired Gradle task, see below) populates with
/// the cross-compiled `.so` per Android ABI before Gradle packages the APK,
/// mirroring how iOS's `PBXShellScriptBuildPhase` runs `cargo build` before
/// Xcode links (Step 2). `abiFilters` is `arm64-v8a` only for now — the one
/// ABI this project's `.cargo/config.toml` linker setup (and Apple Silicon
/// emulators) actually need; widen this once cross-building for more ABIs
/// is verified rather than claiming untested coverage.
fn android_app_build_gradle(bundle_id: &str, crate_lib_name: &str) -> String {
    let package = android_package(bundle_id);
    format!(
        r#"plugins {{
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}}

android {{
    namespace = "{package}"
    compileSdk = 34

    defaultConfig {{
        applicationId = "{package}"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"
        ndk {{
            abiFilters += listOf("arm64-v8a")
        }}
    }}

    buildTypes {{
        release {{
            isMinifyEnabled = false
        }}
    }}
    compileOptions {{
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }}
    kotlinOptions {{
        jvmTarget = "17"
    }}
    sourceSets {{
        getByName("main") {{
            jniLibs.srcDirs("src/main/jniLibs")
        }}
    }}
}}

// Builds the Rust cdylib for the target ABI(s) and stages it into
// src/main/jniLibs/<abi>/ before Gradle's own resource-merge step picks it
// up via the jniLibs.srcDirs above — the Android counterpart to Step 2's
// Xcode PBXShellScriptBuildPhase. Verified: this task, followed by
// assembleDebug, produces a real .so-containing APK (see .steering/
// PHASE_24.md's Step 3 verification note); NDK path matches this machine's
// install and isn't yet configurable — a real per-project setup would read
// it from ANDROID_NDK_HOME, tracked as follow-up.
tasks.register("cargoBuildAndroid") {{
    doLast {{
        val abi = "arm64-v8a"
        val rustTriple = "aarch64-linux-android"
        // NDK root from the environment, not a hardcoded machine path —
        // ANDROID_NDK_HOME if set, else the newest version under
        // $ANDROID_HOME/ndk. Host-tag ("darwin-x86_64" etc.) still assumes
        // the NDK's own prebuilt-toolchain naming; only macOS/Linux/Windows
        // x86_64 hosts are handled, matching what this project has
        // actually been verified on (see .steering/CRATE_CONTRACTS.md
        // Known Issues) — ARM-host NDK layouts are a follow-up.
        val ndkHome = System.getenv("ANDROID_NDK_HOME")
            ?: File(System.getenv("ANDROID_HOME") ?: "${{System.getProperty("user.home")}}/Library/Android/sdk", "ndk")
                .listFiles()?.maxByOrNull {{ it.name }}?.absolutePath
            ?: throw GradleException("Set ANDROID_NDK_HOME, or install an NDK under \$ANDROID_HOME/ndk")
        val hostTag = when {{
            org.gradle.internal.os.OperatingSystem.current().isMacOsX -> "darwin-x86_64"
            org.gradle.internal.os.OperatingSystem.current().isLinux -> "linux-x86_64"
            else -> "windows-x86_64"
        }}
        val minSdk = 24
        val toolchainBin = "$ndkHome/toolchains/llvm/prebuilt/$hostTag/bin"
        val linker = "$toolchainBin/aarch64-linux-android$minSdk-clang"
        // C/C++ compiler + archiver for the target. REQUIRED: several Rust
        // deps compile C — `ring` (rustls TLS, via networking), `rusqlite`'s
        // bundled SQLite (persistence), `ndk-sys`. Without these env vars the
        // `cc` crate looks for a bare `aarch64-linux-android-clang` (no API
        // level) that the NDK doesn't ship, and the build fails. (Phase 24's
        // Android build predated all the C-compiling deps, so the original
        // template only set the linker — this is the fix for that gap.)
        val cc = "$toolchainBin/aarch64-linux-android$minSdk-clang"
        val cxx = "$toolchainBin/aarch64-linux-android$minSdk-clang++"
        val ar = "$toolchainBin/llvm-ar"
        // Plain ProcessBuilder, not Gradle's exec DSL block — that's a
        // Project extension function not reliably reachable from inside a
        // registered task's doLast across Gradle/Kotlin-DSL versions
        // (confirmed: "Unresolved reference 'exec'" against this project's
        // Gradle 9.4 — plain JVM process APIs sidestep that entirely).
        // Dev hot reload (Tier 1): `RSC_HOT=1` (set by `rsc dev --target
        // android`) builds a debug lib WITH the `rosace-ffi/rsc-hot` feature so
        // the app opens its reload socket; otherwise a normal release lib.
        val cargoArgs = mutableListOf("cargo", "build", "--lib", "--target", rustTriple)
        if (System.getenv("RSC_HOT") == "1") {{
            cargoArgs.add("--features"); cargoArgs.add("rosace-ffi/rsc-hot")
        }} else {{
            cargoArgs.add("--release")
        }}
        val processBuilder = ProcessBuilder(cargoArgs)
        processBuilder.directory(rootProject.projectDir.parentFile)
        val env = processBuilder.environment()
        env["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"] = linker
        env["CC_aarch64-linux-android"] = cc
        env["CXX_aarch64-linux-android"] = cxx
        env["AR_aarch64-linux-android"] = ar
        // `cc`/`cmake` crates and NDK tooling also consult these.
        env["ANDROID_NDK_ROOT"] = ndkHome
        env["PATH"] = "$toolchainBin${{File.pathSeparator}}${{env["PATH"] ?: ""}}"
        processBuilder.inheritIO()
        val exitCode = processBuilder.start().waitFor()
        if (exitCode != 0) {{
            throw GradleException("cargo build failed with exit code $exitCode")
        }}
        val src = rootProject.projectDir.parentFile
            .resolve("target/$rustTriple/release/lib{crate_lib_name}.so")
        val destDir = projectDir.resolve("src/main/jniLibs/$abi")
        destDir.mkdirs()
        src.copyTo(destDir.resolve("lib{crate_lib_name}.so"), overwrite = true)
    }}
}}

tasks.named("preBuild") {{
    dependsOn("cargoBuildAndroid")
}}

dependencies {{
}}
"#
    )
}

fn android_gradle_properties() -> String {
    r#"org.gradle.jvmargs=-Xmx2048m
android.useAndroidX=true
kotlin.code.style=official
"#
    .to_string()
}

fn android_manifest_xml(name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <!-- Dev hot reload (Tier 1) binds a localhost socket to receive pushed
         `view!` edits over `adb forward`. INTERNET covers the localhost bind;
         it has no effect on a release build (no socket is opened). -->
    <uses-permission android:name="android.permission.INTERNET" />

    <application
        android:allowBackup="true"
        android:icon="@mipmap/ic_launcher"
        android:roundIcon="@mipmap/ic_launcher_round"
        android:label="@string/app_name"
        android:theme="@android:style/Theme.Black.NoTitleBar.Fullscreen">
        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:configChanges="orientation|screenSize|keyboardHidden|uiMode|fontScale"
            android:windowSoftInputMode="adjustNothing"
            android:label="{name}">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>

</manifest>
"#
    )
}

fn android_strings_xml(name: &str) -> String {
    format!(
        r#"<resources>
    <string name="app_name">{name}</string>
</resources>
"#
    )
}

/// `MainActivity.kt` — owns the app lifecycle (unlike winit's implicit
/// Android activity), drives the engine through the JNI boundary `ffi_rs`
/// generates. `SurfaceView` + `SurfaceHolder.Callback` gets a real
/// `android.view.Surface`; `Choreographer.postFrameCallback` drives the
/// render loop (the Android counterpart to iOS's `CADisplayLink`, already
/// verified in Step 1/2); touch events forward through `onTouchEvent`.
fn android_main_activity_kt(bundle_id: &str, crate_lib_name: &str) -> String {
    let package = android_package(bundle_id);
    format!(
        r#"package {package}

import android.app.Activity
import android.content.Context
import android.content.res.Configuration
import android.os.Bundle
import android.provider.Settings
import android.text.InputType
import android.view.Choreographer
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import android.graphics.Rect
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityNodeProvider
import org.json.JSONObject

/// A `SurfaceView` that can receive the soft keyboard's `InputConnection`
/// (D116 Step 6, Android). A plain `SurfaceView` isn't a text editor, so the
/// OS never offers to show a keyboard for it — opting in via
/// `onCheckIsTextEditor`/`onCreateInputConnection` is the same mechanism a
/// custom text-editing widget uses, mirroring iOS's `UIKeyInput` conformance
/// on the Metal view. Typed characters and special keys (Backspace/Enter/Tab)
/// are forwarded out through the two callbacks rather than calling the JNI
/// bridge directly, so this view has no engine-handle knowledge of its own.
private class EngineSurfaceView(
    context: Context,
    private val onText: (Int) -> Unit,
    private val onKey: (Int) -> Unit,
    /// Supplies the engine's semantic tree as JSON (D132). A callback, like
    /// `onText`/`onKey`, so this view keeps no engine-handle knowledge.
    private val semanticsJson: () -> String?,
) : SurfaceView(context) {{
    // RSC_KEY_* (rosace_ffi::event) — Enter/Tab/Backspace are commands, never
    // literal text, same convention iOS's insertText special-cases them with.
    private val keyEnter = 0
    private val keyBackspace = 3
    private val keyTab = 4

    var keyboardInputType: Int = InputType.TYPE_CLASS_TEXT

    init {{
        isFocusable = true
        isFocusableInTouchMode = true
        // Without this the view never reaches the accessibility tree at all:
        // a SurfaceView carries no text or contentDescription, so the default
        // IMPORTANT_FOR_ACCESSIBILITY_AUTO resolves to "not important" and
        // `getAccessibilityNodeProvider` is never called. Verified with
        // `uiautomator dump`, which showed only the parent FrameLayout (D132).
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
        // We paint our own content, so the host node itself should not be a
        // leaf that swallows focus — the virtual children carry the meaning.
        contentDescription = null
    }}

    // -- Accessibility (D132) ------------------------------------------
    //
    // ROSACE draws every pixel into this one SurfaceView, so without the
    // provider below TalkBack sees a single unlabelled rectangle.
    //
    // Android's model is NOT iOS's. UIKit takes an array of element objects;
    // Android asks for one node at a time by an Int "virtual view id" and
    // expects parent/child links expressed as ids. So the tree is flattened
    // once per query and the LIST INDEX is used as the id — our own semantic
    // ids are u64 and would not fit an Int.
    //
    // Pull, not push: these methods are only called while an accessibility
    // service is exploring, so TalkBack-off costs nothing.

    private class A11yNode(
        val label: String,
        val role: String,
        val bounds: Rect?,
        val children: MutableList<Int> = mutableListOf(),
        var parent: Int = AccessibilityNodeProvider.HOST_VIEW_ID,
    )

    private fun flatten(): List<A11yNode> {{
        val json = semanticsJson() ?: return emptyList()
        val out = mutableListOf<A11yNode>()
        try {{
            walk(JSONObject(json), out, AccessibilityNodeProvider.HOST_VIEW_ID)
        }} catch (e: Exception) {{
            return emptyList()
        }}
        return out
    }}

    /// Same two rules as the iOS bridge, for the same reasons: an
    /// interactive control speaks for its subtree (otherwise a Button and
    /// the Text inside it both become nodes on one rect), and a container is
    /// emitted AFTER its children so its full-width rect does not occlude
    /// them in hit-testing order.
    private fun walk(node: JSONObject, out: MutableList<A11yNode>, parent: Int) {{
        // `optString` returns the literal string "null" for a JSON null —
        // org.json's long-standing gotcha. Read through `isNull` or TalkBack
        // announces a phantom node that literally says "null" (seen in a
        // uiautomator dump before this guard).
        val rawLabel = if (node.isNull("label")) "" else node.optString("label", "")
        val rawValue = if (node.isNull("value")) "" else node.optString("value", "")
        val label = rawLabel.ifEmpty {{ rawValue }}
        val role = node.optString("role", "unknown")
        val speaks = label.isNotEmpty()
        val kids = node.optJSONArray("children")

        if (speaks && isInteractive(role)) {{
            out.add(makeNode(node, label, role, parent))
            return
        }}
        val childIds = mutableListOf<Int>()
        if (kids != null) {{
            for (i in 0 until kids.length()) {{
                val before = out.size
                walk(kids.getJSONObject(i), out, parent)
                for (j in before until out.size) {{
                    if (out[j].parent == parent) childIds.add(j)
                }}
            }}
        }}
        if (speaks) {{
            out.add(makeNode(node, label, role, parent))
        }}
    }}

    private fun makeNode(node: JSONObject, label: String, role: String, parent: Int): A11yNode {{
        val b = node.optJSONObject("bounds")
        var rect: Rect? = null
        if (b != null) {{
            // Rust reports LOGICAL, view-relative px. AccessibilityNodeInfo
            // wants PHYSICAL screen px, so scale by density and offset by the
            // view's position — the mirror of the conversion iOS does with
            // UIAccessibility.convertToScreenCoordinates.
            val d = resources.displayMetrics.density
            val loc = IntArray(2)
            getLocationOnScreen(loc)
            val x = (b.optDouble("x", 0.0) * d).toInt() + loc[0]
            val y = (b.optDouble("y", 0.0) * d).toInt() + loc[1]
            val w = (b.optDouble("w", 0.0) * d).toInt()
            val h = (b.optDouble("h", 0.0) * d).toInt()
            rect = Rect(x, y, x + w, y + h)
        }}
        val n = A11yNode(label, role, rect)
        n.parent = parent
        return n
    }}

    private fun isInteractive(role: String): Boolean = when (role) {{
        "button", "checkbox", "radio", "switch", "textinput",
        "link", "slider", "tab", "menuitem" -> true
        else -> false
    }}

    /// TalkBack derives the spoken role from the class name, the way it does
    /// for real framework widgets.
    private fun classNameFor(role: String): String = when (role) {{
        "button", "menuitem", "tab" -> "android.widget.Button"
        "checkbox" -> "android.widget.CheckBox"
        "radio" -> "android.widget.RadioButton"
        "switch" -> "android.widget.Switch"
        "textinput" -> "android.widget.EditText"
        "image" -> "android.widget.ImageView"
        "slider", "progressbar" -> "android.widget.SeekBar"
        else -> "android.widget.TextView"
    }}

    private val provider = object : AccessibilityNodeProvider() {{
        override fun createAccessibilityNodeInfo(virtualViewId: Int): AccessibilityNodeInfo? {{
            val nodes = flatten()
            if (virtualViewId == HOST_VIEW_ID) {{
                val info = AccessibilityNodeInfo.obtain(this@EngineSurfaceView)
                onInitializeAccessibilityNodeInfo(info)
                // Only top-level nodes attach to the host; nested ones are
                // reached through their own parent.
                nodes.forEachIndexed {{ i, n ->
                    if (n.parent == HOST_VIEW_ID) info.addChild(this@EngineSurfaceView, i)
                }}
                return info
            }}
            val n = nodes.getOrNull(virtualViewId) ?: return null
            val info = AccessibilityNodeInfo.obtain(this@EngineSurfaceView, virtualViewId)
            info.className = classNameFor(n.role)
            info.text = n.label
            info.contentDescription = n.label
            info.packageName = context.packageName
            info.setParent(this@EngineSurfaceView)
            n.bounds?.let {{ info.setBoundsInScreen(it) }}
            info.isVisibleToUser = true
            info.isEnabled = true
            if (isInteractive(n.role)) {{
                info.isClickable = true
                info.isFocusable = true
                info.addAction(AccessibilityNodeInfo.ACTION_CLICK)
            }}
            return info
        }}

        override fun performAction(virtualViewId: Int, action: Int, arguments: Bundle?): Boolean {{
            // Activation would have to route back into the engine's
            // hit-test/dispatch path, which this view deliberately has no
            // handle for. Named as a gap in D132 rather than silently
            // reporting success for something that did nothing.
            return false
        }}

        override fun findFocus(focus: Int): AccessibilityNodeInfo? = null
    }}

    override fun getAccessibilityNodeProvider(): AccessibilityNodeProvider = provider

    override fun onCheckIsTextEditor(): Boolean = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {{
        outAttrs.inputType = keyboardInputType
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN
        return object : BaseInputConnection(this, false) {{
            override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {{
                text.codePoints().forEach {{ onText(it) }}
                return true
            }}

            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {{
                // Predictive-input/composing backspace arrives this way
                // instead of a KeyEvent — treat each deleted char the same
                // as a real Backspace keypress.
                repeat(beforeLength) {{ onKey(keyBackspace) }}
                return true
            }}

            override fun sendKeyEvent(event: KeyEvent): Boolean {{
                if (event.action == KeyEvent.ACTION_DOWN) {{
                    when (event.keyCode) {{
                        KeyEvent.KEYCODE_DEL -> onKey(keyBackspace)
                        KeyEvent.KEYCODE_ENTER -> onKey(keyEnter)
                        KeyEvent.KEYCODE_TAB -> onKey(keyTab)
                    }}
                }}
                return super.sendKeyEvent(event)
            }}
        }}
    }}
}}

class MainActivity : Activity(), SurfaceHolder.Callback {{

    companion object {{
        init {{ System.loadLibrary("{crate_lib_name}") }}
    }}

    private external fun nativeInit(surface: Surface, width: Int, height: Int, scale: Float): Long
    private external fun nativeResize(
        handle: Long, width: Int, height: Int, scale: Float,
        safeTop: Float, safeRight: Float, safeBottom: Float, safeLeft: Float,
    )
    // D127 "environment" track — live OS brightness/accessibility push,
    // called once from surfaceCreated and again from every
    // onConfigurationChanged.
    private external fun nativeSetMediaQuery(
        handle: Long, isDark: Boolean, textScale: Float,
        boldText: Boolean, reduceMotion: Boolean, always24HourFormat: Boolean,
    )
    private external fun nativeTouch(handle: Long, kind: Int, x: Float, y: Float)
    private external fun nativeKey(handle: Long, key: Int)
    private external fun nativeText(handle: Long, character: Int)
    private external fun nativeSemanticsJson(handle: Long): String?
    private external fun nativeTextInputActive(): Boolean
    private external fun nativeFocusedKeyboardType(): Int
    // Platform Channel (D127) — the generic bidirectional method-call bridge
    // to native code, mirroring the plain-C exports iOS's EngineViewController
    // declares via @_silgen_name. take_outgoing is the host's ONE per-frame
    // poll (see pollPlatformChannel below); dispatch is the reverse direction
    // (native calling a Rust-registered handler), included for completeness
    // even though this template doesn't call it itself.
    private external fun nativeTakeOutgoingPlatformCalls(): String?
    private external fun nativePlatformChannelReportResult(callId: Long, resultJson: String)
    private external fun nativePlatformChannelReportError(callId: Long, message: String)
    private external fun nativePlatformChannelDispatch(channel: String, method: String, argsJson: String): String?
    private external fun nativeLifecycle(handle: Long, kind: Int)
    private external fun nativeFrame(handle: Long)
    private external fun nativeShutdown(handle: Long)

    private var engineHandle: Long = 0
    private lateinit var surfaceView: EngineSurfaceView

    private val frameCallback = object : Choreographer.FrameCallback {{
        override fun doFrame(frameTimeNanos: Long) {{
            if (engineHandle != 0L) {{
                nativeFrame(engineHandle)
                pollPlatformChannel()
                syncSoftKeyboard()
                Choreographer.getInstance().postFrameCallback(this)
            }}
        }}
    }}

    override fun onCreate(savedInstanceState: Bundle?) {{
        super.onCreate(savedInstanceState)
        surfaceView = EngineSurfaceView(
            this,
            onText = {{ c -> if (engineHandle != 0L) nativeText(engineHandle, c) }},
            onKey = {{ k -> if (engineHandle != 0L) nativeKey(engineHandle, k) }},
            // D132: TalkBack pulls the tree through here, only while an
            // accessibility service is actually exploring.
            semanticsJson = {{ if (engineHandle != 0L) nativeSemanticsJson(engineHandle) else null }},
        )
        surfaceView.holder.addCallback(this)
        setContentView(surfaceView)
    }}

    /// Show/hide/reconfigure the soft keyboard to match the engine's focused
    /// field (D116 Step 6) — the Android counterpart of iOS's
    /// `syncSoftKeyboard`, polled once per frame tick the same way.
    private fun uiInputTypeFor(hint: Int): Int = when (hint) {{
        1 -> InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS // RSC_KEYBOARD_EMAIL
        2 -> InputType.TYPE_CLASS_NUMBER                                              // RSC_KEYBOARD_NUMERIC
        3 -> InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI           // RSC_KEYBOARD_URL
        4 -> InputType.TYPE_CLASS_PHONE                                              // RSC_KEYBOARD_PHONE
        else -> InputType.TYPE_CLASS_TEXT                                            // RSC_KEYBOARD_DEFAULT
    }}

    private fun syncSoftKeyboard() {{
        val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        if (nativeTextInputActive()) {{
            val want = uiInputTypeFor(nativeFocusedKeyboardType())
            if (want != surfaceView.keyboardInputType) {{
                surfaceView.keyboardInputType = want
                if (surfaceView.isFocused) imm.restartInput(surfaceView)
            }}
            if (!surfaceView.isFocused) {{
                surfaceView.requestFocus()
                imm.showSoftInput(surfaceView, InputMethodManager.SHOW_IMPLICIT)
            }}
        }} else if (surfaceView.isFocused) {{
            imm.hideSoftInputFromWindow(surfaceView.windowToken, 0)
        }}
    }}

    /// The host's ONE per-frame poll for outgoing Platform Channel calls
    /// (D127). Unlike iOS (which already had a real push-permission flow to
    /// migrate onto this), Android push permission (`POST_NOTIFICATIONS`,
    /// API 33+) was never wired here — so `"rosace/push"` is deliberately
    /// NOT recognized below yet (a named follow-up, not a regression: there
    /// was nothing to preserve). An app wanting its own channel (camera, a
    /// custom native SDK, or building out real Android push support) adds a
    /// case here for its own channel name and reports back via
    /// `nativePlatformChannelReportResult`/`_ReportError`.
    private fun pollPlatformChannel() {{
        val json = nativeTakeOutgoingPlatformCalls() ?: return
        val calls = try {{ org.json.JSONArray(json) }} catch (e: org.json.JSONException) {{ return }}
        for (i in 0 until calls.length()) {{
            val call = calls.optJSONObject(i) ?: continue
            val channel = call.optString("channel")
            val method = call.optString("method")
            // (no built-in channels recognized yet — see the doc above;
            // logged so a custom channel's calls are visible during dev)
            android.util.Log.d("rosace", "Platform Channel call: $channel/$method (unhandled)")
        }}
    }}

    // App lifecycle -> RSC_EVENT_LIFECYCLE_* (D110 Phase 29 Step 1);
    // kinds match rsc_engine.h (8 = active, 9 = inactive, 10 = background).
    // Android has no reliable pre-kill callback, so SUSPENDED (11) is not
    // sent — onDestroy is not guaranteed to run. Applied immediately on
    // the Rust side, so onStop's event lands even though the Choreographer
    // callback has gone quiet by then.
    override fun onResume() {{
        super.onResume()
        if (engineHandle != 0L) nativeLifecycle(engineHandle, 8)
    }}

    override fun onPause() {{
        super.onPause()
        if (engineHandle != 0L) nativeLifecycle(engineHandle, 9)
    }}

    override fun onStop() {{
        super.onStop()
        if (engineHandle != 0L) nativeLifecycle(engineHandle, 10)
    }}

    override fun surfaceCreated(holder: SurfaceHolder) {{
        val scale = resources.displayMetrics.density
        val width = surfaceView.width
        val height = surfaceView.height
        engineHandle = nativeInit(holder.surface, width, height, scale)
        Choreographer.getInstance().postFrameCallback(frameCallback)
        syncMediaQuery()
    }}

    // MARK: Environment (D127) — OS brightness/font-scale/reduce-motion,
    // pushed live via nativeSetMediaQuery whenever the OS reports a change.
    // `android:configChanges` in AndroidManifest.xml lists `uiMode|fontScale`
    // so this Activity survives the change and gets the live callback below,
    // instead of being torn down and recreated (which would lose in-memory
    // engine state on every dark-mode toggle).

    /// No clean OS-wide "bold text everywhere" source on Android (unlike
    /// iOS's `UIAccessibility.isBoldTextEnabled`) — stays `false`, a
    /// documented platform gap (see `rosace_core::media_query`'s doc).
    private fun syncMediaQuery() {{
        if (engineHandle == 0L) return
        val uiMode = resources.configuration.uiMode
        val isDark = (uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
        val textScale = resources.configuration.fontScale
        val reduceMotion = Settings.Global.getFloat(
            contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f,
        ) == 0f
        val always24Hour = android.text.format.DateFormat.is24HourFormat(this)
        nativeSetMediaQuery(engineHandle, isDark, textScale, false, reduceMotion, always24Hour)
    }}

    override fun onConfigurationChanged(newConfig: Configuration) {{
        super.onConfigurationChanged(newConfig)
        syncMediaQuery()
    }}

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {{
        if (engineHandle == 0L) return
        val scale = resources.displayMetrics.density
        // Basic safe-area: only the status bar height (systemWindowInsetTop),
        // not a full WindowInsets-driven cutout/gesture-nav treatment — a
        // known simplification (see .steering/CRATE_CONTRACTS.md Known
        // Issues), the Android counterpart of iOS's real UIView.safeAreaInsets
        // (Step 2) is follow-up work, not silently claimed equivalent here.
        nativeResize(engineHandle, width, height, scale, 0f, 0f, 0f, 0f)
    }}

    override fun surfaceDestroyed(holder: SurfaceHolder) {{
        if (engineHandle == 0L) return
        nativeShutdown(engineHandle)
        engineHandle = 0
    }}

    override fun onTouchEvent(event: MotionEvent): Boolean {{
        if (engineHandle == 0L) return false
        val kind = when (event.actionMasked) {{
            MotionEvent.ACTION_DOWN -> 1
            MotionEvent.ACTION_MOVE -> 0
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> 2
            else -> return false
        }}
        // MotionEvent x/y are PHYSICAL pixels; the engine hit-tests in LOGICAL
        // coordinates (like the desktop `position.x / scale_factor` path and
        // iOS's already-logical `touch.location(in: view)`). Divide by the same
        // density used for nativeResize — without this, taps on a hi-DPI screen
        // land at 2-3x the intended point and every click misses its target.
        val density = resources.displayMetrics.density
        nativeTouch(engineHandle, kind, event.x / density, event.y / density)
        return true
    }}
}}
"#
    )
}

// ── iOS Swift host (D106 Phase 24 Step 2) ───────────────────────────────────
//
// Our own AppDelegate/SceneDelegate own the app lifecycle — not winit's
// implicit one (D106's whole point). `EngineViewController` is the real
// version of the throwaway stub validated in Step 1: a CAMetalLayer-backed
// view driving `rsc_engine_init`/`resize`/`input`/`frame` through the FFI
// boundary `rosace-ffi` provides. FFI functions are declared via
// `@_silgen_name` (no bridging header needed) — the same mechanism proven
// working in `rosace-ffi/examples/ios_stub.rs`'s Simulator verification.
// `Info.plist` is Xcode-synthesized (`GENERATE_INFOPLIST_FILE = YES` in the
// generated `.pbxproj`) rather than a physical file here — the physical
// `ios/Info.plist` this module also generates is for the OLDER Phase 20-22
// hand-rolled `rsc run --target ios` harness only (kept working per the
// Migration Rule until Step 4 retires it); the two are independent.

const IOS_APP_DELEGATE_SWIFT: &str = r#"//! Owns the app lifecycle — our own AppDelegate, not winit's implicit one
//! (this is the whole point of D106: winit's iOS backend calls
//! UIApplicationMain itself and generates an AppDelegate no host code can
//! reach, which blocks push notifications, deep links, and background
//! tasks).

import UIKit
import UserNotifications

// Push-notification FFI (D110 Phase 29 Step 2) — the app's own staticlib
// exports these (src/ffi.rs); same @_silgen_name mechanism as the engine
// calls in EngineViewController.swift.
@_silgen_name("rsc_push_report_token")
private func rsc_push_report_token(_ token: UnsafePointer<CChar>?)
@_silgen_name("rsc_push_report_notification")
private func rsc_push_report_notification(
    _ title: UnsafePointer<CChar>?, _ body: UnsafePointer<CChar>?, _ payload: UnsafePointer<CChar>?
)

@main
final class AppDelegate: UIResponder, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        // Foreground-delivery hook — must be set before any notification
        // can arrive, so it lives here, not behind the permission request.
        UNUserNotificationCenter.current().delegate = self
        return true
    }

    func application(
        _ application: UIApplication,
        configurationForConnecting connectingSceneSession: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        let config = UISceneConfiguration(name: "Default", sessionRole: connectingSceneSession.role)
        config.delegateClass = SceneDelegate.self
        return config
    }

    // MARK: Push registration outcome (D110 Phase 29 Step 2)

    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        let token = deviceToken.map { String(format: "%02x", $0) }.joined()
        token.withCString { rsc_push_report_token($0) }
    }

    func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        // A legitimate outcome (no aps-environment entitlement, Simulator
        // without a signing team, no network) — the permission result was
        // already reported; the token atom simply stays unset.
        NSLog("rosace push: APNs registration failed: \(error.localizedDescription)")
    }

    // MARK: Foreground delivery (D110 Phase 29 Step 2)

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        let content = notification.request.content
        var payload = "{}"
        if let userInfo = content.userInfo as? [String: Any],
           JSONSerialization.isValidJSONObject(userInfo),
           let data = try? JSONSerialization.data(withJSONObject: userInfo),
           let s = String(data: data, encoding: .utf8) {
            payload = s
        }
        content.title.withCString { t in
            content.body.withCString { b in
                payload.withCString { p in rsc_push_report_notification(t, b, p) }
            }
        }
        completionHandler([.banner, .sound])
    }
}
"#;

const IOS_SCENE_DELEGATE_SWIFT: &str = r#"import UIKit

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = EngineViewController()
        window.makeKeyAndVisible()
        self.window = window
    }
}
"#;

const IOS_ENGINE_VIEW_CONTROLLER_SWIFT: &str = r#"//! Drives the ROSACE engine through the `rosace-ffi` C boundary
//! (`rosace-ffi/include/rsc_engine.h`) — a CAMetalLayer-backed view,
//! init/resize/input/frame calls, and real `UIView.safeAreaInsets` feeding
//! `rosace_core::SafeArea` (replacing the old winit outer/inner-size
//! workaround from Phase 20-22).

import UIKit
import QuartzCore
import UserNotifications

// MARK: - FFI declarations (mirrors rosace-ffi/include/rsc_engine.h;
// no bridging header needed — matches the pattern proven in
// rosace-ffi/examples/ios_stub.rs's Simulator verification).

typealias RscEngine = OpaquePointer

struct RscInputEvent {
    var kind: UInt32
    var x: Float
    var y: Float
    var button: UInt32
    var key: UInt32
    var character: UInt32
    var width: UInt32
    var height: UInt32
    var delta_x: Float
    var delta_y: Float
}

private let RSC_EVENT_MOUSE_MOVE: UInt32 = 0
private let RSC_EVENT_MOUSE_DOWN: UInt32 = 1
private let RSC_EVENT_MOUSE_UP: UInt32 = 2
private let RSC_BUTTON_LEFT: UInt32 = 0
// Text input (D116 Step 6): typed characters go as RSC_EVENT_TEXT; backspace
// as a RSC_EVENT_KEY_DOWN carrying RSC_KEY_BACKSPACE — same events the desktop
// winit host produces, so the engine's editor handles them identically.
private let RSC_EVENT_KEY_DOWN: UInt32 = 3
private let RSC_EVENT_TEXT: UInt32 = 5
private let RSC_KEY_ENTER: UInt32 = 0
private let RSC_KEY_BACKSPACE: UInt32 = 3
private let RSC_KEY_TAB: UInt32 = 4
private let RSC_EVENT_LIFECYCLE_ACTIVE: UInt32 = 8
private let RSC_EVENT_LIFECYCLE_INACTIVE: UInt32 = 9
private let RSC_EVENT_LIFECYCLE_BACKGROUND: UInt32 = 10
private let RSC_EVENT_LIFECYCLE_SUSPENDED: UInt32 = 11

@_silgen_name("rsc_engine_init")
func rsc_engine_init(_ surfaceHandle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32, _ scale: Float) -> RscEngine?

@_silgen_name("rsc_engine_resize")
func rsc_engine_resize(
    _ engine: RscEngine?, _ width: UInt32, _ height: UInt32, _ scale: Float,
    _ safeTop: Float, _ safeRight: Float, _ safeBottom: Float, _ safeLeft: Float
)

@_silgen_name("rsc_engine_input")
func rsc_engine_input(_ engine: RscEngine?, _ events: UnsafePointer<RscInputEvent>?, _ count: Int)

@_silgen_name("rsc_engine_frame")
func rsc_engine_frame(_ engine: RscEngine?)

@_silgen_name("rsc_engine_shutdown")
func rsc_engine_shutdown(_ engine: RscEngine?)

@_silgen_name("rsc_push_permission_report_result")
func rsc_push_permission_report_result(_ granted: UInt8)

@_silgen_name("rsc_text_input_active")
func rsc_text_input_active() -> UInt8

@_silgen_name("rsc_focused_keyboard_type")
func rsc_focused_keyboard_type() -> UInt32

// D127 "environment" track — live OS brightness/accessibility push, same
// shape as `rsc_engine_resize`'s safe-area push above.
@_silgen_name("rsc_engine_set_media_query")
func rsc_engine_set_media_query(
    _ engine: RscEngine?, _ isDark: UInt8, _ textScale: Float,
    _ boldText: UInt8, _ reduceMotion: UInt8, _ always24HourFormat: UInt8
)

// MARK: - Platform Channel (D127) — the generic bidirectional method-call
// bridge to native code. `take_outgoing`/`report_result`/`report_error`
// replace the old dedicated push-permission-only poll — that discovery now
// goes through this same generic queue, alongside anything an app registers
// itself. `dispatch` is the reverse direction (native calling a
// Rust-registered handler), included for completeness
// even though this template doesn't call it itself.

@_silgen_name("rsc_platform_channel_take_outgoing")
func rsc_platform_channel_take_outgoing() -> UnsafeMutablePointer<CChar>?

@_silgen_name("rsc_string_free")
func rsc_string_free(_ ptr: UnsafeMutablePointer<CChar>?)

// Accessibility (D132): the engine's semantic tree as JSON, pulled on
// demand. See `MetalView`'s UIAccessibilityContainer conformance below.
@_silgen_name("rsc_engine_semantics_json")
func rsc_engine_semantics_json(_ engine: RscEngine?) -> UnsafeMutablePointer<CChar>?

@_silgen_name("rsc_platform_channel_report_result")
func rsc_platform_channel_report_result(_ callId: UInt64, _ resultJson: UnsafePointer<CChar>?)

@_silgen_name("rsc_platform_channel_report_error")
func rsc_platform_channel_report_error(_ callId: UInt64, _ message: UnsafePointer<CChar>?)

@_silgen_name("rsc_platform_channel_dispatch")
func rsc_platform_channel_dispatch(
    _ channel: UnsafePointer<CChar>?, _ method: UnsafePointer<CChar>?, _ argsJson: UnsafePointer<CChar>?
) -> UnsafeMutablePointer<CChar>?

// MARK: - View

/// A `CAMetalLayer`-backed view — the surface the Rust engine renders into.
///
/// `contentsScale` is set explicitly in `init` — UIKit only auto-syncs a
/// view's OWN default `CALayer` to the screen's pixel density; overriding
/// `layerClass` with a custom layer (as this does) opts out of that
/// automatic behavior, and a `CAMetalLayer` left at its default
/// `contentsScale = 1.0` renders a blurry, effectively-downscaled image
/// even though the Rust side correctly renders at full physical-pixel
/// resolution — one of the most common CAMetalLayer gotchas. Root-caused
/// and fixed 2026-07-08 after a direct visual report of blurry text.
/// One node of the engine's semantic tree, decoded from the FFI JSON.
private struct RscSemanticNode: Decodable {
    let id: UInt64?
    let role: String
    let label: String?
    let value: String?
    let bounds: RscBounds?
    let children: [RscSemanticNode]

    struct RscBounds: Decodable { let x: Float; let y: Float; let w: Float; let h: Float }
}

final class MetalView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    /// Supplies the engine's semantic tree as JSON. Set by
    /// `EngineViewController`, which owns the engine pointer.
    var semanticsJSONProvider: (() -> String?)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        (layer as! CAMetalLayer).contentsScale = UIScreen.main.scale
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        (layer as! CAMetalLayer).contentsScale = UIScreen.main.scale
    }

    // ── Accessibility (D132) ────────────────────────────────────────────
    //
    // ROSACE paints every pixel into this one CAMetalLayer, so without the
    // bridge below VoiceOver sees a single blank rectangle. UIKit asks for
    // `accessibilityElements` only while VoiceOver is actually inspecting,
    // so the engine is never serialized in the common case — which is why
    // the Rust side exposes this as a PULL rather than pushing each frame.
    //
    // The container must not itself be an element, or VoiceOver stops at
    // the container and never reaches the children.
    override var isAccessibilityElement: Bool {
        get { false }
        set { }
    }

    /// Cached so UIKit's retained element references stay alive.
    ///
    /// Rebuilding on every getter call returns fresh objects each time; the
    /// ones UIKit already handed to VoiceOver then go stale, and the
    /// Accessibility Inspector reports Label/Traits as None on an element
    /// whose header still shows the right name. Keyed on the JSON so the
    /// tree is only re-decoded when it actually changed.
    private var cachedJSON: String?
    private var cachedElements: [UIAccessibilityElement] = []

    override var accessibilityElements: [Any]? {
        get {
            guard let json = semanticsJSON(), !json.isEmpty else { return nil }
            if json != cachedJSON {
                cachedJSON = json
                cachedElements = buildElements(from: json)
            }
            return cachedElements.isEmpty ? nil : cachedElements
        }
        set { }
    }

    private func semanticsJSON() -> String? { semanticsJSONProvider?() }

    private func buildElements(from json: String) -> [UIAccessibilityElement] {
        guard let data = json.data(using: .utf8),
              let root = try? JSONDecoder().decode(RscSemanticNode.self, from: data)
        else { return [] }
        var out: [UIAccessibilityElement] = []
        appendElements(from: root, into: &out)
        return out
    }

    /// Flattens the tree into the linear list VoiceOver swipes through.
    ///
    /// Two rules, both learned from the Accessibility Inspector:
    ///
    /// 1. **An interactive control speaks for its own subtree.** A Button's
    ///    node and the Text inside it carry the same label, so emitting both
    ///    produced two elements stacked on one rect. The control wins and we
    ///    stop descending.
    /// 2. **Containers are emitted AFTER their children.** An AppBar declares
    ///    a heading spanning the whole bar, which contains the Back and Light
    ///    buttons. Emitting the container first put a full-width element on
    ///    top of them, so only the title was reachable. Overlapping frames are
    ///    legal; order decides priority, so children go in first and the
    ///    container still gets announced rather than being dropped.
    private func appendElements(from node: RscSemanticNode, into out: inout [UIAccessibilityElement]) {
        let speaks = (node.label?.isEmpty == false) || (node.value?.isEmpty == false)

        if speaks, isInteractive(node.role), let b = node.bounds {
            out.append(makeElement(node, b))
            return
        }
        for child in node.children {
            appendElements(from: child, into: &out)
        }
        if speaks, let b = node.bounds {
            out.append(makeElement(node, b))
        }
    }

    private func makeElement(_ node: RscSemanticNode, _ b: RscSemanticNode.RscBounds) -> UIAccessibilityElement {
        let element = UIAccessibilityElement(accessibilityContainer: self)
        element.accessibilityLabel = node.label
        element.accessibilityValue = node.value
        element.accessibilityTraits = traits(for: node.role)
        // Rust reports LOGICAL, view-relative pixels; UIKit wants screen
        // coordinates. UIAccessibility does the conversion (including the
        // scale factor), so we must not pre-multiply — the desktop bridge
        // shipped every element at half size by doing exactly that.
        let local = CGRect(x: CGFloat(b.x), y: CGFloat(b.y),
                           width: CGFloat(b.w), height: CGFloat(b.h))
        element.accessibilityFrame = UIAccessibility.convertToScreenCoordinates(local, in: self)
        return element
    }

    /// Roles that represent a control the user operates, rather than content
    /// or grouping. These stop the descent (rule 1 above).
    private func isInteractive(_ role: String) -> Bool {
        switch role {
        case "button", "checkbox", "radio", "switch", "textinput",
             "link", "slider", "tab", "menuitem":
            return true
        default:
            return false
        }
    }

    /// Maps ROSACE roles onto VoiceOver traits. Names must match
    /// `rosace-ffi`'s `role_name` exactly — that function spells them out
    /// literally so this mapping cannot drift with a Rust-side rename.
    private func traits(for role: String) -> UIAccessibilityTraits {
        switch role {
        case "button":      return .button
        case "link":        return .link
        case "heading":     return .header
        case "image":       return .image
        case "textinput":   return .searchField
        case "slider":      return .adjustable
        case "progressbar": return .updatesFrequently
        case "alert":       return .staticText
        case "tab":         return .button
        case "menuitem":    return .button
        // checkbox/radio/switch have no dedicated trait; VoiceOver conveys
        // their on/off state through accessibilityValue, which the engine
        // already supplies, so `.none` here is correct rather than lossy.
        default:            return .none
        }
    }
}

final class EngineViewController: UIViewController, UIKeyInput {
    private var engine: RscEngine?
    private var displayLink: CADisplayLink?

    // MARK: Soft keyboard (D116 Step 6). The Metal view isn't a text field, so
    // the OS shows no keyboard on its own. Adopt `UIKeyInput` and, each tick,
    // become/resign first responder to match the engine's focused text field
    // (`rsc_text_input_active`), configuring the layout from its keyboard-type
    // hint. Keystrokes are forwarded back through `rsc_engine_input`.
    override var canBecomeFirstResponder: Bool { true }
    var keyboardType: UIKeyboardType = .default
    var hasText: Bool { true }

    private func sendKey(_ key: UInt32) {
        guard let engine else { return }
        var e = RscInputEvent(
            kind: RSC_EVENT_KEY_DOWN, x: 0, y: 0, button: 0,
            key: key, character: 0, width: 0, height: 0, delta_x: 0, delta_y: 0
        )
        withUnsafePointer(to: &e) { rsc_engine_input(engine, $0, 1) }
    }

    func insertText(_ text: String) {
        guard let engine else { return }
        for scalar in text.unicodeScalars {
            // Return and Tab are SPECIAL keys, not literal text: the engine
            // treats them as newline/submit and focus-traversal via KeyDown,
            // and drops control chars from the Text path — so forward them as
            // key events (matching what the desktop winit host sends).
            switch scalar {
            case "\n", "\r": sendKey(RSC_KEY_ENTER)
            case "\t":       sendKey(RSC_KEY_TAB)
            default:
                var e = RscInputEvent(
                    kind: RSC_EVENT_TEXT, x: 0, y: 0, button: 0,
                    key: 0, character: scalar.value, width: 0, height: 0, delta_x: 0, delta_y: 0
                )
                withUnsafePointer(to: &e) { rsc_engine_input(engine, $0, 1) }
            }
        }
    }

    func deleteBackward() {
        sendKey(RSC_KEY_BACKSPACE)
    }

    private func uiKeyboardType(for hint: UInt32) -> UIKeyboardType {
        switch hint {
        case 1:  return .emailAddress // RSC_KEYBOARD_EMAIL
        case 2:  return .numberPad    // RSC_KEYBOARD_NUMERIC
        case 3:  return .URL          // RSC_KEYBOARD_URL
        case 4:  return .phonePad     // RSC_KEYBOARD_PHONE
        default: return .default
        }
    }

    /// Show/hide/reconfigure the OS keyboard to match the focused field.
    private func syncSoftKeyboard() {
        if rsc_text_input_active() != 0 {
            let want = uiKeyboardType(for: rsc_focused_keyboard_type())
            if want != keyboardType {
                keyboardType = want
                if isFirstResponder { reloadInputViews() }
            }
            if !isFirstResponder { becomeFirstResponder() }
        } else if isFirstResponder {
            resignFirstResponder()
        }
    }

    override func loadView() {
        view = MetalView(frame: UIScreen.main.bounds)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        let scale = Float(view.contentScaleFactor)
        let width = UInt32(view.bounds.width * CGFloat(scale))
        let height = UInt32(view.bounds.height * CGFloat(scale))
        let viewPtr = Unmanaged.passUnretained(view).toOpaque()
        engine = rsc_engine_init(viewPtr, width, height, scale)

        // Accessibility (D132): hand the view a way to pull the semantic
        // tree. UIKit only calls this while VoiceOver is inspecting, so an
        // app with no screen reader running never serializes anything.
        // `unowned` rather than a strong capture — the view is owned by this
        // controller, so a strong reference here would be a retain cycle.
        if let metalView = view as? MetalView {
            metalView.semanticsJSONProvider = { [unowned self] in
                guard let engine = self.engine,
                      let ptr = rsc_engine_semantics_json(engine) else { return nil }
                defer { rsc_string_free(ptr) }
                return String(cString: ptr)
            }
        }

        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .default)
        displayLink = link

        // MARK: App lifecycle -> RSC_EVENT_LIFECYCLE_* (D110 Phase 29
        // Step 1). UIApplication notifications rather than AppDelegate/
        // SceneDelegate plumbing — this controller owns the engine handle,
        // so no cross-object wiring is needed. The Rust side applies these
        // immediately (not on the next frame): the display link pauses in
        // background, so a frame-queued Background event would only be
        // seen on resume.
        let nc = NotificationCenter.default
        nc.addObserver(self, selector: #selector(lifecycleActive),
                       name: UIApplication.didBecomeActiveNotification, object: nil)
        nc.addObserver(self, selector: #selector(lifecycleInactive),
                       name: UIApplication.willResignActiveNotification, object: nil)
        nc.addObserver(self, selector: #selector(lifecycleBackground),
                       name: UIApplication.didEnterBackgroundNotification, object: nil)
        nc.addObserver(self, selector: #selector(lifecycleSuspended),
                       name: UIApplication.willTerminateNotification, object: nil)

        // Bold Text / Reduce Motion are `UIAccessibility` settings, not
        // `UITraitCollection` traits — they don't fire `traitCollectionDidChange`,
        // so they need their own notifications.
        nc.addObserver(self, selector: #selector(syncMediaQuery),
                       name: UIAccessibility.boldTextStatusDidChangeNotification, object: nil)
        nc.addObserver(self, selector: #selector(syncMediaQuery),
                       name: UIAccessibility.reduceMotionStatusDidChangeNotification, object: nil)
        syncMediaQuery()
    }

    @objc private func lifecycleActive() { sendLifecycle(RSC_EVENT_LIFECYCLE_ACTIVE) }
    @objc private func lifecycleInactive() { sendLifecycle(RSC_EVENT_LIFECYCLE_INACTIVE) }
    @objc private func lifecycleBackground() { sendLifecycle(RSC_EVENT_LIFECYCLE_BACKGROUND) }
    @objc private func lifecycleSuspended() { sendLifecycle(RSC_EVENT_LIFECYCLE_SUSPENDED) }

    private func sendLifecycle(_ kind: UInt32) {
        guard let engine else { return }
        var event = RscInputEvent(
            kind: kind, x: 0, y: 0, button: 0,
            key: 0, character: 0, width: 0, height: 0, delta_x: 0, delta_y: 0
        )
        withUnsafePointer(to: &event) { rsc_engine_input(engine, $0, 1) }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        guard let engine else { return }
        let scale = Float(view.contentScaleFactor)
        let width = UInt32(view.bounds.width * CGFloat(scale))
        let height = UInt32(view.bounds.height * CGFloat(scale))
        let insets = view.safeAreaInsets
        rsc_engine_resize(
            engine, width, height, scale,
            Float(insets.top), Float(insets.right), Float(insets.bottom), Float(insets.left)
        )
    }

    // MARK: Environment (D127) — OS brightness/Dynamic-Type/accessibility,
    // pushed live via `rsc_engine_set_media_query` whenever the OS reports a
    // change, mirroring how safe-area is pushed on every layout pass above.

    /// Apple's documented default point size for `UIFont.TextStyle.body` at
    /// each `UIContentSizeCategory`, expressed as a ratio against `.large`
    /// (17pt — the non-accessibility system default) — the standard
    /// technique for turning Dynamic Type's category enum into the single
    /// float multiplier `rosace_core::MediaQuery.text_scale` expects.
    private func textScale(for category: UIContentSizeCategory) -> Float {
        switch category {
        case .extraSmall:                       return 14.0 / 17.0
        case .small:                             return 15.0 / 17.0
        case .medium:                            return 16.0 / 17.0
        case .large:                             return 1.0
        case .extraLarge:                        return 19.0 / 17.0
        case .extraExtraLarge:                   return 21.0 / 17.0
        case .extraExtraExtraLarge:              return 23.0 / 17.0
        case .accessibilityMedium:               return 28.0 / 17.0
        case .accessibilityLarge:                return 33.0 / 17.0
        case .accessibilityExtraLarge:           return 40.0 / 17.0
        case .accessibilityExtraExtraLarge:      return 47.0 / 17.0
        case .accessibilityExtraExtraExtraLarge: return 53.0 / 17.0
        default:                                 return 1.0
        }
    }

    @objc private func syncMediaQuery() {
        guard let engine else { return }
        let isDark = traitCollection.userInterfaceStyle == .dark
        let scale = textScale(for: traitCollection.preferredContentSizeCategory)
        rsc_engine_set_media_query(
            engine,
            isDark ? 1 : 0,
            scale,
            UIAccessibility.isBoldTextEnabled ? 1 : 0,
            UIAccessibility.isReduceMotionEnabled ? 1 : 0,
            0 // always_24_hour_format: no clean UIKit source — left undetected on iOS for now
        )
    }

    /// Fires live for BOTH userInterfaceStyle (dark mode) and
    /// preferredContentSizeCategory (Dynamic Type) changes — both are
    /// `UITraitCollection` traits.
    override func traitCollectionDidChange(_ previousTraitCollection: UITraitCollection?) {
        super.traitCollectionDidChange(previousTraitCollection)
        syncMediaQuery()
    }

    @objc private func tick() {
        guard let engine else { return }
        rsc_engine_frame(engine)
        pollPlatformChannel()
        syncSoftKeyboard()
    }

    /// The host's ONE per-frame poll for outgoing Platform Channel calls
    /// (D127) — push-permission discovery included, alongside anything an
    /// app registers itself. Recognizes `"rosace/push"` unconditionally
    /// (every app already carries push-permission polling, so there's no
    /// new per-app cost); an app wanting its own channel (camera, a custom
    /// native SDK, …) adds a case here for its own channel name.
    private func pollPlatformChannel() {
        guard let ptr = rsc_platform_channel_take_outgoing() else { return }
        defer { rsc_string_free(ptr) }
        guard let data = String(cString: ptr).data(using: .utf8),
              let calls = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else { return }
        for call in calls {
            guard let channel = call["channel"] as? String, let method = call["method"] as? String else { continue }
            if channel == "rosace/push" && method == "requestPermission" {
                requestPushPermission()
            }
        }
    }

    /// Real OS permission prompt + APNs registration. The result flows back
    /// through `rsc_push_permission_report_result`; a device token (if
    /// registration succeeds — it can legitimately fail without an
    /// aps-environment entitlement) arrives via AppDelegate's
    /// `didRegisterForRemoteNotificationsWithDeviceToken`.
    private func requestPushPermission() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound]) { granted, _ in
            rsc_push_permission_report_result(granted ? 1 : 0)
            if granted {
                DispatchQueue.main.async {
                    UIApplication.shared.registerForRemoteNotifications()
                }
            }
        }
    }

    // MARK: Touch -> MouseDown/MouseMove/MouseUp (same convention the
    // existing winit `Touch` handling and `RscInputEventFfi` conversion use
    // — no separate touch event kind needed).

    private func send(kind: UInt32, touches: Set<UITouch>) {
        guard let engine, let touch = touches.first else { return }
        let p = touch.location(in: view)
        var event = RscInputEvent(
            kind: kind, x: Float(p.x), y: Float(p.y), button: RSC_BUTTON_LEFT,
            key: 0, character: 0, width: 0, height: 0, delta_x: 0, delta_y: 0
        )
        withUnsafePointer(to: &event) { rsc_engine_input(engine, $0, 1) }
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: RSC_EVENT_MOUSE_DOWN, touches: touches)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: RSC_EVENT_MOUSE_MOVE, touches: touches)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: RSC_EVENT_MOUSE_UP, touches: touches)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: RSC_EVENT_MOUSE_UP, touches: touches)
    }

    deinit {
        displayLink?.invalidate()
        if let engine { rsc_engine_shutdown(engine) }
    }
}
"#;

/// `project.pbxproj` for the generated `ios/App.xcodeproj`.
///
/// Uses `PBXFileSystemSynchronizedRootGroup` (Xcode 16+, `objectVersion`
/// 77): the `App/` folder is referenced as a whole, so new Swift files
/// dropped into it are picked up automatically — no `PBXFileReference`/
/// `PBXBuildFile` pair needed per file, unlike the legacy pbxproj format.
/// This exact structure (object IDs included) was hand-verified against a
/// real `xcodebuild build` + Simulator install/launch before being wired
/// into this generator (Phase 24 Step 2 spike). The object IDs are fixed,
/// arbitrary-but-valid UUIDs — reusing the same ones across every generated
/// project is fine; Xcode only requires uniqueness within one project file.
///
/// `Info.plist` is Xcode-synthesized (`GENERATE_INFOPLIST_FILE = YES`) from
/// `INFOPLIST_KEY_*` build settings — no physical file in `App/`, which
/// avoids a known synchronized-group gotcha (a physical `Info.plist` sitting
/// inside the synced folder gets auto-added as a Resources build file,
/// colliding with Xcode's own Info.plist processing). The separate physical
/// `ios/Info.plist` this module also generates is unrelated — that one's
/// for the older Phase 20-22 hand-rolled `rsc run --target ios` harness.
fn ios_pbxproj(name: &str, crate_name: &str, bundle_id: &str) -> String {
    format!(
        r#"// !$*UTF8*$!
{{
	archiveVersion = 1;
	classes = {{
	}};
	objectVersion = 77;
	objects = {{

/* Begin PBXFileReference section */
		29BFDF34219C04D0F45AA3F6 /* App.app */ = {{isa = PBXFileReference; explicitFileType = wrapper.application; includeInIndex = 0; path = App.app; sourceTree = BUILT_PRODUCTS_DIR; }};
/* End PBXFileReference section */

/* Begin PBXFileSystemSynchronizedRootGroup section */
		C4244DA1E534FD12B2AA8792 /* App */ = {{
			isa = PBXFileSystemSynchronizedRootGroup;
			path = App;
			sourceTree = "<group>";
		}};
/* End PBXFileSystemSynchronizedRootGroup section */

/* Begin PBXFrameworksBuildPhase section */
		18358AF99F4385EAEAE2AE69 /* Frameworks */ = {{
			isa = PBXFrameworksBuildPhase;
			buildActionMask = 2147483647;
			files = (
			);
			runOnlyForDeploymentPostprocessing = 0;
		}};
/* End PBXFrameworksBuildPhase section */

/* Begin PBXGroup section */
		0288BF871AE36CD31BACA868 = {{
			isa = PBXGroup;
			children = (
				C4244DA1E534FD12B2AA8792 /* App */,
				7B07203CC32D538525D03AB2 /* Products */,
			);
			sourceTree = "<group>";
		}};
		7B07203CC32D538525D03AB2 /* Products */ = {{
			isa = PBXGroup;
			children = (
				29BFDF34219C04D0F45AA3F6 /* App.app */,
			);
			name = Products;
			sourceTree = "<group>";
		}};
/* End PBXGroup section */

/* Begin PBXNativeTarget section */
		BBF304700AF99D5D6743CB19 /* App */ = {{
			isa = PBXNativeTarget;
			buildConfigurationList = 9068ED515DA53881152A8216 /* Build configuration list for PBXNativeTarget "App" */;
			buildPhases = (
				6B049AD5F403D6738C4179CB /* Cargo build */,
				0AB57D7A7CA7BD513A06F45C /* Sources */,
				18358AF99F4385EAEAE2AE69 /* Frameworks */,
				4AAFAB8ED1C2446FEC68F01D /* Resources */,
			);
			buildRules = (
			);
			dependencies = (
			);
			fileSystemSynchronizedGroups = (
				C4244DA1E534FD12B2AA8792 /* App */,
			);
			name = App;
			productName = App;
			productReference = 29BFDF34219C04D0F45AA3F6 /* App.app */;
			productType = "com.apple.product-type.application";
		}};
/* End PBXNativeTarget section */

/* Begin PBXProject section */
		DFD2A2909E5B73AF4363D6DC /* Project object */ = {{
			isa = PBXProject;
			attributes = {{
				BuildIndependentTargetsInParallel = 1;
				LastSwiftUpdateCheck = 2600;
				LastUpgradeCheck = 2600;
			}};
			buildConfigurationList = 39D24EFF2CC64443C7C4B0DE /* Build configuration list for PBXProject "App" */;
			developmentRegion = en;
			hasScannedForEncodings = 0;
			knownRegions = (
				en,
				Base,
			);
			mainGroup = 0288BF871AE36CD31BACA868;
			minimizedProjectReferenceProxies = 1;
			preferredProjectObjectVersion = 77;
			productRefGroup = 7B07203CC32D538525D03AB2 /* Products */;
			projectDirPath = "";
			projectRoot = "";
			targets = (
				BBF304700AF99D5D6743CB19 /* App */,
			);
		}};
/* End PBXProject section */

/* Begin PBXResourcesBuildPhase section */
		4AAFAB8ED1C2446FEC68F01D /* Resources */ = {{
			isa = PBXResourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (
			);
			runOnlyForDeploymentPostprocessing = 0;
		}};
/* End PBXResourcesBuildPhase section */

/* Begin PBXShellScriptBuildPhase section */
		6B049AD5F403D6738C4179CB /* Cargo build */ = {{
			isa = PBXShellScriptBuildPhase;
			buildActionMask = 2147483647;
			files = (
			);
			inputFileListPaths = (
			);
			inputPaths = (
			);
			name = "Cargo build";
			outputFileListPaths = (
			);
			outputPaths = (
			);
			runOnlyForDeploymentPostprocessing = 0;
			shellPath = /bin/sh;
			shellScript = "{shell_script}";
		}};
/* End PBXShellScriptBuildPhase section */

/* Begin PBXSourcesBuildPhase section */
		0AB57D7A7CA7BD513A06F45C /* Sources */ = {{
			isa = PBXSourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (
			);
			runOnlyForDeploymentPostprocessing = 0;
		}};
/* End PBXSourcesBuildPhase section */

/* Begin XCBuildConfiguration section */
		FA383BE9511F68518F832D13 /* Debug */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ALWAYS_SEARCH_USER_PATHS = NO;
				CLANG_ENABLE_MODULES = YES;
				CLANG_ENABLE_OBJC_ARC = YES;
				ENABLE_STRICT_OBJC_MSGSEND = YES;
				GCC_NO_COMMON_BLOCKS = YES;
				IPHONEOS_DEPLOYMENT_TARGET = 17.0;
				MTL_ENABLE_DEBUG_INFO = YES;
				ONLY_ACTIVE_ARCH = YES;
				SDKROOT = iphoneos;
				SWIFT_OPTIMIZATION_LEVEL = "-Onone";
				SWIFT_VERSION = 5.0;
			}};
			name = Debug;
		}};
		BB159B7A2910F0E6F7E4A340 /* Release */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ALWAYS_SEARCH_USER_PATHS = NO;
				CLANG_ENABLE_MODULES = YES;
				CLANG_ENABLE_OBJC_ARC = YES;
				ENABLE_STRICT_OBJC_MSGSEND = YES;
				GCC_NO_COMMON_BLOCKS = YES;
				IPHONEOS_DEPLOYMENT_TARGET = 17.0;
				MTL_ENABLE_DEBUG_INFO = NO;
				SDKROOT = iphoneos;
				SWIFT_COMPILATION_MODE = wholemodule;
				SWIFT_VERSION = 5.0;
				VALIDATE_PRODUCT = YES;
			}};
			name = Release;
		}};
		96CE5891704495CBFFF00165 /* Debug */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon;
				ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME = AccentColor;
				CODE_SIGN_STYLE = Automatic;
				CURRENT_PROJECT_VERSION = 1;
				GENERATE_INFOPLIST_FILE = YES;
				INFOPLIST_KEY_CFBundleDisplayName = "{name}";
				INFOPLIST_KEY_UIApplicationSceneManifest_Generation = YES;
				INFOPLIST_KEY_UILaunchScreen_Generation = YES;
				INFOPLIST_KEY_UISupportedInterfaceOrientations = UIInterfaceOrientationPortrait;
				LD_RUNPATH_SEARCH_PATHS = (
					"$(inherited)",
					"@executable_path/Frameworks",
				);
				MARKETING_VERSION = 0.1;
				OTHER_LDFLAGS = "{other_ldflags}";
				PRODUCT_BUNDLE_IDENTIFIER = "{bundle_id}";
				PRODUCT_NAME = "$(TARGET_NAME)";
				SWIFT_EMIT_LOC_STRINGS = YES;
				TARGETED_DEVICE_FAMILY = 1;
			}};
			name = Debug;
		}};
		AF81F49717B17FAB5B04BDC5 /* Release */ = {{
			isa = XCBuildConfiguration;
			buildSettings = {{
				ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon;
				ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME = AccentColor;
				CODE_SIGN_STYLE = Automatic;
				CURRENT_PROJECT_VERSION = 1;
				GENERATE_INFOPLIST_FILE = YES;
				INFOPLIST_KEY_CFBundleDisplayName = "{name}";
				INFOPLIST_KEY_UIApplicationSceneManifest_Generation = YES;
				INFOPLIST_KEY_UILaunchScreen_Generation = YES;
				INFOPLIST_KEY_UISupportedInterfaceOrientations = UIInterfaceOrientationPortrait;
				LD_RUNPATH_SEARCH_PATHS = (
					"$(inherited)",
					"@executable_path/Frameworks",
				);
				MARKETING_VERSION = 0.1;
				OTHER_LDFLAGS = "{other_ldflags}";
				PRODUCT_BUNDLE_IDENTIFIER = "{bundle_id}";
				PRODUCT_NAME = "$(TARGET_NAME)";
				SWIFT_EMIT_LOC_STRINGS = YES;
				TARGETED_DEVICE_FAMILY = 1;
			}};
			name = Release;
		}};
/* End XCBuildConfiguration section */

/* Begin XCConfigurationList section */
		39D24EFF2CC64443C7C4B0DE /* Build configuration list for PBXProject "App" */ = {{
			isa = XCConfigurationList;
			buildConfigurations = (
				FA383BE9511F68518F832D13 /* Debug */,
				BB159B7A2910F0E6F7E4A340 /* Release */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Release;
		}};
		9068ED515DA53881152A8216 /* Build configuration list for PBXNativeTarget "App" */ = {{
			isa = XCConfigurationList;
			buildConfigurations = (
				96CE5891704495CBFFF00165 /* Debug */,
				AF81F49717B17FAB5B04BDC5 /* Release */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Release;
		}};
/* End XCConfigurationList section */
	}};
	rootObject = DFD2A2909E5B73AF4363D6DC /* Project object */;
}}
"#,
        shell_script = ios_cargo_build_script(crate_name),
        other_ldflags = ios_other_ldflags(crate_name),
        name = name,
        bundle_id = bundle_id,
    )
}

/// The Run Script build phase that produces the Rust staticlib before
/// Xcode compiles/links the Swift sources — picks the Rust target triple
/// from Xcode's own `$PLATFORM_NAME`, matching the triple already proven
/// in Phase 24 Step 1 (`aarch64-apple-ios-sim` for the Simulator on this
/// architecture). `cargo`'s rustup shim isn't always on the minimal PATH
/// Xcode runs script phases with, so `~/.cargo/bin` is prepended
/// defensively. The `.a` is copied to `$BUILT_PRODUCTS_DIR`, which Xcode
/// already searches by default — no explicit `LIBRARY_SEARCH_PATHS` needed.
fn ios_cargo_build_script(crate_name: &str) -> String {
    let script = format!(
        r#"set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "${{SRCROOT}}/.."
case "${{PLATFORM_NAME}}" in
  iphonesimulator) RUST_TARGET=aarch64-apple-ios-sim ;;
  iphoneos) RUST_TARGET=aarch64-apple-ios ;;
  *) echo "error: unsupported PLATFORM_NAME ${{PLATFORM_NAME}}" >&2; exit 1 ;;
esac
if [ "${{CONFIGURATION}}" = "Release" ]; then
  CARGO_PROFILE_DIR=release
  cargo build --lib --release --target "${{RUST_TARGET}}"
else
  CARGO_PROFILE_DIR=debug
  cargo build --lib --target "${{RUST_TARGET}}"
fi
cp "target/${{RUST_TARGET}}/${{CARGO_PROFILE_DIR}}/lib{crate_name}.a" "${{BUILT_PRODUCTS_DIR}}/lib{crate_name}.a"
"#
    );
    pbxproj_escape_script(&script)
}

/// Escapes a shell script for embedding as a `.pbxproj` string literal
/// (OpenStep plist syntax: `\"` for quotes, `\n` for newlines).
fn pbxproj_escape_script(script: &str) -> String {
    script.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

/// `-l{crate_name}` (the staticlib the Cargo build script phase produces)
/// plus the system frameworks the Rust engine needs — empirically
/// determined in Phase 24 Step 1's Simulator verification (linking the
/// throwaway Swift stub against `rosace-ffi`'s compiled staticlib).
fn ios_other_ldflags(crate_name: &str) -> String {
    format!(
        "-l{crate_name} -framework UIKit -framework QuartzCore -framework Metal \
         -framework Foundation -framework CoreGraphics -framework Security -framework CoreFoundation"
    )
}

/// Shared Xcode scheme — without this, `xcodebuild -scheme App` can't find
/// a scheme headlessly (Xcode normally auto-creates one on first GUI open,
/// which doesn't happen in a `rsc new`/CI context). Matters for Phase 24
/// Step 4 (`rsc run --target ios` driving `xcodebuild`) too, not just manual use.
fn ios_xcscheme() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<Scheme
   LastUpgradeVersion = "2600"
   version = "1.7">
   <BuildAction
      parallelizeBuildables = "YES"
      buildImplicitDependencies = "YES">
      <BuildActionEntries>
         <BuildActionEntry
            buildForTesting = "YES"
            buildForRunning = "YES"
            buildForProfiling = "YES"
            buildForArchiving = "YES"
            buildForAnalyzing = "YES">
            <BuildableReference
               BuildableIdentifier = "primary"
               BlueprintIdentifier = "BBF304700AF99D5D6743CB19"
               BuildableName = "App.app"
               BlueprintName = "App"
               ReferencedContainer = "container:App.xcodeproj">
            </BuildableReference>
         </BuildActionEntry>
      </BuildActionEntries>
   </BuildAction>
   <LaunchAction
      buildConfiguration = "Debug">
      <BuildableProductRunnable runnableDebuggingMode = "0">
         <BuildableReference
            BuildableIdentifier = "primary"
            BlueprintIdentifier = "BBF304700AF99D5D6743CB19"
            BuildableName = "App.app"
            BlueprintName = "App"
            ReferencedContainer = "container:App.xcodeproj">
         </BuildableReference>
      </BuildableProductRunnable>
   </LaunchAction>
</Scheme>
"#
    .to_string()
}

/// Per-app FFI glue (D106 Phase 24 Step 1/2) — the ~15-line shim that
/// exports the `rsc_engine_*` C symbols the native host links against,
/// instantiating the app's own `AppRoot` (the SAME root component
/// desktop/web already drive). Mirrors `rosace-ffi/examples/ios_stub.rs`,
/// the reference pattern this template is generated from.
fn ffi_rs(bundle_id: &str) -> String {
    let jni_prefix = jni_class_prefix(bundle_id);

    let header = r#"//! Native-host FFI glue (D106 Phase 24) — exports the ABI
//! `ios/App/EngineViewController.swift` and `android/.../MainActivity.kt`
//! call into. iOS uses the plain C ABI in `rosace-ffi`'s
//! `include/rsc_engine.h` (pattern: `rosace-ffi/examples/ios_stub.rs`).
//! Android uses JNI instead — Kotlin's `external fun` resolves to a symbol
//! literally named `Java_<package>_<Class>_<method>` (JNI's mangling: `.` ->
//! `_`, a literal `_` -> `_1` — see `jni_class_prefix` in
//! `rosace-cli/src/commands/new.rs`, which computed the exact prefix below
//! from this app's bundle id at `rsc new` time). Pattern:
//! `rosace-ffi/examples/android_stub.rs`.

use std::os::raw::c_void;
#[cfg(target_os = "ios")]
use std::ptr::NonNull;

#[cfg(any(target_os = "ios", target_os = "android"))]
use rosace::prelude::*;
use rosace_ffi::{Engine, RscInputEventFfi};
#[cfg(target_os = "ios")]
use rosace_ffi::RawSurface;
#[cfg(target_os = "android")]
use rosace_ffi::AndroidSurfaceHandle;

#[cfg(any(target_os = "ios", target_os = "android"))]
use crate::app::AppRoot;

// -- iOS: plain C ABI --------------------------------------------------------

/// # Safety
/// `surface_handle` must be a valid, non-null `CAMetalLayer`-backed
/// `UIView*` for the engine's lifetime.
#[cfg(target_os = "ios")]
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_init(
    surface_handle: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut Engine {
    let Some(handle) = NonNull::new(surface_handle) else { return std::ptr::null_mut() };
    let surface = unsafe { RawSurface::from_ca_metal_layer(handle, None, width, height, scale) };
    let theme = light_theme();
    // Mobile bypasses lib.rs's launch() entirely — app_init() must be
    // called explicitly here too, or one-time app setup silently never
    // runs on iOS (see app_init's doc in lib.rs for why).
    crate::app_init();
    match Engine::init(Box::new(AppRoot), theme, surface) {
        Some(engine) => Box::into_raw(engine),
        None => std::ptr::null_mut(),
    }
}

#[cfg(not(target_os = "ios"))]
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_init(
    _surface_handle: *mut c_void,
    _width: u32,
    _height: u32,
    _scale: f32,
) -> *mut Engine {
    std::ptr::null_mut()
}

/// # Safety
/// `engine` must be a live pointer previously returned by `rsc_engine_init`
/// (or null, which is a no-op).
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_resize(
    engine: *mut Engine,
    width: u32,
    height: u32,
    scale: f32,
    safe_top: f32,
    safe_right: f32,
    safe_bottom: f32,
    safe_left: f32,
) {
    if engine.is_null() { return; }
    let safe_area = rosace::core::SafeArea { top: safe_top, right: safe_right, bottom: safe_bottom, left: safe_left };
    unsafe { (*engine).resize(width, height, scale, safe_area) };
}

/// # Safety
/// `engine` must be a live pointer previously returned by `rsc_engine_init`
/// (or null, which is a no-op). Called by the native host whenever the OS
/// reports an appearance/accessibility change.
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_set_media_query(
    engine: *mut Engine,
    is_dark: u8,
    text_scale: f32,
    bold_text: u8,
    reduce_motion: u8,
    always_24_hour_format: u8,
) {
    if engine.is_null() { return; }
    let mq = rosace::core::MediaQuery {
        text_scale,
        is_dark: is_dark != 0,
        bold_text: bold_text != 0,
        reduce_motion: reduce_motion != 0,
        always_24_hour_format: always_24_hour_format != 0,
    };
    unsafe { (*engine).set_media_query(mq) };
}

/// # Safety
/// `engine` must be a live pointer from `rsc_engine_init`; `events` must
/// point to at least `count` valid `RscInputEvent`s.
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_input(
    engine: *mut Engine,
    events: *const RscInputEventFfi,
    count: usize,
) {
    if engine.is_null() || events.is_null() { return; }
    let slice = unsafe { std::slice::from_raw_parts(events, count) };
    unsafe { (*engine).input(slice) };
}

/// # Safety
/// `engine` must be a live pointer from `rsc_engine_init` (or null).
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_frame(engine: *mut Engine) {
    if engine.is_null() { return; }
    unsafe { (*engine).frame() };
}

/// # Safety
/// `engine` must be a pointer previously returned by `rsc_engine_init` and
/// not yet passed to this function; it must not be used again afterward.
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_shutdown(engine: *mut Engine) {
    if engine.is_null() { return; }
    drop(unsafe { Box::from_raw(engine) });
}

// -- Push notifications (D110 Phase 29 Step 2) --------------------------------
// Discovery ("is a permission request pending?") goes through the generic
// Platform Channel poll below, not a dedicated take_request — see D127 and
// rosace_ffi::capability's module doc. Result-reporting stays a plain
// setter (no call_id correlation needed for a singleton capability).

#[no_mangle]
pub extern "C" fn rsc_push_permission_report_result(granted: u8) {
    rosace_ffi::report_push_result(granted != 0);
}

/// # Safety
/// `token` must be a valid NUL-terminated C string or null (a no-op).
#[no_mangle]
pub unsafe extern "C" fn rsc_push_report_token(token: *const std::os::raw::c_char) {
    if token.is_null() { return; }
    let token = unsafe { std::ffi::CStr::from_ptr(token) }.to_string_lossy().into_owned();
    rosace_ffi::report_push_token(token);
}

/// # Safety
/// Each argument must be a valid NUL-terminated C string or null (null
/// reads as the empty string; the call still delivers).
#[no_mangle]
pub unsafe extern "C" fn rsc_push_report_notification(
    title: *const std::os::raw::c_char,
    body: *const std::os::raw::c_char,
    payload_json: *const std::os::raw::c_char,
) {
    let read = |p: *const std::os::raw::c_char| -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    rosace_ffi::report_push_notification(read(title), read(body), read(payload_json));
}

// -- Platform Channel (D127) ---------------------------------------------------
// The generic bidirectional method-call bridge to native code — named
// channels + methods + JSON payloads, instead of a bespoke FFI function per
// platform feature. Two directions, four exports:
//   - Rust calls native, async: `rsc_platform_channel_take_outgoing` (the
//     host's ONE per-frame poll — this is what push permission discovery
//     above now goes through, alongside anything an app registers itself)
//     + `rsc_platform_channel_report_result`/`_report_error` (the host
//     answers once its native-side work finishes, which may be many frames
//     later — a system dialog, a slow SDK call).
//   - Native calls Rust, sync: `rsc_platform_channel_dispatch` — one
//     blocking call, answered inline by whatever handler the app registered
//     via `rosace_ffi::set_method_call_handler`. For fast work only.
// `rsc_string_free` pairs with every owned string this crate returns across
// the boundary (`take_outgoing`'s JSON array, `dispatch`'s JSON result) —
// the receiver must call it exactly once after copying the bytes into its
// own native string, same discipline `AndroidSurfaceHandle`'s `Drop`
// already follows for the native-window reference.

/// # Safety
/// The returned pointer is an owned, NUL-terminated JSON string (a `[]`
/// array of `{call_id, channel, method, args}` objects) that the caller
/// MUST pass to `rsc_string_free` exactly once when done reading it.
#[no_mangle]
pub extern "C" fn rsc_platform_channel_take_outgoing() -> *mut std::os::raw::c_char {
    let calls: Vec<serde_json::Value> = rosace_ffi::take_outgoing_calls()
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "call_id": c.call_id,
                "channel": c.channel,
                "method": c.method,
                "args": serde_json::from_str::<serde_json::Value>(&c.args_json)
                    .unwrap_or(serde_json::Value::Null),
            })
        })
        .collect();
    let text = serde_json::Value::Array(calls).to_string();
    std::ffi::CString::new(text).unwrap_or_default().into_raw()
}

/// The current accessibility tree as JSON (D132) — what the native host
/// republishes to VoiceOver (iOS) / TalkBack (Android).
///
/// PULL, not push: both mobile a11y APIs are demand-driven, so the host
/// calls this only while assistive tech is actually inspecting. An app with
/// no screen reader running never pays for it.
///
/// `bounds` are LOGICAL pixels, window-relative — each host converts to its
/// own convention (iOS screen-space `CGRect`, Android physical-pixel `Rect`).
///
/// # Safety
/// `engine` must be a live pointer from `rsc_engine_init`. The returned
/// pointer is an owned, NUL-terminated JSON string the caller MUST pass to
/// `rsc_string_free` exactly once.
#[no_mangle]
pub unsafe extern "C" fn rsc_engine_semantics_json(
    engine: *mut Engine,
) -> *mut std::os::raw::c_char {
    if engine.is_null() {
        return std::ptr::null_mut();
    }
    let engine = unsafe { &*engine };
    let text = rosace_ffi::semantics_json(engine);
    std::ffi::CString::new(text).unwrap_or_default().into_raw()
}

/// Frees a string previously returned by `rsc_platform_channel_take_outgoing`,
/// `rsc_platform_channel_dispatch`, or `rsc_engine_semantics_json`.
///
/// # Safety
/// `ptr` must be either null (a no-op) or a pointer this crate returned
/// across the FFI boundary, not yet freed.
#[no_mangle]
pub unsafe extern "C" fn rsc_string_free(ptr: *mut std::os::raw::c_char) {
    if ptr.is_null() { return; }
    drop(unsafe { std::ffi::CString::from_raw(ptr) });
}

/// # Safety
/// `result_json` must be a valid NUL-terminated C string or null (a no-op).
#[no_mangle]
pub unsafe extern "C" fn rsc_platform_channel_report_result(
    call_id: u64,
    result_json: *const std::os::raw::c_char,
) {
    if result_json.is_null() { return; }
    let json = unsafe { std::ffi::CStr::from_ptr(result_json) }.to_string_lossy();
    rosace_ffi::report_call_result(call_id, &json);
}

/// # Safety
/// `message` must be a valid NUL-terminated C string or null (a no-op).
#[no_mangle]
pub unsafe extern "C" fn rsc_platform_channel_report_error(
    call_id: u64,
    message: *const std::os::raw::c_char,
) {
    if message.is_null() { return; }
    let msg = unsafe { std::ffi::CStr::from_ptr(message) }.to_string_lossy().into_owned();
    rosace_ffi::report_call_error(call_id, msg);
}

/// # Safety
/// Each argument must be a valid NUL-terminated C string or null (null
/// reads as the empty string). The returned pointer is owned — see the
/// module doc's note on `rsc_string_free`.
#[no_mangle]
pub unsafe extern "C" fn rsc_platform_channel_dispatch(
    channel: *const std::os::raw::c_char,
    method: *const std::os::raw::c_char,
    args_json: *const std::os::raw::c_char,
) -> *mut std::os::raw::c_char {
    let read = |p: *const std::os::raw::c_char| -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    let result = rosace_ffi::dispatch_call(&read(channel), &read(method), &read(args_json));
    std::ffi::CString::new(result).unwrap_or_default().into_raw()
}

// -- Soft-keyboard sync (D116 Step 6) -----------------------------------------
// Shared, platform-agnostic (like the push functions above): a native host
// polls these once per frame tick to know whether to show/hide its OS soft
// keyboard and which layout to use — iOS via `@_silgen_name`, Android through
// the JNI wrappers below. No engine handle needed; these read the same
// process-global focus signal `ime_cursor_area`/`keyboard_type` already use
// for desktop's real OS IME.

#[no_mangle]
pub extern "C" fn rsc_text_input_active() -> u8 {
    rosace_ffi::text_input_active() as u8
}

#[no_mangle]
pub extern "C" fn rsc_focused_keyboard_type() -> u32 {
    rosace_ffi::focused_keyboard_type()
}

// -- Android: JNI -------------------------------------------------------------
// Symbol names are burned in at codegen time (JNI resolves by exact name,
// no runtime registration) — see the module doc above for why this can't be
// the same plain-C functions iOS uses. `AndroidEngine` keeps the `Engine`
// and the `AndroidSurfaceHandle` (whose `Drop` releases the `ANativeWindow`
// reference) alive together, torn down as a unit in nativeShutdown — same
// reasoning as `rosace-ffi/examples/android_stub.rs`'s `AndroidEngine`.

#[cfg(target_os = "android")]
struct AndroidEngine {
    engine: Box<Engine>,
    #[allow(dead_code)]
    surface: AndroidSurfaceHandle,
}

/// NEVER called — winit's android-native-activity backend references this
/// symbol from its NativeActivity glue (rosace-platform compiles winit for
/// android so the shared types typecheck; see its Cargo.toml note), and
/// without a definition the final cdylib carries an undefined symbol that
/// makes `System.loadLibrary` fail with `UnsatisfiedLinkError` at app
/// startup. The D106 host drives the app entirely via the JNI functions
/// above; winit's own Android entry path is deliberately unused.
#[cfg(target_os = "android")]
#[no_mangle]
extern "C" fn android_main(_app: *mut std::ffi::c_void) {
    unreachable!("NativeActivity entry is unused — the JNI host owns the app (D106)");
}
"#;

    let android = format!(
        r#"
// Android discards a process's stderr, so a Rust panic normally vanishes
// with no trace in `adb logcat` — the app just dies. Route panics to logcat
// (`liblog`) as FATAL so they're visible. Installed once, idempotently, from
// nativeInit before any engine work runs.
#[cfg(target_os = "android")]
#[link(name = "log")]
extern "C" {{
    fn __android_log_write(
        prio: std::os::raw::c_int,
        tag: *const std::os::raw::c_char,
        text: *const std::os::raw::c_char,
    ) -> std::os::raw::c_int;
}}

#[cfg(target_os = "android")]
fn install_panic_logcat() {{
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {{
        std::panic::set_hook(Box::new(|info| {{
            let tag = std::ffi::CString::new("rosace").unwrap();
            let text = std::ffi::CString::new(format!("{{info}}"))
                .unwrap_or_else(|_| std::ffi::CString::new("panic (unprintable message)").unwrap());
            // ANDROID_LOG_FATAL = 7
            unsafe {{ __android_log_write(7, tag.as_ptr(), text.as_ptr()); }}
        }}));
    }});
}}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeInit(
    env: jni::JNIEnv,
    _class: jni::objects::JObject,
    surface: jni::objects::JObject,
    width: jni::sys::jint,
    height: jni::sys::jint,
    scale: jni::sys::jfloat,
) -> jni::sys::jlong {{
    install_panic_logcat();
    let raw_env = env.get_raw();
    let Some(handle) = (unsafe {{ AndroidSurfaceHandle::from_jni(raw_env, &surface) }}) else {{
        return 0;
    }};
    let raw_surface = unsafe {{ handle.raw_surface(width as u32, height as u32, scale) }};
    let theme = light_theme();
    // Mobile bypasses lib.rs's launch() entirely — app_init() must be
    // called explicitly here too, or one-time app setup silently never
    // runs on Android (see app_init's doc in lib.rs for why).
    crate::app_init();
    match Engine::init(Box::new(AppRoot), theme, raw_surface) {{
        Some(engine) => Box::into_raw(Box::new(AndroidEngine {{ engine, surface: handle }})) as jni::sys::jlong,
        None => 0,
    }}
}}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeResize(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
    width: jni::sys::jint,
    height: jni::sys::jint,
    scale: jni::sys::jfloat,
    safe_top: jni::sys::jfloat,
    safe_right: jni::sys::jfloat,
    safe_bottom: jni::sys::jfloat,
    safe_left: jni::sys::jfloat,
) {{
    if handle == 0 {{ return; }}
    let ptr = handle as *mut AndroidEngine;
    let safe_area = rosace::core::SafeArea {{ top: safe_top, right: safe_right, bottom: safe_bottom, left: safe_left }};
    unsafe {{ (*ptr).engine.resize(width as u32, height as u32, scale, safe_area) }};
}}

/// Called once from `nativeInit` and again from every
/// `onConfigurationChanged` (uiMode/fontScale changes) — see `MainActivity.kt`.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeSetMediaQuery(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
    is_dark: jni::sys::jboolean,
    text_scale: jni::sys::jfloat,
    bold_text: jni::sys::jboolean,
    reduce_motion: jni::sys::jboolean,
    always_24_hour_format: jni::sys::jboolean,
) {{
    if handle == 0 {{ return; }}
    let ptr = handle as *mut AndroidEngine;
    let mq = rosace::core::MediaQuery {{
        text_scale,
        is_dark: is_dark != 0,
        bold_text: bold_text != 0,
        reduce_motion: reduce_motion != 0,
        always_24_hour_format: always_24_hour_format != 0,
    }};
    unsafe {{ (*ptr).engine.set_media_query(mq) }};
}}

/// One touch/pointer event per call — `kind` is `0` = move, `1` = down,
/// `2` = up (matching `rosace_ffi`'s `RSC_EVENT_MOUSE_*` constants); a
/// touch is always reported as the left button, mirroring how the existing
/// winit `Touch` handling already treats touch input (see `rosace-ffi`'s
/// `event.rs` module doc).
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeTouch(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
    kind: jni::sys::jint,
    x: jni::sys::jfloat,
    y: jni::sys::jfloat,
) {{
    if handle == 0 {{ return; }}
    let ptr = handle as *mut AndroidEngine;
    let event = RscInputEventFfi {{
        kind: kind as u32, x, y, button: 0, key: 0, character: 0,
        width: 0, height: 0, delta_x: 0.0, delta_y: 0.0,
    }};
    unsafe {{ (*ptr).engine.input(&[event]) }};
}}

/// One key event per call — `key` is an `RSC_KEY_*` constant (matching
/// `rosace_ffi::event`'s desktop/iOS key encoding); `kind` 3 = KeyDown (see
/// `rosace_ffi::event::RSC_EVENT_KEY_DOWN` — not re-exported, so burned in as
/// a literal here, same as `nativeLifecycle`'s kinds below). Used for
/// Backspace, Enter, and Tab, which the engine's editor treats as commands
/// rather than literal text (see `nativeText` below for typed characters).
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeKey(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
    key: jni::sys::jint,
) {{
    if handle == 0 {{ return; }}
    let ptr = handle as *mut AndroidEngine;
    let event = RscInputEventFfi {{
        kind: 3, x: 0.0, y: 0.0, button: 0,
        key: key as u32, character: 0, width: 0, height: 0, delta_x: 0.0, delta_y: 0.0,
    }};
    unsafe {{ (*ptr).engine.input(&[event]) }};
}}

/// One typed Unicode scalar per call — `kind` 5 = Text (`RSC_EVENT_TEXT`).
/// The IME's `commitText` forwards each character here (mirroring iOS's
/// `UIKeyInput.insertText`); Enter/Tab are sent through `nativeKey` instead,
/// never as text (see the Kotlin `InputConnection` this backs for why).
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeText(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
    character: jni::sys::jint,
) {{
    if handle == 0 {{ return; }}
    let ptr = handle as *mut AndroidEngine;
    let event = RscInputEventFfi {{
        kind: 5, x: 0.0, y: 0.0, button: 0,
        key: 0, character: character as u32, width: 0, height: 0, delta_x: 0.0, delta_y: 0.0,
    }};
    unsafe {{ (*ptr).engine.input(&[event]) }};
}}

/// Whether a text field is currently focused (D116 Step 6) — polled once per
/// frame tick (mirroring iOS's `rsc_text_input_active`) to decide whether to
/// show/hide the soft keyboard. No handle needed: same process-global focus
/// signal desktop's real OS IME already uses.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeTextInputActive(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
) -> jni::sys::jboolean {{
    rosace_ffi::text_input_active() as jni::sys::jboolean
}}

/// The focused field's keyboard-type hint, an `RSC_KEYBOARD_*` constant
/// (mirroring iOS's `rsc_focused_keyboard_type`) — used to pick the IME's
/// `inputType` (email/numeric/URL/phone/default).
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeFocusedKeyboardType(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
) -> jni::sys::jint {{
    rosace_ffi::focused_keyboard_type() as jni::sys::jint
}}

// -- Platform Channel (D127) — JNI wrappers around the same rosace_ffi
// primitives the iOS plain-C exports above use. JNI strings are JVM-managed
// (`env.new_string`/`get_string`), unlike iOS's C strings — no
// `rsc_string_free` equivalent is needed here; the JVM garbage-collects
// `JString`s normally.

/// The engine's accessibility tree as JSON (D132) — what the Kotlin side
/// turns into `AccessibilityNodeInfo`s for TalkBack.
///
/// PULL, not push: `AccessibilityNodeProvider` is called only while an
/// accessibility service is actually exploring, so an app with TalkBack off
/// never serializes anything. Bounds are LOGICAL, view-relative pixels; the
/// host multiplies by density for `AccessibilityNodeInfo`'s physical-pixel
/// `Rect`.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeSemanticsJson(
    env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
) -> jni::sys::jstring {{
    if handle == 0 {{
        return std::ptr::null_mut();
    }}
    // The handle is an `AndroidEngine` (the surface-owning wrapper), NOT a
    // bare `Engine` — every other JNI fn here casts it that way. Casting to
    // `Engine` directly read from a bogus offset and segfaulted at 0x10 the
    // moment TalkBack first queried the tree.
    let ptr = handle as *mut AndroidEngine;
    let text = rosace_ffi::semantics_json(unsafe {{ &(*ptr).engine }});
    env.new_string(text).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}}

/// The host's ONE per-frame poll (alongside `nativeFrame`) — drains every
/// queued Platform Channel call (push-permission discovery included) as a
/// JSON array of `{{call_id, channel, method, args}}` objects.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeTakeOutgoingPlatformCalls(
    env: jni::JNIEnv,
    _class: jni::objects::JObject,
) -> jni::sys::jstring {{
    let calls: Vec<serde_json::Value> = rosace_ffi::take_outgoing_calls()
        .into_iter()
        .map(|c| {{
            serde_json::json!({{
                "call_id": c.call_id,
                "channel": c.channel,
                "method": c.method,
                "args": serde_json::from_str::<serde_json::Value>(&c.args_json)
                    .unwrap_or(serde_json::Value::Null),
            }})
        }})
        .collect();
    let text = serde_json::Value::Array(calls).to_string();
    env.new_string(text).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}}

/// Called once `call_id`'s native-side work finishes successfully.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativePlatformChannelReportResult(
    mut env: jni::JNIEnv,
    _class: jni::objects::JObject,
    call_id: jni::sys::jlong,
    result_json: jni::objects::JString,
) {{
    let json: String = env.get_string(&result_json).map(String::from).unwrap_or_default();
    rosace_ffi::report_call_result(call_id as u64, &json);
}}

/// Called when `call_id`'s native-side work fails.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativePlatformChannelReportError(
    mut env: jni::JNIEnv,
    _class: jni::objects::JObject,
    call_id: jni::sys::jlong,
    message: jni::objects::JString,
) {{
    let msg: String = env.get_string(&message).map(String::from).unwrap_or_default();
    rosace_ffi::report_call_error(call_id as u64, msg);
}}

/// Native calls INTO Rust, synchronously — one blocking call answered
/// inline by whatever handler the app registered via
/// `rosace_ffi::set_method_call_handler`. For fast work only.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativePlatformChannelDispatch(
    mut env: jni::JNIEnv,
    _class: jni::objects::JObject,
    channel: jni::objects::JString,
    method: jni::objects::JString,
    args_json: jni::objects::JString,
) -> jni::sys::jstring {{
    let channel: String = env.get_string(&channel).map(String::from).unwrap_or_default();
    let method: String = env.get_string(&method).map(String::from).unwrap_or_default();
    let args: String = env.get_string(&args_json).map(String::from).unwrap_or_default();
    let result = rosace_ffi::dispatch_call(&channel, &method, &args);
    env.new_string(result).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}}

/// One app-lifecycle transition per call (D110 Phase 29 Step 1) — `kind`
/// is a `RSC_EVENT_LIFECYCLE_*` constant (8 = active, 9 = inactive,
/// 10 = background). `Engine::input` applies lifecycle immediately (see
/// its doc), so calling this from `onStop` — after the Choreographer
/// callback has gone quiet — still takes effect right away.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeLifecycle(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
    kind: jni::sys::jint,
) {{
    if handle == 0 {{ return; }}
    let ptr = handle as *mut AndroidEngine;
    let event = RscInputEventFfi {{
        kind: kind as u32, x: 0.0, y: 0.0, button: 0, key: 0, character: 0,
        width: 0, height: 0, delta_x: 0.0, delta_y: 0.0,
    }};
    unsafe {{ (*ptr).engine.input(&[event]) }};
}}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeFrame(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
) {{
    if handle == 0 {{ return; }}
    let ptr = handle as *mut AndroidEngine;
    unsafe {{ (*ptr).engine.frame() }};
}}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_{jni_prefix}_nativeShutdown(
    _env: jni::JNIEnv,
    _class: jni::objects::JObject,
    handle: jni::sys::jlong,
) {{
    if handle == 0 {{ return; }}
    drop(unsafe {{ Box::from_raw(handle as *mut AndroidEngine) }});
}}
"#
    );

    format!("{header}{android}")
}

/// Java package derived from a bundle id: lowercased, `-` -> `_` (Java
/// packages can't contain hyphens); dots stay as package separators.
fn android_package(bundle_id: &str) -> String {
    bundle_id.to_lowercase().replace('-', "_")
}

/// JNI method-name mangling (JNI spec, "Resolving Native Method Names"):
/// `.` (package separator) -> `_`, and a literal `_` already in an
/// identifier -> `_1` so it can't be confused with a mangled separator.
/// `;`/`[` (JNI type-signature characters, not needed for the plain
/// overload forms generated here) map to `_2`/`_3` for completeness.
fn jni_mangle(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '.' => out.push('_'),
            '_' => out.push_str("_1"),
            ';' => out.push_str("_2"),
            '[' => out.push_str("_3"),
            c => out.push(c),
        }
    }
    out
}

/// The `Java_<package>_<Class>` prefix a generated `MainActivity.kt`'s
/// `external fun`s resolve to, e.g. `dev.rosace.theme_preview` ->
/// `dev_rosace_theme_1preview_MainActivity`.
fn jni_class_prefix(bundle_id: &str) -> String {
    format!("{}_MainActivity", jni_mangle(&android_package(bundle_id)))
}

#[cfg(test)]
mod ffi_codegen_tests {
    use super::*;

    #[test]
    fn jni_mangle_replaces_dots_with_underscore() {
        assert_eq!(jni_mangle("dev.rosace.myapp"), "dev_rosace_myapp");
    }

    #[test]
    fn jni_mangle_escapes_literal_underscore_as_1() {
        assert_eq!(jni_mangle("dev.rosace.theme_preview"), "dev_rosace_theme_1preview");
    }

    #[test]
    fn android_package_lowercases_and_strips_hyphens() {
        assert_eq!(android_package("Dev.Rosace.My-App"), "dev.rosace.my_app");
    }

    #[test]
    fn jni_class_prefix_matches_real_symbol_shape() {
        assert_eq!(
            jni_class_prefix("dev.rosace.theme_preview"),
            "dev_rosace_theme_1preview_MainActivity"
        );
    }

    #[test]
    fn ffi_rs_embeds_the_derived_jni_prefix() {
        let src = ffi_rs("dev.rosace.myapp");
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativeInit"));
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativeLifecycle"));
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativeFrame"));
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativeShutdown"));
    }

    #[test]
    fn ffi_rs_embeds_the_keyboard_bridge_for_both_platforms() {
        let src = ffi_rs("dev.rosace.myapp");
        // Shared (iOS via @_silgen_name, Android via the JNI wrappers below).
        assert!(src.contains("fn rsc_text_input_active"));
        assert!(src.contains("fn rsc_focused_keyboard_type"));
        // Android JNI keyboard bridge.
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativeKey"));
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativeText"));
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativeTextInputActive"));
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativeFocusedKeyboardType"));
    }

    #[test]
    fn ios_swift_template_has_the_keyboard_bridge() {
        assert!(IOS_ENGINE_VIEW_CONTROLLER_SWIFT.contains("UIKeyInput"));
        assert!(IOS_ENGINE_VIEW_CONTROLLER_SWIFT.contains("rsc_text_input_active"));
        assert!(IOS_ENGINE_VIEW_CONTROLLER_SWIFT.contains("rsc_focused_keyboard_type"));
        assert!(IOS_ENGINE_VIEW_CONTROLLER_SWIFT.contains("syncSoftKeyboard"));
    }

    #[test]
    fn ffi_rs_embeds_the_platform_channel_bridge_for_both_platforms() {
        let src = ffi_rs("dev.rosace.myapp");
        // Shared (iOS via @_silgen_name, Android via the JNI wrappers below).
        assert!(src.contains("fn rsc_platform_channel_take_outgoing"));
        assert!(src.contains("fn rsc_platform_channel_report_result"));
        assert!(src.contains("fn rsc_platform_channel_report_error"));
        assert!(src.contains("fn rsc_platform_channel_dispatch"));
        assert!(src.contains("fn rsc_string_free"));
        // Push permission discovery must go through the generic poll now,
        // not a dedicated take_request.
        assert!(!src.contains("rsc_push_permission_take_request"));
        assert!(src.contains("fn rsc_push_permission_report_result"));
        // Android JNI Platform Channel bridge.
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativeTakeOutgoingPlatformCalls"));
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativePlatformChannelReportResult"));
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativePlatformChannelReportError"));
        assert!(src.contains("Java_dev_rosace_myapp_MainActivity_nativePlatformChannelDispatch"));
    }

    #[test]
    fn lib_rs_defines_app_init_and_calls_it_from_launch() {
        let opts = NewOptions {
            name: "myapp".into(),
            platforms: vec![Platform::Ios, Platform::Android],
            bundle_id: "dev.rosace.myapp".into(),
        };
        let src = lib_rs("myapp", &opts);
        assert!(src.contains("fn app_init()"));
        assert!(src.contains("    app_init();"), "launch() must call app_init() as its first step");
    }

    #[test]
    fn mobile_entry_points_call_app_init_not_just_launch() {
        // Root-caused live (D127): mobile's Engine::init call sites bypass
        // lib.rs's launch() entirely, so a Platform Channel handler (or any
        // one-time setup) registered only in launch() silently never runs
        // on iOS/Android — a real handler call answered "no handler
        // registered" until app_init() was added and called from both
        // mobile entry points too. This guards the fix.
        let src = ffi_rs("dev.rosace.myapp");
        assert_eq!(
            src.matches("crate::app_init();").count(), 2,
            "both the iOS rsc_engine_init and Android nativeInit must call app_init()"
        );
    }

    #[test]
    fn ios_swift_template_polls_platform_channel_instead_of_the_old_dedicated_push_poll() {
        assert!(IOS_ENGINE_VIEW_CONTROLLER_SWIFT.contains("rsc_platform_channel_take_outgoing"));
        assert!(IOS_ENGINE_VIEW_CONTROLLER_SWIFT.contains("pollPlatformChannel"));
        assert!(!IOS_ENGINE_VIEW_CONTROLLER_SWIFT.contains("rsc_push_permission_take_request"));
        // Result-reporting stays a plain setter, unchanged.
        assert!(IOS_ENGINE_VIEW_CONTROLLER_SWIFT.contains("rsc_push_permission_report_result"));
    }
}
