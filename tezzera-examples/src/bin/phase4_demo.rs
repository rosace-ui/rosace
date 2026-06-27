//! Phase 4 Demo — 1400×900 static PNG showcasing Phase 4 systems.
//!
//! Four panels (340×820 each, 10px gaps):
//!   Panel 1 — Widget Gallery    (checkboxes, switches, sliders, badges, chips, avatars)
//!   Panel 2 — Forms Panel       (email / name / password with validation errors)
//!   Panel 3 — Navigation Panel  (navigator stack diagram)
//!   Panel 4 — Avatar & Badge Showcase (grid of avatars with overlaid badges)
//!
//! Run:    cargo run -p tezzera-examples --bin phase4_demo
//! Output: phase4_demo.png  (1400×900)

use tezzera_core::types::{Point, Rect, Size};
use tezzera_forms::{Form, FormField, Email, MinLength, Required};
use tezzera_nav::{Navigator, Route};
use tezzera_render::{Color, FontCache, SkiaCanvas};
use tezzera_widgets::{Button, ButtonVariant};

// ── Canvas dimensions ─────────────────────────────────────────────────────────
const W: u32 = 1400;
const H: u32 = 900;

// ── Palette ───────────────────────────────────────────────────────────────────
const BG: Color      = Color::rgb(12,  12,  20);
const PANEL_BG: Color = Color::rgb(20,  22,  36);
const BORDER: Color  = Color::rgb(44,  48,  72);
const ACCENT: Color  = Color::rgb(103, 80,  164);
const ACCENT2: Color = Color::rgb(140, 120, 200);
const TEXT_HI: Color = Color::rgb(200, 195, 220);
const TEXT_LO: Color = Color::rgb(120, 115, 145);
const ERROR_C: Color = Color::rgb(200, 80,  80);
const GREEN: Color   = Color::rgb(72,  199, 116);
const SURFACE: Color = Color::rgb(28,  30,  46);

// ── Panel layout ──────────────────────────────────────────────────────────────
// Header: 60px, panels: y=60 h=820, status bar: y=880 h=20
// 4 panels × 340 + 3 gaps × 10 = 1390 → left margin = 5
const PANEL_W: f32 = 340.0;
const PANEL_H: f32 = 820.0;
const PANEL_Y: f32 = 60.0;
const GAP: f32     = 10.0;
const MARGIN: f32  = 5.0;

fn panel_x(n: usize) -> f32 {
    MARGIN + n as f32 * (PANEL_W + GAP)
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn panel_bg(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, w: f32, h: f32, title: &str) {
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: h } }, PANEL_BG);
    c.stroke_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: h } }, BORDER, 1.0);
    // Accent top stripe
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: 3.0 } }, ACCENT);
    c.draw_text(title, Point { x: x + 14.0, y: y + 10.0 }, TEXT_HI, font, 13.0);
}

fn label(c: &mut SkiaCanvas, font: &FontCache, text: &str, x: f32, y: f32) {
    c.draw_text(text, Point { x, y }, TEXT_LO, font, 9.0);
}

// ── Manual widget renderers ───────────────────────────────────────────────────

/// Draw a checkbox (18×18 square, optionally checked / indeterminate) + label.
fn draw_checkbox(
    c: &mut SkiaCanvas,
    font: &FontCache,
    x: f32,
    y: f32,
    state: u8, // 0=unchecked, 1=checked, 2=indeterminate
    lbl: &str,
) {
    let size = 16.0_f32;
    let bg = if state > 0 { ACCENT } else { SURFACE };
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: size, height: size } }, bg);
    c.stroke_rect(Rect { origin: Point { x, y }, size: Size { width: size, height: size } }, ACCENT, 1.5);
    match state {
        1 => {
            // Checkmark: two line segments simulated as thin rects
            c.fill_rect(
                Rect { origin: Point { x: x + 3.0, y: y + 8.0 }, size: Size { width: 4.0, height: 2.0 } },
                Color::WHITE,
            );
            c.fill_rect(
                Rect { origin: Point { x: x + 6.0, y: y + 5.0 }, size: Size { width: 2.0, height: 6.0 } },
                Color::WHITE,
            );
        }
        2 => {
            // Indeterminate dash
            c.fill_rect(
                Rect { origin: Point { x: x + 3.0, y: y + 7.0 }, size: Size { width: 10.0, height: 2.0 } },
                Color::WHITE,
            );
        }
        _ => {}
    }
    c.draw_text(lbl, Point { x: x + size + 8.0, y: y + 2.0 }, TEXT_HI, font, 12.0);
}

/// Draw a switch (pill shape via two rects + circle) with label.
fn draw_switch(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, on: bool, lbl: &str) {
    let track_w = 34.0_f32;
    let track_h = 18.0_f32;
    let track_color = if on { ACCENT } else { SURFACE };
    let border_color = if on { ACCENT } else { BORDER };

    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: track_w, height: track_h } }, track_color);
    c.stroke_rect(Rect { origin: Point { x, y }, size: Size { width: track_w, height: track_h } }, border_color, 1.0);

    let thumb_x = if on { x + track_w - 14.0 } else { x + 2.0 };
    let thumb_y = y + 2.0;
    c.fill_rect(Rect { origin: Point { x: thumb_x, y: thumb_y }, size: Size { width: 14.0, height: 14.0 } }, Color::WHITE);

    c.draw_text(lbl, Point { x: x + track_w + 8.0, y: y + 2.0 }, TEXT_HI, font, 12.0);
}

/// Draw a horizontal slider track + filled portion + thumb.
fn draw_slider(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, w: f32, value: f32, show_label: bool) {
    let track_h = 4.0_f32;
    let ty = y + 8.0;
    // Track background
    c.fill_rect(Rect { origin: Point { x, y: ty }, size: Size { width: w, height: track_h } }, SURFACE);
    c.stroke_rect(Rect { origin: Point { x, y: ty }, size: Size { width: w, height: track_h } }, BORDER, 1.0);
    // Filled portion
    let filled_w = (w * value).max(0.0);
    if filled_w > 0.0 {
        c.fill_rect(Rect { origin: Point { x, y: ty }, size: Size { width: filled_w, height: track_h } }, ACCENT);
    }
    // Thumb
    let thumb_x = x + filled_w - 6.0;
    c.fill_circle(Point { x: thumb_x + 6.0, y: ty + track_h / 2.0 }, 7.0, ACCENT);
    c.fill_circle(Point { x: thumb_x + 6.0, y: ty + track_h / 2.0 }, 5.0, ACCENT2);

    if show_label {
        let pct = format!("{:.0}%", value * 100.0);
        c.draw_text(&pct, Point { x: x + w + 8.0, y: y }, TEXT_LO, font, 10.0);
    }
}

/// Draw a progress bar.
fn draw_progress(c: &mut SkiaCanvas, x: f32, y: f32, w: f32, value: f32, indeterminate: bool, pulse_phase: f32) {
    let h = 8.0_f32;
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: h } }, SURFACE);
    c.stroke_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: h } }, BORDER, 1.0);
    if indeterminate {
        // Pulsing segment
        let seg_w = w * 0.4;
        let seg_x = x + (w - seg_w) * pulse_phase;
        c.fill_rect(
            Rect { origin: Point { x: seg_x.max(x), y }, size: Size { width: seg_w.min(w), height: h } },
            Color::rgba(ACCENT.r, ACCENT.g, ACCENT.b, 200),
        );
    } else {
        let filled = (w * value.clamp(0.0, 1.0)).max(0.0);
        c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: filled, height: h } }, ACCENT);
    }
}

/// Draw a circular badge (count or dot) at (x, y).
fn draw_badge(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, count: Option<u32>) {
    let r = if count.is_some() { 10.0_f32 } else { 5.0_f32 };
    let bg = Color::rgb(180, 30, 30);
    c.fill_circle(Point { x, y }, r, bg);
    if let Some(n) = count {
        let lbl = format!("{}", n.min(99));
        let lbl_x = x - lbl.len() as f32 * 3.5;
        c.draw_text(&lbl, Point { x: lbl_x, y: y - 5.0 }, Color::WHITE, font, 9.0);
    }
}

/// Draw a chip pill shape.
fn draw_chip(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, label_text: &str, selected: bool, dismissible: bool) {
    let tw = label_text.len() as f32 * 7.5;
    let extra = if dismissible { 18.0 } else { 0.0 };
    let cw = tw + 20.0 + extra;
    let ch = 24.0_f32;
    let bg = if selected { ACCENT } else { SURFACE };
    let border = if selected { ACCENT2 } else { BORDER };
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: cw, height: ch } }, bg);
    c.stroke_rect(Rect { origin: Point { x, y }, size: Size { width: cw, height: ch } }, border, 1.0);
    c.draw_text(label_text, Point { x: x + 10.0, y: y + 6.0 }, TEXT_HI, font, 11.0);
    if dismissible {
        c.draw_text("\u{00D7}", Point { x: x + cw - 14.0, y: y + 5.0 }, TEXT_LO, font, 12.0);
    }
}

/// Draw a circular avatar with initials.
fn draw_avatar(c: &mut SkiaCanvas, font: &FontCache, cx: f32, cy: f32, r: f32, initials: &str, badge_count: Option<u32>) {
    c.fill_circle(Point { x: cx, y: cy }, r, ACCENT);
    let char_w = 8.0_f32;
    let tx = cx - initials.len() as f32 * char_w / 2.0;
    let ty = cy - 6.0;
    let fs = if r >= 24.0 { 14.0 } else { 10.0 };
    c.draw_text(initials, Point { x: tx, y: ty }, Color::WHITE, font, fs);
    // Ring
    c.stroke_rect(
        Rect { origin: Point { x: cx - r, y: cy - r }, size: Size { width: r * 2.0, height: r * 2.0 } },
        Color::rgba(ACCENT2.r, ACCENT2.g, ACCENT2.b, 100),
        1.5,
    );
    if let Some(n) = badge_count {
        draw_badge(c, font, cx + r * 0.7, cy - r * 0.7, Some(n));
    }
}

// ── Panel 1 — Widget Gallery ──────────────────────────────────────────────────

fn panel_widget_gallery(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, w: f32, h: f32) {
    panel_bg(c, font, x, y, w, h, "Widget Gallery");

    let ix = x + 14.0;
    let mut cy = y + 30.0;

    // ── Checkboxes ──
    label(c, font, "Checkboxes", ix, cy);
    cy += 14.0;
    draw_checkbox(c, font, ix,          cy, 0, "Subscribe");
    cy += 26.0;
    draw_checkbox(c, font, ix,          cy, 1, "Agree to terms");
    cy += 26.0;
    draw_checkbox(c, font, ix,          cy, 2, "Select all");
    cy += 32.0;

    // ── Switches ──
    label(c, font, "Switches", ix, cy);
    cy += 14.0;
    draw_switch(c, font, ix, cy, false, "Notifications");
    cy += 28.0;
    draw_switch(c, font, ix, cy, true,  "Dark mode");
    cy += 34.0;

    // ── Sliders ──
    label(c, font, "Sliders", ix, cy);
    cy += 14.0;
    let slider_w = w - 60.0;
    draw_slider(c, font, ix, cy, slider_w, 0.35, true);
    cy += 28.0;
    draw_slider(c, font, ix, cy, slider_w, 0.70, true);
    cy += 34.0;

    // ── Progress Bars ──
    label(c, font, "Progress Bars", ix, cy);
    cy += 14.0;
    draw_progress(c, ix, cy, slider_w, 0.6, false, 0.0);
    cy += 18.0;
    draw_progress(c, ix, cy, slider_w, 0.0, true, 0.4);
    cy += 26.0;

    // ── Badges ──
    label(c, font, "Badges", ix, cy);
    cy += 16.0;
    draw_badge(c, font, ix + 12.0, cy, Some(5));
    c.draw_text("count=5", Point { x: ix + 26.0, y: cy - 6.0 }, TEXT_LO, font, 10.0);
    draw_badge(c, font, ix + 120.0, cy, None);
    c.draw_text("dot", Point { x: ix + 130.0, y: cy - 5.0 }, TEXT_LO, font, 10.0);
    cy += 30.0;

    // ── Chips ──
    label(c, font, "Chips", ix, cy);
    cy += 14.0;
    draw_chip(c, font, ix,            cy, "Rust",  true,  false);
    draw_chip(c, font, ix + 66.0,     cy, "WASM",  false, false);
    draw_chip(c, font, ix + 126.0,    cy, "UI",    false, true);
    cy += 36.0;

    // ── Avatars ──
    label(c, font, "Avatars", ix, cy);
    cy += 20.0;
    draw_avatar(c, font, ix + 24.0,  cy, 20.0, "GJ", None);
    draw_avatar(c, font, ix + 76.0,  cy, 24.0, "AK", None);
    c.draw_text("GJ (sm)", Point { x: ix + 8.0, y: cy + 26.0 }, TEXT_LO, font, 8.0);
    c.draw_text("AK (md)", Point { x: ix + 58.0, y: cy + 32.0 }, TEXT_LO, font, 8.0);
}

// ── Panel 2 — Forms Panel ─────────────────────────────────────────────────────

fn panel_forms(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, w: f32, h: f32) {
    panel_bg(c, font, x, y, w, h, "Forms Panel");

    let ix = x + 14.0;
    let field_w = w - 28.0;
    let mut cy = y + 30.0;

    // Build and validate a form using tezzera-forms
    let mut form = Form::new()
        .field(FormField::new("email").rule(Required).rule(Email))
        .field(FormField::new("name").rule(Required))
        .field(FormField::new("password").rule(Required).rule(MinLength(8)));

    form.field_named_mut("email").unwrap().set("not-an-email");
    form.field_named_mut("name").unwrap().set("");
    form.field_named_mut("password").unwrap().set("abc");
    let _ = form.validate_all();

    // ── Helper: draw one form field ──
    let field_defs: &[(&str, &str, &str)] = &[
        ("Email",    "not-an-email", "Must be a valid email address."),
        ("Name",     "",             "This field is required."),
        ("Password", "abc",          "Must be at least 8 characters."),
    ];

    for (field_lbl, value, default_err) in field_defs {
        // Label
        c.draw_text(field_lbl, Point { x: ix, y: cy }, TEXT_LO, font, 10.0);
        cy += 14.0;

        // Input rect (simulate text input)
        let input_h = 32.0_f32;
        c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: field_w, height: input_h } }, SURFACE);
        c.stroke_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: field_w, height: input_h } }, ERROR_C, 1.5);
        let display_val = if value.is_empty() { "\u{00A0}" } else { value };
        c.draw_text(display_val, Point { x: ix + 8.0, y: cy + 9.0 }, TEXT_HI, font, 12.0);
        cy += input_h + 4.0;

        // Error message (use actual form errors where possible, else default)
        let field_name = field_lbl.to_lowercase();
        let err_text = form
            .errors()
            .iter()
            .find(|e| e.field == field_name)
            .map(|e| e.message.as_str())
            .unwrap_or(default_err);

        c.draw_text(
            &format!("\u{26A0} {}", err_text),
            Point { x: ix, y: cy },
            ERROR_C,
            font,
            10.0,
        );
        cy += 22.0;

        // Spacing between fields
        c.fill_rect(
            Rect { origin: Point { x: ix, y: cy }, size: Size { width: field_w, height: 1.0 } },
            Color::rgba(BORDER.r, BORDER.g, BORDER.b, 80),
        );
        cy += 14.0;
    }

    // ── Form summary ──
    cy += 8.0;
    let errors = form.errors();
    let summary = format!("Errors: {}   Valid: {}", errors.len(), form.validate_all());
    c.draw_text(&summary, Point { x: ix, y: cy }, TEXT_LO, font, 10.0);
    cy += 18.0;

    c.draw_text("tezzera-forms validation in action", Point { x: ix, y: cy }, TEXT_LO, font, 9.0);
    cy += 14.0;
    c.draw_text("Required / Email / MinLength rules", Point { x: ix, y: cy }, TEXT_LO, font, 9.0);
    cy += 28.0;

    // ── Submit button (visual only) ──
    let btn_w = field_w;
    let btn_h = 40.0_f32;
    c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: btn_w, height: btn_h } }, Color::rgba(ACCENT.r, ACCENT.g, ACCENT.b, 80));
    c.stroke_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: btn_w, height: btn_h } }, BORDER, 1.0);
    c.draw_text("Submit (disabled \u{2014} form invalid)", Point { x: ix + 12.0, y: cy + 12.0 }, TEXT_LO, font, 11.0);
}

// ── Panel 3 — Navigation Panel ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum DemoScreen { Home, Profile, Settings }
impl Route for DemoScreen {}

fn panel_navigation(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, w: f32, h: f32) {
    panel_bg(c, font, x, y, w, h, "Navigation Panel");

    let ix = x + 14.0;
    let mut cy = y + 32.0;

    // Build a real Navigator and push screens
    let nav: Navigator<DemoScreen> = Navigator::new(DemoScreen::Home);
    nav.push(DemoScreen::Profile);
    nav.push(DemoScreen::Settings);

    let depth = nav.depth();
    let stack = nav.stack();

    label(c, font, "Navigator<Screen> stack (bottom \u{2192} top)", ix, cy);
    cy += 14.0;

    // Draw each entry in the stack as a box
    let box_w = w - 60.0;
    let box_h = 44.0_f32;
    let box_gap = 8.0_f32;

    for (i, screen) in stack.iter().enumerate() {
        let bx = ix + 16.0;
        let by = cy + i as f32 * (box_h + box_gap);
        let is_current = i == stack.len() - 1;

        let bg = if is_current { ACCENT } else { SURFACE };
        let border = if is_current { ACCENT2 } else { BORDER };
        c.fill_rect(Rect { origin: Point { x: bx, y: by }, size: Size { width: box_w, height: box_h } }, bg);
        c.stroke_rect(Rect { origin: Point { x: bx, y: by }, size: Size { width: box_w, height: box_h } }, border, 1.5);

        let screen_name = match screen {
            DemoScreen::Home     => "Home",
            DemoScreen::Profile  => "Profile",
            DemoScreen::Settings => "Settings",
        };
        let label_text = if is_current {
            format!("{} \u{2190} current", screen_name)
        } else {
            screen_name.to_string()
        };
        c.draw_text(&label_text, Point { x: bx + 12.0, y: by + 14.0 }, Color::WHITE, font, 13.0);

        // Index badge on right
        let idx_label = format!("[{}]", i);
        c.draw_text(&idx_label, Point { x: bx + box_w - 30.0, y: by + 14.0 }, TEXT_LO, font, 11.0);

        // Arrow up between boxes
        if i < stack.len() - 1 {
            let arr_y = by + box_h + 2.0;
            c.fill_rect(
                Rect { origin: Point { x: bx + box_w / 2.0 - 1.0, y: arr_y }, size: Size { width: 2.0, height: box_gap - 4.0 } },
                BORDER,
            );
        }
    }
    cy += stack.len() as f32 * (box_h + box_gap) + 10.0;

    // Stats
    let depth_label = format!("Depth: {}   can_go_back: {}", depth, nav.can_go_back());
    c.draw_text(&depth_label, Point { x: ix, y: cy }, TEXT_LO, font, 10.0);
    cy += 16.0;

    c.draw_text("\u{2190} pop()  takes Settings off the stack", Point { x: ix, y: cy }, TEXT_LO, font, 9.0);
    cy += 14.0;
    c.draw_text("\u{2190} pop()  takes Profile off the stack", Point { x: ix, y: cy }, TEXT_LO, font, 9.0);
    cy += 28.0;

    // ── Route info ──
    label(c, font, "Public API used in nav_demo.rs", ix, cy);
    cy += 14.0;
    let api_lines = [
        "Navigator::new(Screen::Home)",
        "nav.push(Screen::Profile)",
        "nav.pop()",
        "nav.current() -> Option<Screen>",
        "nav.depth() -> usize",
        "nav.can_go_back() -> bool",
        "nav.stack() -> Vec<Screen>",
    ];
    for line in &api_lines {
        c.draw_text(line, Point { x: ix + 8.0, y: cy }, TEXT_LO, font, 9.0);
        cy += 13.0;
    }
    cy += 8.0;

    // ── Can-go-back indicator ──
    let back_color = if nav.can_go_back() { GREEN } else { ERROR_C };
    c.draw_text(
        if nav.can_go_back() { "\u{2714} can go back" } else { "\u{2718} at root" },
        Point { x: ix, y: cy },
        back_color,
        font,
        11.0,
    );
}

// ── Panel 4 — Avatar & Badge Showcase ────────────────────────────────────────

fn panel_avatar_showcase(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32, w: f32, h: f32) {
    panel_bg(c, font, x, y, w, h, "Avatar & Badge Showcase");

    let ix = x + 14.0;
    let mut cy = y + 32.0;

    // ── Size variation row ──
    label(c, font, "Avatar sizes (with badges)", ix, cy);
    cy += 18.0;

    let initials = ["TZ", "GJ", "AK", "UI", "RU", "ST"];
    let radii = [16.0_f32, 20.0, 24.0, 28.0];
    let radius_labels = ["32px", "40px", "48px", "56px"];
    let badge_counts = [1u32, 3, 7, 12];

    // One row per radius size
    for (ri, &r) in radii.iter().enumerate() {
        let row_y = cy + ri as f32 * (r * 2.0 + 34.0);
        // Size label
        c.draw_text(radius_labels[ri], Point { x: ix, y: row_y + r - 6.0 }, TEXT_LO, font, 9.0);

        for (ai, init) in initials.iter().enumerate() {
            let ax = ix + 38.0 + ai as f32 * (r * 2.0 + 12.0);
            let ay = row_y + r;
            draw_avatar(c, font, ax, ay, r, init, Some(badge_counts[ai % badge_counts.len()]));
            c.draw_text(*init, Point { x: ax - 6.0, y: ay + r + 6.0 }, TEXT_LO, font, 8.0);
        }
    }

    cy += radii.iter().map(|&r| r * 2.0 + 34.0).sum::<f32>() + 10.0;

    // ── Dot-badge row ──
    label(c, font, "Dot badges (status indicators)", ix, cy);
    cy += 16.0;

    let status_colors = [
        Color::rgb(72, 199, 116),   // online
        Color::rgb(255, 165, 55),   // away
        Color::rgb(200, 80, 80),    // busy
        Color::rgb(120, 115, 145),  // offline
    ];
    let status_labels = ["Online", "Away", "Busy", "Offline"];

    for (si, (sc, sl)) in status_colors.iter().zip(status_labels.iter()).enumerate() {
        let ax = ix + 24.0 + si as f32 * 72.0;
        draw_avatar(c, font, ax, cy + 20.0, 18.0, initials[si], None);
        // Status dot (colored, overrides badge)
        c.fill_circle(Point { x: ax + 13.0, y: cy + 7.0 }, 6.0, *sc);
        c.fill_circle(Point { x: ax + 13.0, y: cy + 7.0 }, 4.0, *sc);
        c.draw_text(*sl, Point { x: ax - 10.0, y: cy + 44.0 }, TEXT_LO, font, 8.0);
    }
    cy += 60.0;

    // ── Button widget showcase (using tezzera_widgets::Button) ──
    label(c, font, "Widget Button variants", ix, cy);
    cy += 14.0;
    let variants: &[(&str, ButtonVariant)] = &[
        ("Primary",   ButtonVariant::Primary),
        ("Secondary", ButtonVariant::Secondary),
        ("Ghost",     ButtonVariant::Ghost),
    ];
    for (vi, (lbl_text, variant)) in variants.iter().enumerate() {
        let bx = ix;
        let by = cy + vi as f32 * 46.0;
        Button::new(*lbl_text)
            .variant(*variant)
            .width(w - 28.0)
            .height(36.0)
            .render(c, font, bx, by);
    }
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() {
    let font = FontCache::system_mono().expect("system font not found");
    let mut c = SkiaCanvas::new(W, H);

    // ── Global background ──
    c.clear(BG);

    // ── Header ──
    c.fill_rect(Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: W as f32, height: 60.0 } }, Color::rgb(18, 18, 30));
    c.draw_text("TEZZERA  Phase 4 Demo", Point { x: 20.0, y: 18.0 }, TEXT_HI, &font, 18.0);
    c.draw_text(
        "Widgets \u{00B7} Forms \u{00B7} Nav \u{00B7} Avatars",
        Point { x: W as f32 - 260.0, y: 22.0 },
        TEXT_LO, &font, 12.0,
    );
    c.fill_rect(Rect { origin: Point { x: 0.0, y: 57.0 }, size: Size { width: W as f32, height: 3.0 } }, ACCENT);

    // ── Panels ──
    panel_widget_gallery  (&mut c, &font, panel_x(0), PANEL_Y, PANEL_W, PANEL_H);
    panel_forms           (&mut c, &font, panel_x(1), PANEL_Y, PANEL_W, PANEL_H);
    panel_navigation      (&mut c, &font, panel_x(2), PANEL_Y, PANEL_W, PANEL_H);
    panel_avatar_showcase (&mut c, &font, panel_x(3), PANEL_Y, PANEL_W, PANEL_H);

    // ── Status bar ──
    let sb_y = 880.0_f32;
    c.fill_rect(Rect { origin: Point { x: 0.0, y: sb_y }, size: Size { width: W as f32, height: 20.0 } }, Color::rgb(14, 14, 24));
    c.draw_text(
        "PHASE 4  \u{00B7}  Widgets \u{2713}  \u{00B7}  Forms \u{2713}  \u{00B7}  Nav \u{2713}  \u{00B7}  Avatars \u{2713}",
        Point { x: 20.0, y: sb_y + 4.0 },
        TEXT_LO,
        &font,
        10.0,
    );

    // ── Encode and write ──
    let png = c.encode_png().expect("png encode failed");
    std::fs::write("phase4_demo.png", &png).expect("write phase4_demo.png");
    println!("Saved phase4_demo.png ({}x{})", W, H);
}
