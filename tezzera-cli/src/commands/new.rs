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

/// A target platform the scaffolder can wire up. Desktop is always included.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Platform {
    Desktop,
    Web,
    Ios,
    Android,
}

impl Platform {
    fn key(&self) -> &'static str {
        match self {
            Platform::Desktop => "desktop",
            Platform::Web => "web",
            Platform::Ios => "ios",
            Platform::Android => "android",
        }
    }
    fn from_key(s: &str) -> Option<Self> {
        match s {
            "desktop" => Some(Platform::Desktop),
            "web" => Some(Platform::Web),
            "ios" => Some(Platform::Ios),
            "android" => Some(Platform::Android),
            _ => None,
        }
    }
}

pub struct NewOptions {
    pub name: String,
    /// Selected platforms (always contains Desktop).
    pub platforms: Vec<Platform>,
}

impl NewOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let name = args
            .first()
            .ok_or_else(|| "usage: tzr new <name> [--platforms desktop,web,ios,android] [--all]".to_string())?
            .clone();
        if name.starts_with("--") {
            return Err("usage: tzr new <name> [--platforms ...]".to_string());
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(format!("invalid project name '{}': use letters, numbers, - or _", name));
        }

        // Parse flags. `--platforms a,b,c` or `--all` skip the interactive prompt.
        let mut explicit: Option<Vec<Platform>> = None;
        let mut i = 1;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--all" {
                explicit = Some(vec![Platform::Desktop, Platform::Web, Platform::Ios, Platform::Android]);
            } else if let Some(v) = arg.strip_prefix("--platforms=") {
                explicit = Some(parse_platforms(v)?);
            } else if arg == "--platforms" {
                i += 1;
                let v = args.get(i).ok_or_else(|| "--platforms requires a value".to_string())?;
                explicit = Some(parse_platforms(v)?);
            }
            i += 1;
        }

        let platforms = match explicit {
            Some(mut p) => {
                if !p.contains(&Platform::Desktop) {
                    p.insert(0, Platform::Desktop);
                }
                p
            }
            None => prompt_platforms(),
        };

        Ok(Self { name, platforms })
    }
}

fn parse_platforms(v: &str) -> Result<Vec<Platform>, String> {
    let mut out = Vec::new();
    for part in v.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let p = Platform::from_key(part)
            .ok_or_else(|| format!("unknown platform '{}'. Use: desktop, web, ios, android", part))?;
        if !out.contains(&p) {
            out.push(p);
        }
    }
    Ok(out)
}

/// Interactive checkbox-style prompt. Desktop is always on; ask about the rest.
fn prompt_platforms() -> Vec<Platform> {
    let mut platforms = vec![Platform::Desktop];
    println!();
    println!("  Which platforms should this app target?");
    println!("  (Desktop is always included.)");
    println!();
    for (p, label) in [
        (Platform::Web, "Web (WebAssembly)"),
        (Platform::Ios, "iOS (simulator)"),
        (Platform::Android, "Android"),
    ] {
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

pub fn run(opts: NewOptions) -> Result<(), String> {
    let name = &opts.name;
    let crate_name = name.replace('-', "_");
    let bundle_id = format!("dev.tezzera.{}", crate_name);
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
        write(dir.join("src").join("ffi.rs"), &ffi_rs())?;
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
        // Placeholder — Android harness generation is a follow-up (Phase 24 Step 3).
        fs::create_dir_all(dir.join("android")).map_err(|e| e.to_string())?;
        write(dir.join("android").join("README.md"),
            "Android harness scaffolding is not generated yet.\n")?;
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
    println!("    desktop/icon.{{icns,ico}}  desktop app icon (macOS/Windows)");
    if has(Platform::Ios) { println!("    ios/App/Assets.xcassets/  iOS app icon"); }
    if has(Platform::Android) { println!("    android/.../mipmap-*/    Android launcher icon"); }
    if has(Platform::Web) { println!("    web/favicon.ico, icon-*.png  web/PWA icons"); }
    println!("    tzr.toml           app manifest");
    println!();
    println!("  Run it:");
    println!("    cd {}", name);
    println!("    tzr run                 # desktop");
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
{tezzera_ffi_dep}{web}"#
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
final class MetalView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }
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
fn ffi_rs() -> String {
    r#"//! Native-host FFI glue (D106 Phase 24) — exports the C ABI
//! `ios/App/EngineViewController.swift` (and, later, an Android host) call
//! into. See `tezzera-ffi`'s `include/tzr_engine.h` for the ABI this
//! implements, and `tezzera-ffi/examples/ios_stub.rs` for the pattern this
//! file follows.

use std::os::raw::c_void;
#[cfg(any(target_os = "ios", target_os = "android"))]
use std::ptr::NonNull;

#[cfg(any(target_os = "ios", target_os = "android"))]
use tezzera::prelude::*;
use tezzera_ffi::{Engine, TzrInputEventFfi};
#[cfg(any(target_os = "ios", target_os = "android"))]
use tezzera_ffi::RawSurface;

#[cfg(any(target_os = "ios", target_os = "android"))]
use crate::app::AppRoot;

/// # Safety
/// `surface_handle` must be a valid, non-null `CAMetalLayer`-backed
/// `UIView*` (iOS) or `ANativeWindow*` (Android) for the engine's lifetime.
#[cfg(any(target_os = "ios", target_os = "android"))]
#[no_mangle]
pub unsafe extern "C" fn tzr_engine_init(
    surface_handle: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut Engine {
    let Some(handle) = NonNull::new(surface_handle) else { return std::ptr::null_mut() };

    #[cfg(target_os = "ios")]
    let surface = unsafe { RawSurface::from_ca_metal_layer(handle, None, width, height, scale) };
    #[cfg(target_os = "android")]
    let surface = unsafe { RawSurface::from_native_window(handle, width, height, scale) };

    let theme = light_theme();
    match Engine::init(Box::new(AppRoot), theme, surface) {
        Some(engine) => Box::into_raw(engine),
        None => std::ptr::null_mut(),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
"#
    .to_string()
}
