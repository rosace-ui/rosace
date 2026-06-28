//! Phase 10 Demo — 1400×900 static PNG showcasing Phase 10 systems.
//!
//! Four panels arranged 2×2:
//!   Panel 1 (top-left)     — Animation    (Easing curves, Tween<f32>, Timeline keyframes)
//!   Panel 2 (top-right)    — Accessibility (A11yTree node graph, FocusManager tab order)
//!   Panel 3 (bottom-left)  — Test Harness  (WidgetEnv, EventSim, SnapshotAssert flow)
//!   Panel 4 (bottom-right) — Package CLI   (tzr package flow, PackageManifest JSON)
//!
//! Run:    cargo run -p tezzera-examples --bin phase10_demo
//! Output: phase10_demo.png (1400×900)

use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::{Color, FontCache, SkiaCanvas};

// ── Phase 10 crate imports ────────────────────────────────────────────────────

// Animation
use tezzera_anim::{easing_fn, Easing, Keyframe, Timeline, Tween};

// Accessibility
use tezzera_a11y::{A11yNode, A11yTree, FocusManager, Role};

// Test utils
use tezzera_test_utils::{EventSim, SnapshotAssert, WidgetEnv};

// ── Canvas dimensions ─────────────────────────────────────────────────────────
const W: u32 = 1400;
const H: u32 = 900;

// Layout
const HEADER_H: f32 = 72.0;
const PANEL_W:  f32 = W as f32 / 2.0;
const PANEL_H:  f32 = (H as f32 - HEADER_H - 14.0) / 2.0;

// ── Color palette ─────────────────────────────────────────────────────────────
const BG:          Color = Color::rgb(10,  12,  20);
const PANEL_BG:    Color = Color::rgb(16,  18,  30);
const DIVIDER:     Color = Color::rgb(40,  44,  64);
const ACCENT:      Color = Color::rgb(107,  80, 200);
const ACCENT2:     Color = Color::rgb( 72, 199, 116);
const ACCENT3:     Color = Color::rgb(255, 160,  60);
const ACCENT4:     Color = Color::rgb( 80, 180, 255);
const TEXT_PRIMARY:Color = Color::rgb(230, 232, 250);
const TEXT_MUTED:  Color = Color::rgb(110, 115, 145);
const TEXT_DIM:    Color = Color::rgb( 70,  74, 100);
const CHIP_DARK:   Color = Color::rgb( 22,  26,  44);

// ── Helpers ───────────────────────────────────────────────────────────────────

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { origin: Point { x, y }, size: Size { width: w, height: h } }
}
fn p(x: f32, y: f32) -> Point { Point { x, y } }

fn text(c: &mut SkiaCanvas, font: &FontCache, s: &str, x: f32, y: f32, color: Color, px: f32) {
    c.draw_text(s, p(x, y), color, font, px);
}

fn chip(c: &mut SkiaCanvas, font: &FontCache, label: &str, x: f32, y: f32, accent: Color) {
    c.fill_rect(r(x, y, label.len() as f32 * 6.8 + 10.0, 16.0), CHIP_DARK);
    c.fill_rect(r(x, y, 3.0, 16.0), accent);
    text(c, font, label, x + 6.0, y + 4.0, TEXT_PRIMARY, 9.0);
}

fn section_label(c: &mut SkiaCanvas, font: &FontCache, label: &str, x: f32, y: f32) {
    text(c, font, label, x, y, TEXT_MUTED, 9.0);
    c.fill_rect(r(x, y + 13.0, label.len() as f32 * 5.6, 1.0), TEXT_DIM);
}

fn bullet(c: &mut SkiaCanvas, font: &FontCache, s: &str, x: f32, y: f32) {
    c.fill_rect(r(x, y + 4.0, 3.0, 3.0), ACCENT);
    text(c, font, s, x + 8.0, y, TEXT_PRIMARY, 10.0);
}

// ── Panel 1 — Animation ───────────────────────────────────────────────────────

fn panel_anim(c: &mut SkiaCanvas, font: &FontCache, ox: f32, oy: f32) {
    c.fill_rect(r(ox, oy, PANEL_W, PANEL_H), PANEL_BG);
    text(c, font, "Animation", ox + 16.0, oy + 14.0, ACCENT, 13.0);
    text(c, font, "tezzera-anim  \u{2014}  Easing \u{2022} Tween \u{2022} AnimationController \u{2022} Timeline",
         ox + 16.0, oy + 30.0, TEXT_MUTED, 9.0);

    let cx = ox + 16.0;
    let mut cy = oy + 52.0;

    // ── Easing curve chart ────────────────────────────────────────────────────
    section_label(c, font, "EASING CURVES  (sampled t=0..1)", cx, cy); cy += 18.0;

    let easings = [
        (Easing::Linear,      "Linear",      TEXT_MUTED),
        (Easing::EaseIn,      "EaseIn",      ACCENT),
        (Easing::EaseOut,     "EaseOut",     ACCENT2),
        (Easing::EaseInOut,   "EaseInOut",   ACCENT3),
        (Easing::CubicBezier(0.68, -0.55, 0.265, 1.55), "CubicBezier", ACCENT4),
        (Easing::Spring { stiffness: 200.0, damping: 15.0 }, "Spring",    Color::rgb(220, 100, 180)),
    ];

    let chart_w = PANEL_W - 32.0;
    let chart_h = 80.0;
    let steps   = 60usize;

    c.fill_rect(r(cx, cy, chart_w, chart_h), Color::rgb(12, 14, 24));

    for (easing, label, color) in &easings {
        for i in 0..steps {
            let t  = i as f32 / (steps - 1) as f32;
            let v  = easing_fn(*easing, t).clamp(0.0, 1.3);
            let px = cx + t * chart_w;
            let py = cy + chart_h - v * chart_h * 0.9 - 4.0;
            c.fill_rect(r(px, py, 2.0, 2.0), *color);
        }
        let last_t  = 1.0_f32;
        let last_v  = easing_fn(*easing, last_t).clamp(0.0, 1.3);
        let lx = cx + last_t * chart_w + 4.0;
        let ly = cy + chart_h - last_v * chart_h * 0.9 - 8.0;
        text(c, font, label, lx.min(cx + chart_w - 72.0), ly.clamp(cy, cy + chart_h - 10.0), *color, 8.0);
    }
    cy += chart_h + 12.0;

    // ── Tween<f32> samples ────────────────────────────────────────────────────
    section_label(c, font, "TWEEN<f32>  from=0  to=100  dur=1.0s  EaseInOut", cx, cy); cy += 18.0;

    let tween = Tween::new(0.0_f32, 100.0_f32, 1.0, Easing::EaseInOut);
    let samples: Vec<(f32, f32)> = (0..=10)
        .map(|i| { let t = i as f32 / 10.0; (t, tween.sample(t)) })
        .collect();

    let bar_total = chart_w;
    for (i, (t, v)) in samples.iter().enumerate() {
        let bx = cx + i as f32 * (bar_total / 10.0);
        let bh = (v / 100.0) * 36.0;
        c.fill_rect(r(bx, cy + 36.0 - bh, 12.0, bh), ACCENT);
        text(c, font, &format!("{:.0}", v), bx, cy + 40.0, TEXT_MUTED, 7.5);
        if i == 0 || i == 5 || i == 10 {
            text(c, font, &format!("t={:.1}", t), bx - 2.0, cy + 50.0, TEXT_DIM, 7.0);
        }
    }
    cy += 66.0;

    // ── Timeline keyframes ────────────────────────────────────────────────────
    section_label(c, font, "TIMELINE<f32>  3 keyframes", cx, cy); cy += 18.0;

    let timeline: Timeline<f32> = Timeline::new()
        .add(Keyframe::new(0.0,  0.0,   Easing::Linear))
        .add(Keyframe::new(0.5, 80.0,   Easing::EaseOut))
        .add(Keyframe::new(1.0, 50.0,   Easing::EaseInOut));

    let kf_steps = 40usize;
    let kf_w = chart_w;
    for i in 0..kf_steps {
        let t = i as f32 / (kf_steps - 1) as f32;
        let v = timeline.sample(t);
        let px = cx + t * kf_w;
        let py = cy + 28.0 - (v / 100.0) * 24.0;
        c.fill_rect(r(px, py, 3.0, 3.0), ACCENT2);
    }
    let kf_data = [(0.0_f32, 0.0_f32, "t=0 v=0"), (0.5, 80.0, "t=0.5 v=80"), (1.0, 50.0, "t=1 v=50")];
    for (t, v, lbl) in &kf_data {
        let px = cx + t * kf_w;
        let py = cy + 28.0 - (v / 100.0) * 24.0;
        c.fill_rect(r(px - 3.0, py - 3.0, 6.0, 6.0), ACCENT3);
        text(c, font, lbl, px + 4.0, py - 10.0, TEXT_MUTED, 8.0);
    }
}

// ── Panel 2 — Accessibility ───────────────────────────────────────────────────

fn panel_a11y(c: &mut SkiaCanvas, font: &FontCache, ox: f32, oy: f32) {
    c.fill_rect(r(ox, oy, PANEL_W, PANEL_H), PANEL_BG);
    text(c, font, "Accessibility", ox + 16.0, oy + 14.0, ACCENT2, 13.0);
    text(c, font, "tezzera-a11y  \u{2014}  A11yTree \u{2022} A11yNode \u{2022} Role \u{2022} FocusManager",
         ox + 16.0, oy + 30.0, TEXT_MUTED, 9.0);

    let cx = ox + 16.0;
    let mut cy = oy + 52.0;

    // ── Node graph ────────────────────────────────────────────────────────────
    section_label(c, font, "A11Y TREE  (Dialog \u{2192} Button\u{00d7}3, Checkbox, TextInput)", cx, cy);
    cy += 18.0;

    let mut tree = A11yTree::new(0);
    tree.add_node(A11yNode::new(0, Role::Dialog).with_label("Settings"));
    tree.add_child(0, A11yNode::new(1, Role::Button).with_label("Save"));
    tree.add_child(0, A11yNode::new(2, Role::Button).with_label("Cancel"));
    tree.add_child(0, A11yNode::new(3, Role::Checkbox).with_label("Remember me").with_checked(false));
    tree.add_child(0, A11yNode::new(4, Role::TextInput).with_label("Username"));
    tree.add_child(0, A11yNode::new(5, Role::Button).with_label("Help").with_disabled(true));

    // Root node box
    let root_x = cx + 180.0;
    let root_y = cy;
    c.fill_rect(r(root_x - 4.0, root_y - 2.0, 88.0, 18.0), ACCENT2);
    text(c, font, "Dialog [0]", root_x, root_y + 2.0, Color::rgb(10, 12, 20), 9.5);
    text(c, font, "\"Settings\"", root_x + 2.0, root_y + 12.0, Color::rgb(10, 12, 20), 7.5);

    // Child nodes
    let node_data: &[(u64, &str, &str, bool, Color)] = &[
        (1, "Button [1]",    "\"Save\"",        true,  ACCENT),
        (2, "Button [2]",    "\"Cancel\"",       true,  ACCENT),
        (3, "Checkbox [3]",  "\"Remember me\"",  true,  ACCENT3),
        (4, "TextInput [4]", "\"Username\"",     true,  ACCENT4),
        (5, "Button [5]",    "\"Help\" disabled",false, TEXT_DIM),
    ];

    let child_start_y = root_y + 32.0;
    let child_gap = 26.0;
    let child_x = cx;

    for (i, (id, role, label, focusable, color)) in node_data.iter().enumerate() {
        let ny = child_start_y + i as f32 * child_gap;
        // Arrow from root to child
        let mid_x = child_x + 130.0;
        c.fill_rect(r(child_x + 130.0, ny + 6.0, root_x - 4.0 - mid_x, 1.0), TEXT_DIM);
        c.fill_rect(r(root_x + 40.0, root_y + 18.0, 1.0, ny + 6.0 - root_y - 18.0), TEXT_DIM);
        // Node chip
        c.fill_rect(r(child_x, ny - 1.0, 125.0, 16.0), CHIP_DARK);
        c.fill_rect(r(child_x, ny - 1.0, 3.0, 16.0), *color);
        text(c, font, role, child_x + 6.0, ny + 2.0, *color, 8.5);
        text(c, font, label, child_x + 6.0, ny + 10.0, TEXT_MUTED, 7.5);
        // Focusable badge
        if *focusable {
            c.fill_rect(r(child_x + 128.0, ny + 1.0, 34.0, 12.0), Color::rgb(20, 40, 25));
            text(c, font, "focus", child_x + 130.0, ny + 3.0, ACCENT2, 7.5);
        }
        let _ = id;
    }
    cy += child_start_y - root_y + node_data.len() as f32 * child_gap + 14.0;

    // ── Focus manager ─────────────────────────────────────────────────────────
    section_label(c, font, "FOCUS MANAGER  tab order (BFS, interactive only)", cx, cy); cy += 18.0;

    let mut fm = FocusManager::new();
    fm.sync(&tree);

    let order = fm.tab_order();
    for (i, id) in order.iter().enumerate() {
        let lx = cx + i as f32 * 82.0;
        c.fill_rect(r(lx, cy, 76.0, 18.0), CHIP_DARK);
        c.fill_rect(r(lx, cy, 3.0, 18.0), ACCENT2);
        text(c, font, &format!("Tab {}", i + 1), lx + 6.0, cy + 2.0, ACCENT2, 8.0);
        text(c, font, &format!("id={}", id), lx + 6.0, cy + 11.0, TEXT_MUTED, 7.5);
    }
    cy += 30.0;

    // Focus next preview
    fm.focus_next();
    let focused = fm.focused;
    text(c, font,
         &format!("focus_next() \u{2192} {:?}   focus_count() = {}", focused, fm.focus_count()),
         cx, cy, TEXT_PRIMARY, 9.5);
    cy += 16.0;

    // Role stats
    let buttons  = tree.find_by_role(Role::Button);
    let inputs   = tree.find_by_role(Role::TextInput);
    let checks   = tree.find_by_role(Role::Checkbox);
    text(c, font,
         &format!("find_by_role: Button\u{00d7}{}  TextInput\u{00d7}{}  Checkbox\u{00d7}{}  node_count={}",
                  buttons.len(), inputs.len(), checks.len(), tree.node_count()),
         cx, cy, TEXT_MUTED, 9.0);
}

// ── Panel 3 — Test Harness ────────────────────────────────────────────────────

fn panel_test(c: &mut SkiaCanvas, font: &FontCache, ox: f32, oy: f32) {
    c.fill_rect(r(ox, oy, PANEL_W, PANEL_H), PANEL_BG);
    text(c, font, "Test Harness", ox + 16.0, oy + 14.0, ACCENT3, 13.0);
    text(c, font, "tezzera-test-utils  \u{2014}  WidgetEnv \u{2022} EventSim \u{2022} SnapshotAssert",
         ox + 16.0, oy + 30.0, TEXT_MUTED, 9.0);

    let cx = ox + 16.0;
    let mut cy = oy + 52.0;

    // ── WidgetEnv ─────────────────────────────────────────────────────────────
    section_label(c, font, "WIDGET ENV  (headless canvas)", cx, cy); cy += 18.0;

    let mut env = WidgetEnv::new(160, 40);
    env.clear(Color::rgb(20, 22, 36));
    env.render_text("Hello World", 6.0, 22.0, 14.0);
    let canvas_px = env.pixel_at(0, 0);
    let png = env.encode_png();

    // Show mini canvas preview
    let preview_scale = 2.0_f32;
    c.fill_rect(r(cx, cy, env.width() as f32 * preview_scale, env.height() as f32 * preview_scale),
                Color::rgb(20, 22, 36));
    c.fill_rect(r(cx, cy, env.width() as f32 * preview_scale, 1.0), TEXT_DIM);
    c.fill_rect(r(cx, cy + env.height() as f32 * preview_scale - 1.0,
                   env.width() as f32 * preview_scale, 1.0), TEXT_DIM);
    text(c, font, "Hello World", cx + 8.0, cy + 16.0, TEXT_PRIMARY, 12.0);

    let px_info = format!(
        "{}x{}px  pixel_at(0,0)=({},{},{})  PNG {} bytes",
        env.width(), env.height(),
        canvas_px.r, canvas_px.g, canvas_px.b,
        png.len()
    );
    text(c, font, &px_info, cx + env.width() as f32 * preview_scale + 12.0, cy + 8.0, TEXT_MUTED, 9.0);
    cy += env.height() as f32 * preview_scale + 10.0;

    // ── EventSim ──────────────────────────────────────────────────────────────
    section_label(c, font, "EVENT SIM  (input simulation)", cx, cy); cy += 18.0;

    let tap    = EventSim::tap(80.0, 20.0);
    let typed  = EventSim::type_text("Hi");
    let scroll = EventSim::scroll(0.0, 0.0, -3.0);

    let sim_rows: &[(&str, usize, Color)] = &[
        ("tap(80, 20)          →  MouseDown + MouseUp", tap.len(), ACCENT3),
        ("type_text(\"Hi\")    →  KeyDown+Text+KeyUp ×2", typed.len(), ACCENT3),
        ("scroll(0,0, -3.0)    →  Scroll { delta_y: -3.0 }", scroll.len(), ACCENT3),
    ];
    for (desc, count, color) in sim_rows {
        c.fill_rect(r(cx, cy - 1.0, 3.0, 13.0), *color);
        text(c, font, desc, cx + 6.0, cy, TEXT_PRIMARY, 9.5);
        text(c, font, &format!("{} event(s)", count), cx + 360.0, cy, TEXT_MUTED, 9.0);
        cy += 16.0;
    }
    cy += 6.0;

    // ── SnapshotAssert ────────────────────────────────────────────────────────
    section_label(c, font, "SNAPSHOT ASSERT  (pixel comparison)", cx, cy); cy += 18.0;

    let flow: &[(&str, Color)] = &[
        ("1. render widget \u{2192} encode_png() \u{2192} Vec<u8>", ACCENT4),
        ("2. save_snapshot(name, png)  \u{2192}  test_snapshots/<name>.png", ACCENT4),
        ("3. on next run: assert_snapshot(name, png)", ACCENT4),
        ("4. pixel_diff_count(baseline, current)  \u{2264}  threshold", ACCENT4),
        ("5. PANIC if diff > threshold \u{2022} PASS otherwise", ACCENT2),
    ];
    for (step, color) in flow {
        bullet(c, font, step, cx, cy);
        let _ = color;
        cy += 14.0;
    }

    // pixel_diff demo
    cy += 4.0;
    let a = vec![0u8, 0, 0, 255, 255, 0];
    let b = vec![0u8, 0, 0, 255, 128, 0];
    let diff = SnapshotAssert::pixel_diff_count(&a, &b);
    text(c, font,
         &format!("pixel_diff_count([0,0,0,255,255,0], [0,0,0,255,128,0]) = {} byte(s)", diff),
         cx, cy, TEXT_MUTED, 9.0);
}

// ── Panel 4 — Package CLI ─────────────────────────────────────────────────────

fn panel_package(c: &mut SkiaCanvas, font: &FontCache, ox: f32, oy: f32) {
    c.fill_rect(r(ox, oy, PANEL_W, PANEL_H), PANEL_BG);
    text(c, font, "Package CLI", ox + 16.0, oy + 14.0, ACCENT4, 13.0);
    text(c, font, "tezzera-cli  \u{2014}  tzr package \u{2022} PackageConfig \u{2022} PackageManifest",
         ox + 16.0, oy + 30.0, TEXT_MUTED, 9.0);

    let cx = ox + 16.0;
    let mut cy = oy + 52.0;

    // ── Command ───────────────────────────────────────────────────────────────
    section_label(c, font, "TZR PACKAGE  COMMAND", cx, cy); cy += 18.0;

    let cmd_str = "$ tzr package --name my-app --version 1.0.0 --out dist/";
    c.fill_rect(r(cx, cy - 2.0, PANEL_W - 32.0, 18.0), Color::rgb(8, 10, 18));
    text(c, font, cmd_str, cx + 4.0, cy + 1.0, ACCENT4, 9.5);
    cy += 24.0;

    // ── Flow diagram ──────────────────────────────────────────────────────────
    section_label(c, font, "PACKAGE FLOW", cx, cy); cy += 18.0;

    let steps: &[(&str, Color)] = &[
        ("\u{25b6}  parse PackageConfig { name, version, out_dir }", ACCENT4),
        ("\u{25b6}  cargo build --release --workspace", ACCENT4),
        ("\u{25b6}  collect binary paths from target/release/", ACCENT4),
        ("\u{25b6}  write PackageManifest \u{2192} <out_dir>/manifest.json", ACCENT4),
        ("\u{2713}  CommandResult { exit_code: 0, success: true }", ACCENT2),
    ];
    for (desc, color) in steps {
        c.fill_rect(r(cx, cy - 1.0, 3.0, 13.0), *color);
        text(c, font, desc, cx + 8.0, cy, TEXT_PRIMARY, 9.5);
        cy += 16.0;
    }
    cy += 8.0;

    // ── Manifest JSON ─────────────────────────────────────────────────────────
    section_label(c, font, "PACKAGE MANIFEST  (manifest.json)", cx, cy); cy += 18.0;

    let json_lines = [
        "{",
        "  \"name\": \"my-app\",",
        "  \"version\": \"1.0.0\",",
        "  \"target\": \"desktop\",",
        "  \"crates\": [\"tezzera-core\", \"tezzera-render\", ...],",
        "  \"examples\": [\"phase10_demo\"],",
        "  \"built_at\": \"2026-06-28T00:00:00Z\"",
        "}",
    ];
    c.fill_rect(r(cx, cy - 2.0, PANEL_W - 32.0, json_lines.len() as f32 * 14.0 + 8.0),
                Color::rgb(8, 10, 18));
    for line in &json_lines {
        text(c, font, line, cx + 8.0, cy + 1.0, ACCENT2, 9.5);
        cy += 14.0;
    }
    cy += 12.0;

    // ── Phase 10 crate chips ──────────────────────────────────────────────────
    section_label(c, font, "PHASE 10 CRATES", cx, cy); cy += 18.0;

    let p10_crates: &[(&str, Color)] = &[
        ("tezzera-anim",       ACCENT),
        ("tezzera-a11y",       ACCENT2),
        ("tezzera-test-utils", ACCENT3),
        ("tzr-package",        ACCENT4),
    ];
    let mut chip_x = cx;
    for (name, color) in p10_crates {
        chip(c, font, name, chip_x, cy, *color);
        chip_x += name.len() as f32 * 6.8 + 18.0;
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let font = FontCache::system_mono().expect("no system mono font");
    let mut c = SkiaCanvas::new(W, H);
    c.clear(BG);

    // Header
    c.fill_rect(r(0.0, 0.0, W as f32, 3.0), ACCENT);
    text(&mut c, &font, "TEZZERA \u{2014} Phase 10 Showcase",
         28.0, 22.0, TEXT_PRIMARY, 20.0);
    text(&mut c, &font,
         "Animation  \u{2022}  Accessibility  \u{2022}  Test Harness  \u{2022}  Package CLI",
         28.0, 50.0, TEXT_MUTED, 10.0);

    // Panels
    let top_y    = HEADER_H;
    let bottom_y = HEADER_H + PANEL_H;
    panel_anim   (&mut c, &font, 0.0,     top_y);
    panel_a11y   (&mut c, &font, PANEL_W, top_y);
    panel_test   (&mut c, &font, 0.0,     bottom_y);
    panel_package(&mut c, &font, PANEL_W, bottom_y);

    // Grid lines
    c.fill_rect(r(PANEL_W, HEADER_H, 1.0, PANEL_H * 2.0), DIVIDER);
    c.fill_rect(r(0.0, bottom_y, W as f32, 1.0), DIVIDER);
    c.fill_rect(r(0.0, HEADER_H - 1.0, W as f32, 1.0), DIVIDER);

    // Status bar
    let sb_y = H as f32 - 14.0;
    c.fill_rect(r(0.0, sb_y - 4.0, W as f32, 18.0), Color::rgb(8, 10, 16));
    text(&mut c, &font,
         "TEZZERA  \u{2022}  Phase 10  \u{2022}  Anim \u{2713}  A11y \u{2713}  Test Harness \u{2713}  Package CLI \u{2713}",
         W as f32 / 2.0 - 280.0, sb_y - 1.0, TEXT_MUTED, 10.0);

    let png = c.encode_png().expect("png encode failed");
    std::fs::write("phase10_demo.png", &png).expect("write phase10_demo.png");
    println!("Saved phase10_demo.png ({}x{})", W, H);
}
