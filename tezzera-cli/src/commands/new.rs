//! `tzr new` — scaffold a new TEZZERA app.
//!
//! Generates a well-structured, multi-file project (not everything in one
//! file): a root component with routing, a theme module, and a `screens/`
//! folder — plus the per-platform boilerplate (`web/index.html`,
//! `ios/Info.plist`, feature-gated `Cargo.toml`) for the platforms the user
//! selects. `tzr run --target <platform>` then builds/runs each without the
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
/// `tezzera_core::Platform` already uses for the same reason (D105).
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
    /// The host OS `tzr` itself is running on, as a `Platform` — used to
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
    /// App bundle/package identifier (e.g. `dev.tezzera.myapp`) — shared by
    /// iOS `CFBundleIdentifier`, the Xcode `PRODUCT_BUNDLE_IDENTIFIER`, and
    /// macOS `Info.plist`. Updatable later via `tzr bundle-id <id>`.
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
            .ok_or_else(|| "usage: tzr new <name> [--platforms macos,windows,linux,web,ios,android] [--all] [--bundle-id <id>]".to_string())?
            .clone();
        if name.starts_with("--") {
            return Err("usage: tzr new <name> [--platforms ...]".to_string());
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

        let default_bundle_id = format!("dev.tezzera.{}", crate_name);
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

/// Also used by `tzr bundle-id` to validate an id typed after project
/// creation, not just at `tzr new` time.
pub(crate) fn validate_bundle_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("bundle id cannot be empty".to_string());
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_') {
        return Err(format!(
            "invalid bundle id '{}': use letters, numbers, '.', '-', '_' (e.g. dev.tezzera.myapp)",
            id
        ));
    }
    Ok(())
}

/// Interactive checkbox-style prompt. The host OS (whichever `tzr` itself is
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
/// falls back to the default so `tzr new x < /dev/null` still works.
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

/// Prints `tzr new --help`'s focused usage.
pub fn print_help() {
    println!("tzr new <name> — scaffold a new TEZZERA app");
    println!();
    println!("USAGE:");
    println!("  tzr new <name> [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("  --platforms <list>  Comma list: macos,windows,linux,web,ios,android");
    println!("                      (skips the interactive platform prompt)");
    println!("  --all               Every platform (skips the prompt)");
    println!("  --bundle-id <id>    App bundle/package id, e.g. dev.tezzera.myapp");
    println!("                      (skips the bundle-id prompt; update later with `tzr bundle-id`)");
    println!("  -h, --help          Print this message");
    println!();
    println!("With no --platforms/--all, you're prompted interactively; the host OS");
    println!("(the one running `tzr`) is included automatically.");
    println!();
    println!("EXAMPLES:");
    println!("  tzr new myapp");
    println!("  tzr new myapp --platforms macos,ios --bundle-id com.example.myapp");
    println!("  tzr new myapp --all");
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
        "Creating TEZZERA app '{}' for: {}",
        name,
        opts.platforms.iter().map(|p| p.key()).collect::<Vec<_>>().join(", ")
    );

    // ── Directory tree ─────────────────────────────────────────────────────
    fs::create_dir_all(dir.join("src").join("screens"))
        .map_err(|e| format!("failed to create directories: {}", e))?;

    // ── Core project files ─────────────────────────────────────────────────
    write(dir.join("Cargo.toml"), &cargo_toml(name, &crate_name, &framework, &opts))?;
    write(dir.join("tzr.toml"), &tzr_toml(name, &bundle_id, &opts))?;
    write(dir.join(".gitignore"), "/target\n/dist\n*.app\n")?;
    write(dir.join("README.md"), &readme(name, &opts))?;

    // ── Structured source ──────────────────────────────────────────────────
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
    }
    if has(Platform::Ios) {
        // Physical Info.plist — for the older Phase 20-22 hand-rolled
        // `tzr run --target ios` harness only (kept working per the
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
    if has(Platform::Web) { println!("    web/index.html     web host page"); }
    if has(Platform::Ios) {
        println!("    ios/App.xcodeproj/ real Xcode project — open, build, run");
        println!("    ios/App/           AppDelegate/SceneDelegate/EngineViewController.swift");
        println!("    ios/Info.plist     legacy plist (tzr run --target ios only, superseded by App.xcodeproj)");
    }
    if has(Platform::MacOs) { println!("    macos/             icon.icns, Info.plist, entitlements.plist"); }
    if has(Platform::Windows) { println!("    windows/           icon.ico, app.manifest"); }
    if has(Platform::Linux) { println!("    linux/             icon.png, app.desktop"); }
    if has(Platform::Ios) { println!("    ios/App/Assets.xcassets/  iOS app icon"); }
    if has(Platform::Android) { println!("    android/.../mipmap-*/    Android launcher icon"); }
    if has(Platform::Web) { println!("    web/favicon.ico, icon-*.png  web/PWA icons"); }
    println!("    tzr.toml           app manifest (name, bundle id — `tzr bundle-id` to change)");
    println!();
    println!("  Run it:");
    println!("    cd {}", name);
    if has(Platform::MacOs) { println!("    tzr run --mac           # macOS"); }
    if has(Platform::Windows) { println!("    tzr run --win           # Windows (build only — see Known Issues)"); }
    if has(Platform::Linux) { println!("    tzr run --lnx           # Linux (build only on this host)"); }
    if has(Platform::Web) { println!("    tzr run --target web    # browser"); }
    if has(Platform::Ios) {
        println!("    tzr run --target ios    # iOS simulator (hand-rolled harness, Phase 20-22)");
        println!("    open ios/App.xcodeproj  # real Xcode project (D106) — build & run from Xcode");
    }
    println!();
    Ok(())
}

/// The framework checkout this `tzr` was built from — used for path deps so
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
    let tezzera_ffi_dep = if native_bridge {
        format!("tezzera-ffi = {{ path = \"{framework}/tezzera-ffi\" }}\n")
    } else {
        String::new()
    };
    // `ffi.rs`'s Android section uses `jni::JNIEnv`/`JObject`/`jint` etc.
    // directly (JNI function signatures cross this crate boundary, unlike
    // the plain-C iOS path) — needs its own `jni` dependency alongside
    // `tezzera-ffi`'s internal one, target-gated the same way.
    let android_jni_dep = if opts.platforms.contains(&Platform::Android) {
        "\n[target.'cfg(target_os = \"android\")'.dependencies]\njni = \"0.21\"\n"
    } else {
        ""
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
tezzera = {{ path = "{framework}/tezzera" }}
{tezzera_ffi_dep}{web}{android_jni_dep}"#
    )
}

fn tzr_toml(name: &str, bundle_id: &str, opts: &NewOptions) -> String {
    let plats = opts
        .platforms
        .iter()
        .map(|p| format!("\"{}\"", p.key()))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"# TEZZERA app manifest — read by `tzr run` / `tzr build`.
name = "{name}"
bundle_id = "{bundle_id}"
platforms = [{plats}]
"#
    )
}

fn readme(name: &str, opts: &NewOptions) -> String {
    let mut runs = String::from("- `tzr run` — desktop\n");
    if opts.platforms.contains(&Platform::Web) {
        runs.push_str("- `tzr run --target web` — browser (WebAssembly)\n");
    }
    if opts.platforms.contains(&Platform::Ios) {
        runs.push_str("- `tzr run --target ios` — iOS simulator\n");
    }
    format!("# {name}\n\nA TEZZERA app.\n\n## Run\n\n{runs}")
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
    // `tezzera-ffi` dependency in Cargo.toml.
    let ffi_mod = if wants_platform_themes { "mod ffi;\n" } else { "" };
    format!(
        r#"//! {name} — a TEZZERA app.
//!
//! `launch()` is shared by every platform. The native binary calls it from
//! `main`; the web build calls it from a `wasm-bindgen(start)` entry.

mod app;
{ffi_mod}mod screens;
mod theme;

use tezzera::prelude::*;

/// Start the app. Runs the winit event loop on native; hands off to the
/// browser's requestAnimationFrame loop on web.
pub fn launch() {{
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

use tezzera::prelude::*;
use tezzera::theme::set_theme;

use crate::screens::{{counter_screen, home_screen}};

/// Every screen in the app. Add a variant + a match arm to add a route.
#[derive(Clone, Copy, PartialEq)]
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
        let is_dark = ctx.state(true);

        let screen = nav.current().unwrap_or(Screen::Home);
        let body: BoxedWidget = match screen {{
            Screen::Home => Box::new(home_screen(&nav)),
            Screen::Counter => Box::new(counter_screen(&count)),
        }};

        // App bar: a back button appears off Home; a theme toggle on the right.
        let mut bar = AppBar::new(screen.title()).back_button(&nav);
        let label = if is_dark.get() {{ "\u{{2600}} Light" }} else {{ "\u{{263e}} Dark" }};
        let d = is_dark.clone();
        bar = bar.action(Button::new(label).on_press(move || {{
            let next = !d.get();
            d.set(next);
            set_theme(if next {{ crate::theme::dark() }} else {{ crate::theme::light() }});
        }}));

        Scaffold::new(body).app_bar(bar).into_element()
    }}
}}
"#
    )
}

/// Generates `src/theme.rs`. Always emits `dark()`/`light()` (used by the
/// in-app theme toggle in `app.rs`); when iOS and/or Android are selected it
/// also emits `themes()`, a platform-keyed `Themes` bundle wiring Cupertino
/// for iOS and Material for Android (D105 Phase 23 Step 5) so a generated
/// app looks native-appropriate on each target with no hand-editing.
fn theme_rs(opts: &NewOptions) -> String {
    let has_ios = opts.platforms.contains(&Platform::Ios);
    let has_android = opts.platforms.contains(&Platform::Android);

    let mut out = String::from(
        r#"//! App theme. Edit these to customize colors, or build a `ThemeData` from
//! scratch — the built-ins are just a convenient starting point.

use tezzera::prelude::ThemeData;

/// The app's dark theme.
pub fn dark() -> ThemeData {
    tezzera::prelude::dark_theme()
}

/// The app's light theme.
pub fn light() -> ThemeData {
    tezzera::prelude::light_theme()
}
"#,
    );

    if has_ios || has_android {
        out.push_str(
            r#"
/// Per-platform look (D105): iOS gets Cupertino chrome, Android gets
/// Material chrome; every other platform (desktop, web) falls back to
/// `light()`. Passed to `App::themes(..)` in `lib.rs`.
pub fn themes() -> tezzera::prelude::Themes {
    tezzera::prelude::Themes::new(light())
"#,
        );
        if has_ios {
            out.push_str(
                "        .platform(tezzera::prelude::Platform::Ios, tezzera::prelude::cupertino())\n",
            );
        }
        if has_android {
            out.push_str(
                "        .platform(tezzera::prelude::Platform::Android, tezzera::prelude::material())\n",
            );
        }
        out.push_str("}\n");
    }

    out
}

const SCREENS_MOD_RS: &str = r#"//! One file per screen. Re-export each screen's builder here.

mod counter;
mod home;

pub use counter::counter_screen;
pub use home::home_screen;
"#;

const HOME_RS: &str = r#"//! The home screen — an index of the app's routes.

use tezzera::prelude::*;

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

use tezzera::prelude::*;

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
  <style>html, body {{ margin: 0; padding: 0; background: #14141a; }}</style>
</head>
<body>
  <script type="module">
    import init from './{crate_name}.js';
    init().catch((e) => {{
      console.error('tezzera init failed:', e);
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
// regenerated) by `tzr package`. See package.rs for the consuming side.

/// `macos/Info.plist` — the real bundle plist `tzr package`'s `bundle_macos`
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
/// ID notarization) needs specific entitlements added here by hand; `tzr`
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
  tzr package's identity flag (see tzr package help) passes this file to
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
/// `Icon` are filled in at `tzr package` time (paths depend on install
/// location); this template ships placeholders `tzr package` substitutes.
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
        val linker = "$ndkHome/toolchains/llvm/prebuilt/$hostTag/bin/aarch64-linux-android$minSdk-clang"
        // Plain ProcessBuilder, not Gradle's exec DSL block — that's a
        // Project extension function not reliably reachable from inside a
        // registered task's doLast across Gradle/Kotlin-DSL versions
        // (confirmed: "Unresolved reference 'exec'" against this project's
        // Gradle 9.4 — plain JVM process APIs sidestep that entirely).
        val processBuilder = ProcessBuilder(
            "cargo", "build", "--lib", "--target", rustTriple, "--release"
        )
        processBuilder.directory(rootProject.projectDir.parentFile)
        processBuilder.environment()["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"] = linker
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

    <application
        android:allowBackup="true"
        android:icon="@mipmap/ic_launcher"
        android:roundIcon="@mipmap/ic_launcher_round"
        android:label="@string/app_name"
        android:theme="@android:style/Theme.Black.NoTitleBar.Fullscreen">
        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:configChanges="orientation|screenSize|keyboardHidden"
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
import android.os.Bundle
import android.view.Choreographer
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView

class MainActivity : Activity(), SurfaceHolder.Callback {{

    companion object {{
        init {{ System.loadLibrary("{crate_lib_name}") }}
    }}

    private external fun nativeInit(surface: Surface, width: Int, height: Int, scale: Float): Long
    private external fun nativeResize(
        handle: Long, width: Int, height: Int, scale: Float,
        safeTop: Float, safeRight: Float, safeBottom: Float, safeLeft: Float,
    )
    private external fun nativeTouch(handle: Long, kind: Int, x: Float, y: Float)
    private external fun nativeFrame(handle: Long)
    private external fun nativeShutdown(handle: Long)

    private var engineHandle: Long = 0
    private lateinit var surfaceView: SurfaceView

    private val frameCallback = object : Choreographer.FrameCallback {{
        override fun doFrame(frameTimeNanos: Long) {{
            if (engineHandle != 0L) {{
                nativeFrame(engineHandle)
                Choreographer.getInstance().postFrameCallback(this)
            }}
        }}
    }}

    override fun onCreate(savedInstanceState: Bundle?) {{
        super.onCreate(savedInstanceState)
        surfaceView = SurfaceView(this)
        surfaceView.holder.addCallback(this)
        setContentView(surfaceView)
    }}

    override fun surfaceCreated(holder: SurfaceHolder) {{
        val scale = resources.displayMetrics.density
        val width = surfaceView.width
        val height = surfaceView.height
        engineHandle = nativeInit(holder.surface, width, height, scale)
        Choreographer.getInstance().postFrameCallback(frameCallback)
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
        nativeTouch(engineHandle, kind, event.x, event.y)
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
// view driving `tzr_engine_init`/`resize`/`input`/`frame` through the FFI
// boundary `tezzera-ffi` provides. FFI functions are declared via
// `@_silgen_name` (no bridging header needed) — the same mechanism proven
// working in `tezzera-ffi/examples/ios_stub.rs`'s Simulator verification.
// `Info.plist` is Xcode-synthesized (`GENERATE_INFOPLIST_FILE = YES` in the
// generated `.pbxproj`) rather than a physical file here — the physical
// `ios/Info.plist` this module also generates is for the OLDER Phase 20-22
// hand-rolled `tzr run --target ios` harness only (kept working per the
// Migration Rule until Step 4 retires it); the two are independent.

const IOS_APP_DELEGATE_SWIFT: &str = r#"//! Owns the app lifecycle — our own AppDelegate, not winit's implicit one
//! (this is the whole point of D106: winit's iOS backend calls
//! UIApplicationMain itself and generates an AppDelegate no host code can
//! reach, which blocks push notifications, deep links, and background
//! tasks).

import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        true
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

const IOS_ENGINE_VIEW_CONTROLLER_SWIFT: &str = r#"//! Drives the TEZZERA engine through the `tezzera-ffi` C boundary
//! (`tezzera-ffi/include/tzr_engine.h`) — a CAMetalLayer-backed view,
//! init/resize/input/frame calls, and real `UIView.safeAreaInsets` feeding
//! `tezzera_core::SafeArea` (replacing the old winit outer/inner-size
//! workaround from Phase 20-22).

import UIKit
import QuartzCore

// MARK: - FFI declarations (mirrors tezzera-ffi/include/tzr_engine.h;
// no bridging header needed — matches the pattern proven in
// tezzera-ffi/examples/ios_stub.rs's Simulator verification).

typealias TzrEngine = OpaquePointer

struct TzrInputEvent {
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

private let TZR_EVENT_MOUSE_MOVE: UInt32 = 0
private let TZR_EVENT_MOUSE_DOWN: UInt32 = 1
private let TZR_EVENT_MOUSE_UP: UInt32 = 2
private let TZR_BUTTON_LEFT: UInt32 = 0

@_silgen_name("tzr_engine_init")
func tzr_engine_init(_ surfaceHandle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32, _ scale: Float) -> TzrEngine?

@_silgen_name("tzr_engine_resize")
func tzr_engine_resize(
    _ engine: TzrEngine?, _ width: UInt32, _ height: UInt32, _ scale: Float,
    _ safeTop: Float, _ safeRight: Float, _ safeBottom: Float, _ safeLeft: Float
)

@_silgen_name("tzr_engine_input")
func tzr_engine_input(_ engine: TzrEngine?, _ events: UnsafePointer<TzrInputEvent>?, _ count: Int)

@_silgen_name("tzr_engine_frame")
func tzr_engine_frame(_ engine: TzrEngine?)

@_silgen_name("tzr_engine_shutdown")
func tzr_engine_shutdown(_ engine: TzrEngine?)

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
final class MetalView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    override init(frame: CGRect) {
        super.init(frame: frame)
        (layer as! CAMetalLayer).contentsScale = UIScreen.main.scale
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        (layer as! CAMetalLayer).contentsScale = UIScreen.main.scale
    }
}

final class EngineViewController: UIViewController {
    private var engine: TzrEngine?
    private var displayLink: CADisplayLink?

    override func loadView() {
        view = MetalView(frame: UIScreen.main.bounds)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        let scale = Float(view.contentScaleFactor)
        let width = UInt32(view.bounds.width * CGFloat(scale))
        let height = UInt32(view.bounds.height * CGFloat(scale))
        let viewPtr = Unmanaged.passUnretained(view).toOpaque()
        engine = tzr_engine_init(viewPtr, width, height, scale)

        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .default)
        displayLink = link
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        guard let engine else { return }
        let scale = Float(view.contentScaleFactor)
        let width = UInt32(view.bounds.width * CGFloat(scale))
        let height = UInt32(view.bounds.height * CGFloat(scale))
        let insets = view.safeAreaInsets
        tzr_engine_resize(
            engine, width, height, scale,
            Float(insets.top), Float(insets.right), Float(insets.bottom), Float(insets.left)
        )
    }

    @objc private func tick() {
        guard let engine else { return }
        tzr_engine_frame(engine)
    }

    // MARK: Touch -> MouseDown/MouseMove/MouseUp (same convention the
    // existing winit `Touch` handling and `TzrInputEventFfi` conversion use
    // — no separate touch event kind needed).

    private func send(kind: UInt32, touches: Set<UITouch>) {
        guard let engine, let touch = touches.first else { return }
        let p = touch.location(in: view)
        var event = TzrInputEvent(
            kind: kind, x: Float(p.x), y: Float(p.y), button: TZR_BUTTON_LEFT,
            key: 0, character: 0, width: 0, height: 0, delta_x: 0, delta_y: 0
        )
        withUnsafePointer(to: &event) { tzr_engine_input(engine, $0, 1) }
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: TZR_EVENT_MOUSE_DOWN, touches: touches)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: TZR_EVENT_MOUSE_MOVE, touches: touches)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: TZR_EVENT_MOUSE_UP, touches: touches)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        send(kind: TZR_EVENT_MOUSE_UP, touches: touches)
    }

    deinit {
        displayLink?.invalidate()
        if let engine { tzr_engine_shutdown(engine) }
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
/// for the older Phase 20-22 hand-rolled `tzr run --target ios` harness.
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
/// throwaway Swift stub against `tezzera-ffi`'s compiled staticlib).
fn ios_other_ldflags(crate_name: &str) -> String {
    format!(
        "-l{crate_name} -framework UIKit -framework QuartzCore -framework Metal \
         -framework Foundation -framework CoreGraphics -framework Security -framework CoreFoundation"
    )
}

/// Shared Xcode scheme — without this, `xcodebuild -scheme App` can't find
/// a scheme headlessly (Xcode normally auto-creates one on first GUI open,
/// which doesn't happen in a `tzr new`/CI context). Matters for Phase 24
/// Step 4 (`tzr run --target ios` driving `xcodebuild`) too, not just manual use.
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
/// exports the `tzr_engine_*` C symbols the native host links against,
/// instantiating the app's own `AppRoot` (the SAME root component
/// desktop/web already drive). Mirrors `tezzera-ffi/examples/ios_stub.rs`,
/// the reference pattern this template is generated from.
fn ffi_rs(bundle_id: &str) -> String {
    let jni_prefix = jni_class_prefix(bundle_id);

    let header = r#"//! Native-host FFI glue (D106 Phase 24) — exports the ABI
//! `ios/App/EngineViewController.swift` and `android/.../MainActivity.kt`
//! call into. iOS uses the plain C ABI in `tezzera-ffi`'s
//! `include/tzr_engine.h` (pattern: `tezzera-ffi/examples/ios_stub.rs`).
//! Android uses JNI instead — Kotlin's `external fun` resolves to a symbol
//! literally named `Java_<package>_<Class>_<method>` (JNI's mangling: `.` ->
//! `_`, a literal `_` -> `_1` — see `jni_class_prefix` in
//! `tezzera-cli/src/commands/new.rs`, which computed the exact prefix below
//! from this app's bundle id at `tzr new` time). Pattern:
//! `tezzera-ffi/examples/android_stub.rs`.

use std::os::raw::c_void;
#[cfg(target_os = "ios")]
use std::ptr::NonNull;

#[cfg(any(target_os = "ios", target_os = "android"))]
use tezzera::prelude::*;
use tezzera_ffi::{Engine, TzrInputEventFfi};
#[cfg(target_os = "ios")]
use tezzera_ffi::RawSurface;
#[cfg(target_os = "android")]
use tezzera_ffi::AndroidSurfaceHandle;

#[cfg(any(target_os = "ios", target_os = "android"))]
use crate::app::AppRoot;

// -- iOS: plain C ABI --------------------------------------------------------

/// # Safety
/// `surface_handle` must be a valid, non-null `CAMetalLayer`-backed
/// `UIView*` for the engine's lifetime.
#[cfg(target_os = "ios")]
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_init(
    surface_handle: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut Engine {
    let Some(handle) = NonNull::new(surface_handle) else { return std::ptr::null_mut() };
    let surface = unsafe { RawSurface::from_ca_metal_layer(handle, None, width, height, scale) };
    let theme = light_theme();
    match Engine::init(Box::new(AppRoot), theme, surface) {
        Some(engine) => Box::into_raw(engine),
        None => std::ptr::null_mut(),
    }
}

#[cfg(not(target_os = "ios"))]
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_init(
    _surface_handle: *mut c_void,
    _width: u32,
    _height: u32,
    _scale: f32,
) -> *mut Engine {
    std::ptr::null_mut()
}

/// # Safety
/// `engine` must be a live pointer previously returned by `tzr_engine_init`
/// (or null, which is a no-op).
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_resize(
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
    let safe_area = tezzera::core::SafeArea { top: safe_top, right: safe_right, bottom: safe_bottom, left: safe_left };
    unsafe { (*engine).resize(width, height, scale, safe_area) };
}

/// # Safety
/// `engine` must be a live pointer from `tzr_engine_init`; `events` must
/// point to at least `count` valid `TzrInputEvent`s.
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_input(
    engine: *mut Engine,
    events: *const TzrInputEventFfi,
    count: usize,
) {
    if engine.is_null() || events.is_null() { return; }
    let slice = unsafe { std::slice::from_raw_parts(events, count) };
    unsafe { (*engine).input(slice) };
}

/// # Safety
/// `engine` must be a live pointer from `tzr_engine_init` (or null).
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_frame(engine: *mut Engine) {
    if engine.is_null() { return; }
    unsafe { (*engine).frame() };
}

/// # Safety
/// `engine` must be a pointer previously returned by `tzr_engine_init` and
/// not yet passed to this function; it must not be used again afterward.
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_shutdown(engine: *mut Engine) {
    if engine.is_null() { return; }
    drop(unsafe { Box::from_raw(engine) });
}

// -- Android: JNI -------------------------------------------------------------
// Symbol names are burned in at codegen time (JNI resolves by exact name,
// no runtime registration) — see the module doc above for why this can't be
// the same plain-C functions iOS uses. `AndroidEngine` keeps the `Engine`
// and the `AndroidSurfaceHandle` (whose `Drop` releases the `ANativeWindow`
// reference) alive together, torn down as a unit in nativeShutdown — same
// reasoning as `tezzera-ffi/examples/android_stub.rs`'s `AndroidEngine`.

#[cfg(target_os = "android")]
struct AndroidEngine {
    engine: Box<Engine>,
    #[allow(dead_code)]
    surface: AndroidSurfaceHandle,
}
"#;

    let android = format!(
        r#"
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
    let raw_env = env.get_raw();
    let Some(handle) = (unsafe {{ AndroidSurfaceHandle::from_jni(raw_env, &surface) }}) else {{
        return 0;
    }};
    let raw_surface = unsafe {{ handle.raw_surface(width as u32, height as u32, scale) }};
    let theme = light_theme();
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
    let safe_area = tezzera::core::SafeArea {{ top: safe_top, right: safe_right, bottom: safe_bottom, left: safe_left }};
    unsafe {{ (*ptr).engine.resize(width as u32, height as u32, scale, safe_area) }};
}}

/// One touch/pointer event per call — `kind` is `0` = move, `1` = down,
/// `2` = up (matching `tezzera_ffi`'s `TZR_EVENT_MOUSE_*` constants); a
/// touch is always reported as the left button, mirroring how the existing
/// winit `Touch` handling already treats touch input (see `tezzera-ffi`'s
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
    let event = TzrInputEventFfi {{
        kind: kind as u32, x, y, button: 0, key: 0, character: 0,
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
/// `external fun`s resolve to, e.g. `dev.tezzera.theme_preview` ->
/// `dev_tezzera_theme_1preview_MainActivity`.
fn jni_class_prefix(bundle_id: &str) -> String {
    format!("{}_MainActivity", jni_mangle(&android_package(bundle_id)))
}

#[cfg(test)]
mod ffi_codegen_tests {
    use super::*;

    #[test]
    fn jni_mangle_replaces_dots_with_underscore() {
        assert_eq!(jni_mangle("dev.tezzera.myapp"), "dev_tezzera_myapp");
    }

    #[test]
    fn jni_mangle_escapes_literal_underscore_as_1() {
        assert_eq!(jni_mangle("dev.tezzera.theme_preview"), "dev_tezzera_theme_1preview");
    }

    #[test]
    fn android_package_lowercases_and_strips_hyphens() {
        assert_eq!(android_package("Dev.Tezzera.My-App"), "dev.tezzera.my_app");
    }

    #[test]
    fn jni_class_prefix_matches_real_symbol_shape() {
        assert_eq!(
            jni_class_prefix("dev.tezzera.theme_preview"),
            "dev_tezzera_theme_1preview_MainActivity"
        );
    }

    #[test]
    fn ffi_rs_embeds_the_derived_jni_prefix() {
        let src = ffi_rs("dev.tezzera.myapp");
        assert!(src.contains("Java_dev_tezzera_myapp_MainActivity_nativeInit"));
        assert!(src.contains("Java_dev_tezzera_myapp_MainActivity_nativeFrame"));
        assert!(src.contains("Java_dev_tezzera_myapp_MainActivity_nativeShutdown"));
    }
}
