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

    // ── Per-platform scaffolding ───────────────────────────────────────────
    if has(Platform::Web) {
        fs::create_dir_all(dir.join("web")).map_err(|e| e.to_string())?;
        write(dir.join("web").join("index.html"), &web_index_html(name, &crate_name))?;
    }
    if has(Platform::Ios) {
        fs::create_dir_all(dir.join("ios")).map_err(|e| e.to_string())?;
        write(dir.join("ios").join("Info.plist"), &ios_info_plist(name, &crate_name, &bundle_id))?;
    }
    if has(Platform::Android) {
        // Placeholder — Android harness generation is a follow-up.
        fs::create_dir_all(dir.join("android")).map_err(|e| e.to_string())?;
        write(dir.join("android").join("README.md"),
            "Android harness scaffolding is not generated yet.\n")?;
    }

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
    println!("    src/screens/       home + counter screens");
    if has(Platform::Web) { println!("    web/index.html     web host page"); }
    if has(Platform::Ios) { println!("    ios/Info.plist     iOS app bundle plist"); }
    println!("    tzr.toml           app manifest");
    println!();
    println!("  Run it:");
    println!("    cd {}", name);
    println!("    tzr run                 # desktop");
    if has(Platform::Web) { println!("    tzr run --target web    # browser"); }
    if has(Platform::Ios) { println!("    tzr run --target ios    # iOS simulator"); }
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
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[workspace]

[lib]
name = "{crate_name}"
crate-type = ["cdylib", "rlib"]
path = "src/lib.rs"

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
tezzera = {{ path = "{framework}/tezzera" }}
{web}"#
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
    format!(
        r#"//! {name} — a TEZZERA app.
//!
//! `launch()` is shared by every platform. The native binary calls it from
//! `main`; the web build calls it from a `wasm-bindgen(start)` entry.

mod app;
mod screens;
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
  <key>LSRequiresIPhoneOS</key><true/>
  <key>UILaunchScreen</key><dict/>
  <key>UIRequiredDeviceCapabilities</key><array><string>arm64</string></array>
  <key>MinimumOSVersion</key><string>13.0</string>
</dict>
</plist>
"#
    )
}
