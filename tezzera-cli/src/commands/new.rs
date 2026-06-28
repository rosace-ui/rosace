use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum Template {
    Counter,
    NavApp,
    FormApp,
    Dashboard,
}

impl Template {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "counter"   => Ok(Template::Counter),
            "nav-app"   => Ok(Template::NavApp),
            "form-app"  => Ok(Template::FormApp),
            "dashboard" => Ok(Template::Dashboard),
            other => Err(format!(
                "unknown template '{}'. Available: counter, nav-app, form-app, dashboard",
                other
            )),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Template::Counter   => "counter",
            Template::NavApp    => "nav-app",
            Template::FormApp   => "form-app",
            Template::Dashboard => "dashboard",
        }
    }
}

pub struct NewOptions {
    pub name: String,
    pub template: Template,
}

impl NewOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let name = args.first()
            .ok_or_else(|| "usage: tzr new <name>".to_string())?
            .clone();
        // Validate: only alphanumeric + underscores + hyphens
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(format!("invalid project name '{}': use letters, numbers, - or _", name));
        }

        let mut template = Template::Counter;
        let mut i = 1;
        while i < args.len() {
            let arg = &args[i];
            if arg.starts_with("--template=") {
                let val = &arg["--template=".len()..];
                template = Template::from_str(val)?;
            } else if arg == "--template" {
                i += 1;
                let val = args.get(i)
                    .ok_or_else(|| "--template requires a value".to_string())?;
                template = Template::from_str(val)?;
            }
            i += 1;
        }

        Ok(Self { name, template })
    }
}

pub fn run(opts: NewOptions) -> Result<(), String> {
    let name = &opts.name;
    let dir = Path::new(name);

    if dir.exists() {
        return Err(format!("directory '{}' already exists", name));
    }

    println!("Creating TEZZERA project '{}' (template: {})...", name, opts.template.name());

    fs::create_dir_all(dir.join("src"))
        .map_err(|e| format!("failed to create directories: {}", e))?;

    let cargo = cargo_toml_for_template(name, &opts.template);
    let main = match opts.template {
        Template::Counter   => main_rs(name),
        Template::NavApp    => nav_app_main_rs(name),
        Template::FormApp   => form_app_main_rs(name),
        Template::Dashboard => dashboard_main_rs(name),
    };

    write_file(dir.join("Cargo.toml"), &cargo)?;
    write_file(dir.join("src").join("main.rs"), &main)?;
    write_file(dir.join(".gitignore"), "/target\n")?;

    println!();
    println!("  Created '{}' with template '{}'", name, opts.template.name());
    println!();
    println!("  Next steps:");
    println!("    cd {}", name);
    println!("    tzr dev");
    println!();
    Ok(())
}

fn write_file(path: impl AsRef<Path>, content: &str) -> Result<(), String> {
    fs::write(&path, content)
        .map_err(|e| format!("failed to write {}: {}", path.as_ref().display(), e))
}

fn cargo_toml_for_template(name: &str, template: &Template) -> String {
    let nav_dep = if *template == Template::NavApp {
        r#"tezzera-nav      = { git = "https://github.com/tezzera-ui/tezzera" }
"#
    } else {
        ""
    };

    format!(r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
tezzera-core     = {{ git = "https://github.com/tezzera-ui/tezzera" }}
tezzera-layout   = {{ git = "https://github.com/tezzera-ui/tezzera" }}
tezzera-render   = {{ git = "https://github.com/tezzera-ui/tezzera" }}
tezzera-state    = {{ git = "https://github.com/tezzera-ui/tezzera" }}
tezzera-platform = {{ git = "https://github.com/tezzera-ui/tezzera" }}
tezzera-theme    = {{ git = "https://github.com/tezzera-ui/tezzera" }}
tezzera-widgets  = {{ git = "https://github.com/tezzera-ui/tezzera" }}
tezzera-animate  = {{ git = "https://github.com/tezzera-ui/tezzera" }}
{nav_dep}"#)
}

fn main_rs(name: &str) -> String {
    let title = name.to_uppercase();
    let title_len = title.len();
    format!(r#"use tezzera_core::types::{{Point, Rect, Size}};
use tezzera_platform::{{InputEvent, MouseButton, TezzeraApp}};
use tezzera_render::{{Color, FontCache, SkiaCanvas}};
use tezzera_state::use_atom;

const W: u32 = 480;
const H: u32 = 320;

const BG:     Color = Color::rgb(18, 18, 28);
const ACCENT: Color = Color::rgb(103, 80, 164);
const TEXT:   Color = Color::rgb(230, 225, 229);

fn main() {{
    let font = FontCache::system_mono().expect("no system font found");
    let count = use_atom(0_i32);
    let mut mx = 0.0_f32;
    let mut my = 0.0_f32;

    TezzeraApp::new()
        .title("{title} — TEZZERA")
        .size(W, H)
        .run(move |canvas: &mut SkiaCanvas, events: &[InputEvent]| {{
            for ev in events {{
                match ev {{
                    InputEvent::MouseDown {{ x, y, button: MouseButton::Left }} => {{
                        let bx = W as f32 / 2.0 - 60.0;
                        let by = H as f32 / 2.0 + 20.0;
                        if *x >= bx && *x <= bx + 120.0 && *y >= by && *y <= by + 40.0 {{
                            count.update(|n| n + 1);
                        }}
                    }}
                    InputEvent::MouseMove {{ x, y }} => {{ mx = *x; my = *y; }}
                    _ => {{}}
                }}
            }}

            canvas.clear(BG);

            canvas.draw_text(
                "{title}",
                Point {{ x: W as f32 / 2.0 - {title_len}.0 * 9.0 / 2.0, y: 40.0 }},
                TEXT, &font, 18.0,
            );

            let label = format!("{{}}", count.get());
            canvas.draw_text(
                &label,
                Point {{ x: W as f32 / 2.0 - label.len() as f32 * 12.0 / 2.0, y: H as f32 / 2.0 - 20.0 }},
                ACCENT, &font, 36.0,
            );

            let bx = W as f32 / 2.0 - 60.0;
            let by = H as f32 / 2.0 + 20.0;
            let hovered = mx >= bx && mx <= bx + 120.0 && my >= by && my <= by + 40.0;
            let btn_color = if hovered {{ Color::rgb(130, 100, 200) }} else {{ ACCENT }};
            canvas.fill_rect(
                Rect {{ origin: Point {{ x: bx, y: by }}, size: Size {{ width: 120.0, height: 40.0 }} }},
                btn_color,
            );
            canvas.draw_text("Click me", Point {{ x: bx + 18.0, y: by + 12.0 }}, Color::WHITE, &font, 14.0);
        }});
}}
"#, title = title, title_len = title_len)
}

fn nav_app_main_rs(name: &str) -> String {
    let title = name.to_uppercase();
    format!(r#"use tezzera_core::types::{{Point, Rect, Size}};
use tezzera_platform::{{InputEvent, Key, MouseButton, TezzeraApp}};
use tezzera_render::{{Color, FontCache, SkiaCanvas}};
use tezzera_state::use_atom;
use tezzera_nav::{{Navigator, Route}};

const W: u32 = 640;
const H: u32 = 480;

const BG:     Color = Color::rgb(18, 18, 28);
const ACCENT: Color = Color::rgb(103, 80, 164);
const TEXT:   Color = Color::rgb(230, 225, 229);
const MUTED:  Color = Color::rgb(140, 145, 175);
const SURFACE:Color = Color::rgb(28, 30, 46);

#[derive(Debug, Clone, PartialEq)]
enum Screen {{ Home, Detail, Settings }}
impl Route for Screen {{}}

fn draw_button(canvas: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, w: f32, label: &str, hovered: bool) {{
    let color = if hovered {{ Color::rgb(130, 100, 200) }} else {{ ACCENT }};
    canvas.fill_rect(Rect {{ origin: Point {{ x, y }}, size: Size {{ width: w, height: 40.0 }} }}, color);
    canvas.draw_text(label, Point {{ x: x + 12.0, y: y + 12.0 }}, TEXT, font, 14.0);
}}

fn main() {{
    let font = FontCache::system_mono().expect("no font");
    let nav = Navigator::new(Screen::Home);
    let mx = use_atom(0.0_f32);
    let my = use_atom(0.0_f32);

    TezzeraApp::new()
        .title("{title} — TEZZERA")
        .size(W, H)
        .run(move |canvas, events| {{
            let mouse_x = mx.get();
            let mouse_y = my.get();

            for ev in events {{
                match ev {{
                    InputEvent::MouseMove {{ x, y }} => {{ mx.set(*x); my.set(*y); }}
                    InputEvent::MouseDown {{ x, y, button: MouseButton::Left }} => {{
                        match nav.current() {{
                            Some(Screen::Home) => {{
                                if *y >= 160.0 && *y <= 200.0 && *x >= 60.0 && *x <= 260.0 {{ nav.push(Screen::Detail); }}
                                if *y >= 160.0 && *y <= 200.0 && *x >= 280.0 && *x <= 480.0 {{ nav.push(Screen::Settings); }}
                            }}
                            _ => {{
                                if *y >= 400.0 && *y <= 440.0 && *x >= 60.0 && *x <= 200.0 {{ nav.pop(); }}
                            }}
                        }}
                    }}
                    InputEvent::KeyDown {{ key: Key::Backspace, .. }} => {{ nav.pop(); }}
                    _ => {{}}
                }}
            }}

            canvas.clear(BG);

            match nav.current() {{
                Some(Screen::Home) => {{
                    canvas.draw_text("{title}", Point {{ x: 60.0, y: 60.0 }}, TEXT, &font, 24.0);
                    canvas.draw_text("Home Screen", Point {{ x: 60.0, y: 100.0 }}, MUTED, &font, 14.0);
                    let h1 = mouse_x >= 60.0 && mouse_x <= 260.0 && mouse_y >= 160.0 && mouse_y <= 200.0;
                    let h2 = mouse_x >= 280.0 && mouse_x <= 480.0 && mouse_y >= 160.0 && mouse_y <= 200.0;
                    draw_button(canvas, &font, 60.0, 160.0, 200.0, "→ Detail", h1);
                    draw_button(canvas, &font, 280.0, 160.0, 200.0, "→ Settings", h2);
                }}
                Some(Screen::Detail) => {{
                    canvas.fill_rect(Rect {{ origin: Point {{ x: 0.0, y: 0.0 }}, size: Size {{ width: W as f32, height: 4.0 }} }}, ACCENT);
                    canvas.draw_text("Detail Screen", Point {{ x: 60.0, y: 60.0 }}, TEXT, &font, 24.0);
                    canvas.draw_text(&format!("Stack depth: {{}}", nav.depth()), Point {{ x: 60.0, y: 100.0 }}, MUTED, &font, 14.0);
                    let hb = mouse_x >= 60.0 && mouse_x <= 200.0 && mouse_y >= 400.0 && mouse_y <= 440.0;
                    draw_button(canvas, &font, 60.0, 400.0, 140.0, "← Back", hb);
                }}
                Some(Screen::Settings) => {{
                    canvas.fill_rect(Rect {{ origin: Point {{ x: 0.0, y: 0.0 }}, size: Size {{ width: W as f32, height: 4.0 }} }}, Color::rgb(80, 160, 200));
                    canvas.draw_text("Settings Screen", Point {{ x: 60.0, y: 60.0 }}, TEXT, &font, 24.0);
                    let hb = mouse_x >= 60.0 && mouse_x <= 200.0 && mouse_y >= 400.0 && mouse_y <= 440.0;
                    draw_button(canvas, &font, 60.0, 400.0, 140.0, "← Back", hb);
                }}
                None => {{}}
            }}
        }});
}}
"#, title = title)
}

fn form_app_main_rs(name: &str) -> String {
    let title = name.to_uppercase();
    format!(r#"use tezzera_core::types::{{Point, Rect, Size}};
use tezzera_platform::{{InputEvent, Key, MouseButton, TezzeraApp}};
use tezzera_render::{{Color, FontCache, SkiaCanvas}};
use tezzera_state::use_atom;

const W: u32 = 480;
const H: u32 = 400;
const BG:     Color = Color::rgb(18, 18, 28);
const ACCENT: Color = Color::rgb(103, 80, 164);
const TEXT:   Color = Color::rgb(230, 225, 229);
const MUTED:  Color = Color::rgb(140, 145, 175);
const ERROR:  Color = Color::rgb(207, 102, 121);
const SURFACE:Color = Color::rgb(28, 30, 46);

fn main() {{
    let font = FontCache::system_mono().expect("no font");
    let email = use_atom(String::new());
    let password = use_atom(String::new());
    let active_field = use_atom(0u8); // 0=none, 1=email, 2=password
    let error_msg = use_atom(String::new());
    let submitted = use_atom(false);
    let mx = use_atom(0.0_f32);
    let my = use_atom(0.0_f32);

    TezzeraApp::new()
        .title("{title} — Form App")
        .size(W, H)
        .run(move |canvas, events| {{
            for ev in events {{
                match ev {{
                    InputEvent::MouseMove {{ x, y }} => {{ mx.set(*x); my.set(*y); }}
                    InputEvent::MouseDown {{ x, y, button: MouseButton::Left }} => {{
                        if *y >= 140.0 && *y <= 172.0 {{ active_field.set(1); }}
                        else if *y >= 220.0 && *y <= 252.0 {{ active_field.set(2); }}
                        else {{ active_field.set(0); }}
                        // Submit button
                        if *x >= 100.0 && *x <= 380.0 && *y >= 300.0 && *y <= 340.0 {{
                            let e = email.get();
                            let p = password.get();
                            if e.is_empty() {{ error_msg.set("Email is required.".into()); }}
                            else if !e.contains('@') {{ error_msg.set("Invalid email address.".into()); }}
                            else if p.len() < 6 {{ error_msg.set("Password must be 6+ characters.".into()); }}
                            else {{ error_msg.set(String::new()); submitted.set(true); }}
                        }}
                    }}
                    InputEvent::KeyDown {{ key, .. }} => {{
                        match key {{
                            Key::Backspace => {{
                                if active_field.get() == 1 {{ email.update(|mut s| {{ s.pop(); s }}); }}
                                else if active_field.get() == 2 {{ password.update(|mut s| {{ s.pop(); s }}); }}
                            }}
                            Key::Character(c) => {{
                                if active_field.get() == 1 {{ email.update(|mut s| {{ s.push(*c); s }}); }}
                                else if active_field.get() == 2 {{ password.update(|mut s| {{ s.push(*c); s }}); }}
                            }}
                            _ => {{}}
                        }}
                    }}
                    _ => {{}}
                }}
            }}

            canvas.clear(BG);
            canvas.draw_text("{title}", Point {{ x: 100.0, y: 40.0 }}, TEXT, &font, 22.0);
            canvas.draw_text("Sign In", Point {{ x: 100.0, y: 80.0 }}, MUTED, &font, 14.0);

            // Email field
            canvas.draw_text("Email", Point {{ x: 100.0, y: 120.0 }}, MUTED, &font, 12.0);
            let email_border = if active_field.get() == 1 {{ ACCENT }} else {{ Color::rgb(60, 65, 90) }};
            canvas.stroke_rect(Rect {{ origin: Point {{ x: 100.0, y: 138.0 }}, size: Size {{ width: 280.0, height: 36.0 }} }}, email_border, 1.5);
            canvas.draw_text(&email.get(), Point {{ x: 108.0, y: 150.0 }}, TEXT, &font, 14.0);

            // Password field
            canvas.draw_text("Password", Point {{ x: 100.0, y: 200.0 }}, MUTED, &font, 12.0);
            let pass_border = if active_field.get() == 2 {{ ACCENT }} else {{ Color::rgb(60, 65, 90) }};
            canvas.stroke_rect(Rect {{ origin: Point {{ x: 100.0, y: 218.0 }}, size: Size {{ width: 280.0, height: 36.0 }} }}, pass_border, 1.5);
            let masked: String = "*".repeat(password.get().len());
            canvas.draw_text(&masked, Point {{ x: 108.0, y: 230.0 }}, TEXT, &font, 14.0);

            // Error
            if !error_msg.get().is_empty() {{
                canvas.draw_text(&error_msg.get(), Point {{ x: 100.0, y: 276.0 }}, ERROR, &font, 12.0);
            }}

            // Submit button
            let hovered = mx.get() >= 100.0 && mx.get() <= 380.0 && my.get() >= 300.0 && my.get() <= 340.0;
            canvas.fill_rect(Rect {{ origin: Point {{ x: 100.0, y: 300.0 }}, size: Size {{ width: 280.0, height: 40.0 }} }},
                if hovered {{ Color::rgb(130, 100, 200) }} else {{ ACCENT }});
            canvas.draw_text(if submitted.get() {{ "✓ Signed In" }} else {{ "Sign In" }},
                Point {{ x: 180.0, y: 312.0 }}, Color::WHITE, &font, 14.0);
        }});
}}
"#, title = title)
}

fn dashboard_main_rs(name: &str) -> String {
    let title = name.to_uppercase();
    format!(r#"use tezzera_core::types::{{Point, Rect, Size}};
use tezzera_platform::{{InputEvent, TezzeraApp}};
use tezzera_render::{{Color, FontCache, SkiaCanvas}};

const W: u32 = 800;
const H: u32 = 600;
const BG:      Color = Color::rgb(12, 13, 20);
const SURFACE: Color = Color::rgb(22, 24, 36);
const BORDER:  Color = Color::rgb(40, 44, 64);
const TEXT:    Color = Color::rgb(230, 225, 229);
const MUTED:   Color = Color::rgb(120, 125, 155);

struct Metric {{ label: &'static str, value: &'static str, color: Color }}

fn draw_card(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, w: f32, h: f32, m: &Metric) {{
    c.fill_rect(Rect {{ origin: Point {{ x, y }}, size: Size {{ width: w, height: h }} }}, SURFACE);
    c.stroke_rect(Rect {{ origin: Point {{ x, y }}, size: Size {{ width: w, height: h }} }}, BORDER, 1.0);
    c.fill_rect(Rect {{ origin: Point {{ x, y }}, size: Size {{ width: w, height: 3.0 }} }}, m.color);
    c.draw_text(m.value, Point {{ x: x + 16.0, y: y + 20.0 }}, m.color, font, 28.0);
    c.draw_text(m.label, Point {{ x: x + 16.0, y: y + 58.0 }}, MUTED, font, 12.0);
}}

fn main() {{
    let font = FontCache::system_mono().expect("no font");
    let metrics = [
        Metric {{ label: "Total Users",    value: "12,847",  color: Color::rgb(103,  80, 164) }},
        Metric {{ label: "Revenue",        value: "$48,290", color: Color::rgb( 72, 199, 116) }},
        Metric {{ label: "Active Sessions",value: "3,291",   color: Color::rgb(100, 160, 255) }},
        Metric {{ label: "Error Rate",     value: "0.12%",   color: Color::rgb(207, 102, 121) }},
    ];

    TezzeraApp::new()
        .title("{title} — Dashboard")
        .size(W, H)
        .run(move |canvas, _events| {{
            canvas.clear(BG);
            canvas.draw_text("{title}", Point {{ x: 40.0, y: 32.0 }}, TEXT, &font, 20.0);
            canvas.draw_text("Dashboard", Point {{ x: 40.0, y: 60.0 }}, MUTED, &font, 12.0);

            let card_w = (W as f32 - 80.0 - 30.0) / 4.0;
            for (i, m) in metrics.iter().enumerate() {{
                draw_card(canvas, &font, 40.0 + i as f32 * (card_w + 10.0), 100.0, card_w, 90.0, m);
            }}
        }});
}}
"#, title = title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn new_opts_parses_name() {
        let args = vec!["my_app".to_string()];
        let opts = NewOptions::from_args(&args).unwrap();
        assert_eq!(opts.name, "my_app");
    }

    #[test]
    fn new_opts_errors_on_missing_name() {
        assert!(NewOptions::from_args(&[]).is_err());
    }

    #[test]
    fn new_opts_rejects_invalid_chars() {
        let args = vec!["my app!".to_string()];
        assert!(NewOptions::from_args(&args).is_err());
    }

    #[test]
    fn cargo_toml_contains_name() {
        let toml = cargo_toml_for_template("hello_world", &Template::Counter);
        assert!(toml.contains("hello_world"));
        assert!(toml.contains("tezzera-platform"));
    }

    #[test]
    fn main_rs_contains_title() {
        let src = main_rs("my_app");
        assert!(src.contains("MY_APP"));
        assert!(src.contains("use_atom"));
    }

    #[test]
    fn run_creates_directory_and_files() {
        let name = format!("_test_new_{}", std::process::id());
        let opts = NewOptions { name: name.clone(), template: Template::Counter };
        run(opts).unwrap();
        assert!(std::path::Path::new(&name).join("Cargo.toml").exists());
        assert!(std::path::Path::new(&name).join("src/main.rs").exists());
        assert!(std::path::Path::new(&name).join(".gitignore").exists());
        fs::remove_dir_all(&name).unwrap();
    }

    #[test]
    fn run_errors_if_dir_exists() {
        let name = format!("_test_exists_{}", std::process::id());
        fs::create_dir(&name).unwrap();
        let result = run(NewOptions { name: name.clone(), template: Template::Counter });
        assert!(result.is_err());
        fs::remove_dir_all(&name).unwrap();
    }

    // --- Template enum tests ---

    #[test]
    fn new_opts_default_template_is_counter() {
        let args = vec!["my_app".to_string()];
        let opts = NewOptions::from_args(&args).unwrap();
        assert_eq!(opts.template, Template::Counter);
    }

    #[test]
    fn new_opts_template_nav_app() {
        let args = vec!["my_app".to_string(), "--template".to_string(), "nav-app".to_string()];
        let opts = NewOptions::from_args(&args).unwrap();
        assert_eq!(opts.template, Template::NavApp);
    }

    #[test]
    fn new_opts_template_form_app() {
        let args = vec!["my_app".to_string(), "--template".to_string(), "form-app".to_string()];
        let opts = NewOptions::from_args(&args).unwrap();
        assert_eq!(opts.template, Template::FormApp);
    }

    #[test]
    fn new_opts_template_dashboard() {
        let args = vec!["my_app".to_string(), "--template".to_string(), "dashboard".to_string()];
        let opts = NewOptions::from_args(&args).unwrap();
        assert_eq!(opts.template, Template::Dashboard);
    }

    #[test]
    fn new_opts_unknown_template_errors() {
        let args = vec!["my_app".to_string(), "--template".to_string(), "unknown-thing".to_string()];
        assert!(NewOptions::from_args(&args).is_err());
    }

    #[test]
    fn template_from_str_counter() {
        assert_eq!(Template::from_str("counter").unwrap(), Template::Counter);
    }

    #[test]
    fn template_from_str_nav_app() {
        assert_eq!(Template::from_str("nav-app").unwrap(), Template::NavApp);
    }

    #[test]
    fn template_from_str_unknown() {
        assert!(Template::from_str("does-not-exist").is_err());
    }

    #[test]
    fn template_name_counter() {
        assert_eq!(Template::Counter.name(), "counter");
    }

    #[test]
    fn template_name_nav_app() {
        assert_eq!(Template::NavApp.name(), "nav-app");
    }

    #[test]
    fn cargo_toml_nav_app_has_nav_dep() {
        let toml = cargo_toml_for_template("my_app", &Template::NavApp);
        assert!(toml.contains("tezzera-nav"));
    }

    #[test]
    fn cargo_toml_counter_no_nav_dep() {
        let toml = cargo_toml_for_template("my_app", &Template::Counter);
        assert!(!toml.contains("tezzera-nav"));
    }

    #[test]
    fn nav_app_main_contains_navigator() {
        let src = nav_app_main_rs("my_nav");
        assert!(src.contains("Navigator"));
        assert!(src.contains("MY_NAV"));
    }

    #[test]
    fn form_app_main_contains_sign_in() {
        let src = form_app_main_rs("my_form");
        assert!(src.contains("Sign In"));
        assert!(src.contains("MY_FORM"));
    }

    #[test]
    fn dashboard_main_contains_metrics() {
        let src = dashboard_main_rs("my_dash");
        assert!(src.contains("metrics"));
        assert!(src.contains("MY_DASH"));
    }

    #[test]
    fn run_creates_nav_app() {
        let name = format!("_test_nav_{}", std::process::id());
        let opts = NewOptions { name: name.clone(), template: Template::NavApp };
        run(opts).unwrap();
        let cargo = fs::read_to_string(std::path::Path::new(&name).join("Cargo.toml")).unwrap();
        let main = fs::read_to_string(std::path::Path::new(&name).join("src/main.rs")).unwrap();
        assert!(cargo.contains("tezzera-nav"));
        assert!(main.contains("Navigator"));
        fs::remove_dir_all(&name).unwrap();
    }

    #[test]
    fn new_opts_template_equals_syntax() {
        let args = vec!["my_app".to_string(), "--template=form-app".to_string()];
        let opts = NewOptions::from_args(&args).unwrap();
        assert_eq!(opts.template, Template::FormApp);
    }
}
