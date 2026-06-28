//! Phase 7 Demo — 1400×900 static PNG showcasing Phase 7 systems.
//!
//! Four panels (350 wide each, y=80–900):
//!   Panel 1 — Glyph Metrics   (measure_text, measure_text_heuristic)
//!   Panel 2 — RTL Text        (detect_direction, reverse_words, TextDirection)
//!   Panel 3 — Clipboard       (NoopClipboard, ClipboardProvider)
//!   Panel 4 — WebSocket+Pinch (WsError, WsMessage, WsState, WsStream, PinchRecognizer)
//!
//! Run:    cargo run -p tezzera-examples --bin phase7_demo
//! Output: phase7_demo.png (1400×900)

use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::{Color, FontCache, SkiaCanvas};

// ── Phase 7 crate imports ─────────────────────────────────────────────────────

// Glyph metrics
use tezzera_text::{measure_text, measure_text_heuristic};

// RTL text
use tezzera_text::{detect_direction, reverse_words, TextDirection};

// Clipboard
use tezzera_clipboard::{ClipboardProvider, NoopClipboard};

// WebSocket
use tezzera_ws::{WsError, WsMessage, WsState, WsStream};

// Pinch recognizer
use tezzera_gesture::{GestureRecognizer, PinchRecognizer};
use tezzera_platform::InputEvent;

// ── Canvas dimensions ─────────────────────────────────────────────────────────
const W: u32 = 1400;
const H: u32 = 900;

// ── Color palette ─────────────────────────────────────────────────────────────
const BG:           Color = Color::rgb(10,  12,  20);
const PANEL_BG:     Color = Color::rgb(16,  18,  30);
const DIVIDER:      Color = Color::rgb(40,  44,  64);
const ACCENT:       Color = Color::rgb(107, 80, 200);
const ACCENT2:      Color = Color::rgb( 72, 199, 116);
const ACCENT3:      Color = Color::rgb(255, 160,  60);
const ACCENT4:      Color = Color::rgb( 80, 180, 255);
const ACCENT5:      Color = Color::rgb(255,  80,  80);
const TEXT_PRIMARY: Color = Color::rgb(230, 230, 245);
const TEXT_MUTED:   Color = Color::rgb(120, 125, 155);
const CARD_BG:      Color = Color::rgb(22,  24,  40);

// ── Layout ────────────────────────────────────────────────────────────────────
const HEADER_H: f32 = 80.0;
const PANEL_W:  f32 = 350.0;
const PANEL_H:  f32 = 820.0;

fn px(n: usize) -> f32 { n as f32 * PANEL_W }

// ── Shared helpers ────────────────────────────────────────────────────────────

fn lbl(c: &mut SkiaCanvas, font: &FontCache, text: &str, x: f32, y: f32) {
    c.draw_text(text, Point { x, y }, TEXT_MUTED, font, 10.0);
}

fn section_label(c: &mut SkiaCanvas, font: &FontCache, text: &str, x: f32, y: f32, color: Color) {
    c.draw_text(text, Point { x, y }, color, font, 13.0);
}

fn card_box(c: &mut SkiaCanvas, x: f32, y: f32, w: f32, h: f32, fill: Color, stroke: Color, stroke_w: f32) {
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: h } }, fill);
    c.stroke_rect(Rect { origin: Point { x, y }, size: Size { width: w, height: h } }, stroke, stroke_w);
}

// ── Panel 1 — Glyph Metrics ──────────────────────────────────────────────────

fn panel_glyph_metrics(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: PANEL_W, height: PANEL_H } }, PANEL_BG);

    let ix = x + 20.0;
    let mut cy = y + 14.0;

    section_label(c, font, "Glyph Metrics", ix, cy, ACCENT);
    cy += 20.0;

    lbl(c, font, "fontdue advance_width vs. heuristic (font_size=14)", ix, cy);
    cy += 18.0;

    // Column headers
    c.draw_text("String", Point { x: ix,          y: cy }, TEXT_MUTED, font, 10.0);
    c.draw_text("Heuristic",  Point { x: ix + 80.0, y: cy }, TEXT_MUTED, font, 10.0);
    c.draw_text("fontdue",    Point { x: ix + 170.0, y: cy }, TEXT_MUTED, font, 10.0);
    c.draw_text("\u{0394}",   Point { x: ix + 265.0, y: cy }, TEXT_MUTED, font, 10.0);
    cy += 14.0;

    // Divider line
    c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: 310.0, height: 1.0 } }, DIVIDER);
    cy += 8.0;

    let samples = ["Hello World", "TEZZERA", "fig", "WWW", "iii"];
    let font_size = 14.0_f32;
    let max_bar_w = 130.0_f32;

    // Pre-compute max width for bar scaling
    let max_w = samples.iter()
        .map(|s| {
            let h = measure_text_heuristic(s, font_size);
            let r = measure_text(s, font_size, font);
            h.max(r)
        })
        .fold(1.0_f32, f32::max);

    for sample in &samples {
        let heuristic_w = measure_text_heuristic(sample, font_size);
        let real_w      = measure_text(sample, font_size, font);
        let delta       = (real_w - heuristic_w).abs();

        // String label
        c.draw_text(sample, Point { x: ix, y: cy }, TEXT_PRIMARY, font, 10.0);
        cy += 13.0;

        // Heuristic bar (TEXT_MUTED)
        let hbar = (heuristic_w / max_w * max_bar_w).max(2.0);
        c.fill_rect(
            Rect { origin: Point { x: ix, y: cy }, size: Size { width: hbar, height: 10.0 } },
            TEXT_MUTED,
        );
        c.draw_text(
            &format!("{:.1}px", heuristic_w),
            Point { x: ix + hbar + 3.0, y: cy + 1.0 },
            TEXT_MUTED, font, 9.0,
        );

        // fontdue bar (ACCENT), offset to right column
        let rbar = (real_w / max_w * max_bar_w).max(2.0);
        let col2_x = ix + 160.0;
        c.fill_rect(
            Rect { origin: Point { x: col2_x, y: cy }, size: Size { width: rbar, height: 10.0 } },
            ACCENT,
        );
        c.draw_text(
            &format!("{:.1}px", real_w),
            Point { x: col2_x + rbar + 3.0, y: cy + 1.0 },
            ACCENT, font, 9.0,
        );

        cy += 13.0;

        // Delta note
        let delta_color = if delta < 5.0 { ACCENT2 } else { ACCENT3 };
        c.draw_text(
            &format!("\u{0394} {:.1}px", delta),
            Point { x: ix + 265.0, y: cy - 13.0 },
            delta_color, font, 9.0,
        );

        cy += 4.0;
    }

    cy += 8.0;

    // Legend
    card_box(c, ix, cy, 310.0, 48.0, CARD_BG, DIVIDER, 1.0);
    c.fill_rect(Rect { origin: Point { x: ix + 10.0, y: cy + 10.0 }, size: Size { width: 28.0, height: 10.0 } }, TEXT_MUTED);
    c.draw_text("heuristic (len \u{00D7} size \u{00D7} 0.55)", Point { x: ix + 44.0, y: cy + 10.0 }, TEXT_MUTED, font, 10.0);
    c.fill_rect(Rect { origin: Point { x: ix + 10.0, y: cy + 28.0 }, size: Size { width: 28.0, height: 10.0 } }, ACCENT);
    c.draw_text("fontdue advance_width sum", Point { x: ix + 44.0, y: cy + 28.0 }, ACCENT, font, 10.0);
    cy += 60.0;

    // Accuracy summary
    lbl(c, font, "Accuracy comparison at 14px:", ix, cy);
    cy += 16.0;

    let mut total_delta = 0.0_f32;
    for s in &samples {
        let h = measure_text_heuristic(s, font_size);
        let r = measure_text(s, font_size, font);
        total_delta += (r - h).abs();
    }
    let avg_delta = total_delta / samples.len() as f32;

    card_box(c, ix, cy, 310.0, 56.0, CARD_BG, ACCENT, 1.0);
    c.draw_text(
        &format!("Avg \u{0394}: {:.2}px over {} strings", avg_delta, samples.len()),
        Point { x: ix + 10.0, y: cy + 10.0 },
        TEXT_PRIMARY, font, 11.0,
    );
    c.draw_text(
        &format!("Total \u{0394}: {:.2}px", total_delta),
        Point { x: ix + 10.0, y: cy + 30.0 },
        TEXT_MUTED, font, 10.0,
    );
    cy += 68.0;

    // Single-char showcase
    lbl(c, font, "Single char advance widths at 16px:", ix, cy);
    cy += 16.0;
    let chars = ['W', 'i', 'M', 'l', 'f'];
    for ch in &chars {
        let (metrics, _) = font.rasterize(*ch, 16.0);
        let adv = metrics.advance_width;
        let bar_w = (adv / 20.0 * 80.0).max(2.0);
        c.draw_text(&format!("'{}'", ch), Point { x: ix, y: cy }, TEXT_PRIMARY, font, 10.0);
        c.fill_rect(Rect { origin: Point { x: ix + 20.0, y: cy + 1.0 }, size: Size { width: bar_w, height: 9.0 } }, ACCENT4);
        c.draw_text(
            &format!("{:.1}px", adv),
            Point { x: ix + 24.0 + bar_w, y: cy + 1.0 },
            TEXT_MUTED, font, 9.0,
        );
        cy += 16.0;
    }

    // Stats footer
    let footer_y = y + PANEL_H - 28.0;
    lbl(c, font, "65 tests \u{2022} fontdue advance_width", ix, footer_y);
}

// ── Panel 2 — RTL Text ────────────────────────────────────────────────────────

fn panel_rtl_text(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: PANEL_W, height: PANEL_H } }, PANEL_BG);

    let ix = x + 20.0;
    let mut cy = y + 14.0;

    section_label(c, font, "RTL Text Detection", ix, cy, ACCENT2);
    cy += 24.0;

    // ── Direction detection ───────────────────────────────────────────────────
    lbl(c, font, "detect_direction() samples:", ix, cy);
    cy += 16.0;

    let detection_samples: &[(&str, &str)] = &[
        ("Hello World",        "Latin phrase"),
        ("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} \u{0628}\u{0627}\u{0644}\u{0639}\u{0627}\u{0644}\u{0645}", "Arabic phrase"),
        ("\u{05E9}\u{05DC}\u{05D5}\u{05DD} \u{05E2}\u{05D5}\u{05DC}\u{05DD}", "Hebrew phrase"),
        ("TEZZERA v0.7",       "ASCII + version"),
    ];

    for (text, hint) in detection_samples {
        let dir = detect_direction(text);

        // Direction badge colors and label
        let (badge_color, dir_label) = match dir {
            TextDirection::Ltr  => (ACCENT2, "LTR"),
            TextDirection::Rtl  => (ACCENT3, "RTL"),
            TextDirection::Auto => (ACCENT4, "AUTO"),
        };

        let row_h = 50.0_f32;
        card_box(c, ix, cy, 310.0, row_h, CARD_BG, DIVIDER, 1.0);

        // Sample text (truncated for display, safe for multi-byte UTF-8)
        let display: String = text.chars().take(18).collect();
        let display = display.as_str();
        c.draw_text(display, Point { x: ix + 6.0, y: cy + 8.0 }, TEXT_PRIMARY, font, 11.0);
        c.draw_text(hint,    Point { x: ix + 6.0, y: cy + 28.0 }, TEXT_MUTED,   font, 9.0);

        // Direction badge (right side)
        let badge_x = ix + 220.0;
        c.fill_rect(
            Rect { origin: Point { x: badge_x, y: cy + 12.0 }, size: Size { width: 64.0, height: 22.0 } },
            badge_color,
        );
        c.draw_text(dir_label, Point { x: badge_x + 16.0, y: cy + 16.0 }, Color::rgb(10, 12, 20), font, 11.0);

        cy += row_h + 6.0;
    }

    cy += 10.0;

    // ── reverse_words demo ────────────────────────────────────────────────────
    c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: 310.0, height: 1.0 } }, DIVIDER);
    cy += 12.0;

    section_label(c, font, "reverse_words() demo:", ix, cy, ACCENT2);
    cy += 18.0;

    let input_str  = "one two three";
    let output_str = reverse_words(input_str);

    // Input box
    lbl(c, font, "INPUT", ix, cy);
    cy += 13.0;
    card_box(c, ix, cy, 310.0, 30.0, CARD_BG, DIVIDER, 1.0);
    c.draw_text(input_str, Point { x: ix + 10.0, y: cy + 9.0 }, TEXT_PRIMARY, font, 12.0);
    cy += 30.0;

    // Arrow
    c.draw_text("\u{2193}  reverse_words()", Point { x: ix + 80.0, y: cy + 4.0 }, ACCENT2, font, 10.0);
    cy += 22.0;

    // Output box
    lbl(c, font, "OUTPUT", ix, cy);
    cy += 13.0;
    card_box(c, ix, cy, 310.0, 30.0, CARD_BG, ACCENT2, 1.5);
    c.draw_text(&output_str, Point { x: ix + 10.0, y: cy + 9.0 }, ACCENT2, font, 12.0);
    cy += 38.0;

    // ── TextDirection::Auto explanation ───────────────────────────────────────
    c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: 310.0, height: 1.0 } }, DIVIDER);
    cy += 12.0;

    section_label(c, font, "TextDirection variants:", ix, cy, ACCENT2);
    cy += 18.0;

    let variants: &[(&str, Color, &str)] = &[
        ("Ltr",  ACCENT2, "Left-to-right (Latin default)"),
        ("Rtl",  ACCENT3, "Right-to-left (Arabic/Hebrew)"),
        ("Auto", ACCENT4, "Detect from first strong bidi char"),
    ];

    for (name, color, desc) in variants {
        card_box(c, ix, cy, 310.0, 42.0, CARD_BG, DIVIDER, 1.0);
        c.fill_rect(Rect { origin: Point { x: ix + 4.0, y: cy + 4.0 }, size: Size { width: 4.0, height: 34.0 } }, *color);
        c.draw_text(
            &format!("TextDirection::{}", name),
            Point { x: ix + 14.0, y: cy + 8.0 },
            *color, font, 11.0,
        );
        c.draw_text(desc, Point { x: ix + 14.0, y: cy + 26.0 }, TEXT_MUTED, font, 9.0);
        cy += 46.0;
    }

    cy += 4.0;

    // Unicode range info
    card_box(c, ix, cy, 310.0, 60.0, CARD_BG, DIVIDER, 1.0);
    c.draw_text("Unicode RTL ranges detected:", Point { x: ix + 8.0, y: cy + 6.0 }, TEXT_MUTED, font, 9.0);
    c.draw_text("Hebrew:  U+0590\u{2013}U+05FF", Point { x: ix + 8.0, y: cy + 20.0 }, ACCENT3, font, 9.0);
    c.draw_text("Arabic:  U+0600\u{2013}U+06FF", Point { x: ix + 8.0, y: cy + 34.0 }, ACCENT3, font, 9.0);
    c.draw_text("Ext-A:   U+08A0\u{2013}U+08FF", Point { x: ix + 8.0, y: cy + 48.0 }, ACCENT3, font, 9.0);

    // Stats footer
    let footer_y = y + PANEL_H - 28.0;
    lbl(c, font, "22+ tests \u{2022} Arabic/Hebrew Unicode ranges", ix, footer_y);
}

// ── Panel 3 — Clipboard ───────────────────────────────────────────────────────

fn panel_clipboard(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: PANEL_W, height: PANEL_H } }, PANEL_BG);

    let ix = x + 20.0;
    let mut cy = y + 14.0;

    section_label(c, font, "OS Clipboard", ix, cy, ACCENT3);
    cy += 24.0;

    // ── State machine diagram ─────────────────────────────────────────────────
    lbl(c, font, "Clipboard state machine:", ix, cy);
    cy += 14.0;

    let steps: &[(&str, &str, Color)] = &[
        ("EMPTY",          "Initial state",         TEXT_MUTED),
        ("WRITE(\"Hello\")", "cb.write(\"Hello\")", ACCENT3),
        ("READ",           "cb.read()",             ACCENT4),
        ("\"Hello\"",      "Returns value",         ACCENT2),
    ];

    let box_w = 260.0_f32;
    let box_h = 34.0_f32;
    let box_x = ix + 20.0;

    for (i, (label, hint, color)) in steps.iter().enumerate() {
        card_box(c, box_x, cy, box_w, box_h, CARD_BG, *color, if i == 0 { 1.0 } else { 1.5 });
        c.draw_text(label, Point { x: box_x + 10.0, y: cy + 8.0 }, *color, font, 11.0);
        c.draw_text(hint,  Point { x: box_x + 10.0, y: cy + 22.0 }, TEXT_MUTED, font, 9.0);
        cy += box_h;

        if i < steps.len() - 1 {
            // Arrow down
            let arrow_x = box_x + box_w / 2.0;
            c.fill_rect(
                Rect { origin: Point { x: arrow_x - 1.0, y: cy }, size: Size { width: 2.0, height: 14.0 } },
                DIVIDER,
            );
            c.fill_circle(Point { x: arrow_x, y: cy + 14.0 }, 3.0, DIVIDER);
            cy += 16.0;
        }
    }

    cy += 14.0;

    // ── Live NoopClipboard demo ───────────────────────────────────────────────
    c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: 310.0, height: 1.0 } }, DIVIDER);
    cy += 12.0;

    section_label(c, font, "Live demo (NoopClipboard):", ix, cy, ACCENT3);
    cy += 18.0;

    // Actually use the API
    let cb = NoopClipboard::new();
    cb.write("TEZZERA clipboard demo").unwrap();
    let value = cb.read().unwrap_or_default();

    // Code block
    let code_lines = [
        "let cb = NoopClipboard::new();",
        "cb.write(\"TEZZERA clipboard demo\")",
        "   .unwrap();",
        "let value = cb.read()",
        "   .unwrap_or_default();",
    ];
    card_box(c, ix, cy, 310.0, code_lines.len() as f32 * 14.0 + 8.0, CARD_BG, DIVIDER, 1.0);
    for (i, line) in code_lines.iter().enumerate() {
        c.draw_text(line, Point { x: ix + 6.0, y: cy + 4.0 + i as f32 * 14.0 }, TEXT_MUTED, font, 9.0);
    }
    cy += code_lines.len() as f32 * 14.0 + 16.0;

    // Result display
    lbl(c, font, "value =", ix, cy);
    cy += 13.0;
    card_box(c, ix, cy, 310.0, 28.0, Color::rgb(18, 28, 18), ACCENT2, 1.5);
    c.draw_text(&format!("\"{}\"", value), Point { x: ix + 8.0, y: cy + 8.0 }, ACCENT2, font, 11.0);
    cy += 36.0;

    // ── SystemClipboard info card ─────────────────────────────────────────────
    c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: 310.0, height: 1.0 } }, DIVIDER);
    cy += 10.0;

    lbl(c, font, "SystemClipboard platform support:", ix, cy);
    cy += 14.0;

    let platform_info: &[(&str, &str, Color)] = &[
        ("macOS", "pbcopy / pbpaste",  ACCENT4),
        ("Linux", "xclip / xsel",      ACCENT2),
        ("WASM",  "stub (NoopClipboard)", TEXT_MUTED),
    ];

    for (platform, cmd, color) in platform_info {
        card_box(c, ix, cy, 310.0, 30.0, CARD_BG, DIVIDER, 1.0);
        c.fill_rect(Rect { origin: Point { x: ix + 4.0, y: cy + 4.0 }, size: Size { width: 4.0, height: 22.0 } }, *color);
        c.draw_text(platform, Point { x: ix + 14.0, y: cy + 6.0 }, *color,     font, 10.0);
        c.draw_text(cmd,      Point { x: ix + 70.0,  y: cy + 6.0 }, TEXT_MUTED, font, 10.0);
        cy += 32.0;
    }

    // ── ClipboardError variants ───────────────────────────────────────────────
    cy += 4.0;
    lbl(c, font, "ClipboardError variants:", ix, cy);
    cy += 13.0;

    let errors: &[(&str, &str)] = &[
        ("Unavailable",         "clipboard unavailable"),
        ("CommandFailed(msg)",  "clipboard command failed: <msg>"),
        ("Unsupported",         "not supported on this platform"),
    ];

    for (variant, desc) in errors {
        card_box(c, ix, cy, 310.0, 32.0, CARD_BG, ACCENT5, 1.0);
        c.draw_text(variant, Point { x: ix + 8.0, y: cy + 6.0 },  ACCENT5,    font, 10.0);
        c.draw_text(desc,    Point { x: ix + 8.0, y: cy + 20.0 }, TEXT_MUTED, font, 9.0);
        cy += 34.0;
    }

    // Stats footer
    let footer_y = y + PANEL_H - 28.0;
    lbl(c, font, "22 tests \u{2022} macOS/Linux/WASM", ix, footer_y);
}

// ── Panel 4 — WebSocket + Pinch ───────────────────────────────────────────────

fn panel_ws_pinch(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: PANEL_W, height: PANEL_H } }, PANEL_BG);

    let ix = x + 20.0;
    let mut cy = y + 14.0;

    section_label(c, font, "WebSocket + Pinch", ix, cy, ACCENT4);
    cy += 22.0;

    // ── WsState flow diagram ──────────────────────────────────────────────────
    lbl(c, font, "WsState state machine:", ix, cy);
    cy += 14.0;

    let states: &[(&str, Color)] = &[
        ("Connecting", TEXT_MUTED),
        ("Open",       ACCENT2),
        ("Closing",    ACCENT3),
        ("Closed",     TEXT_MUTED),
        ("Error",      ACCENT5),
    ];

    let state_box_w = 80.0_f32;
    let state_box_h = 28.0_f32;
    let total_w = states.len() as f32 * state_box_w + (states.len() - 1) as f32 * 8.0;
    let start_x = ix + (310.0 - total_w) / 2.0;

    for (i, (label, color)) in states.iter().enumerate() {
        let sx = start_x + i as f32 * (state_box_w + 8.0);
        card_box(c, sx, cy, state_box_w, state_box_h, CARD_BG, *color, 1.5);
        let text_x = sx + (state_box_w - label.len() as f32 * 6.0) / 2.0;
        c.draw_text(label, Point { x: text_x, y: cy + 8.0 }, *color, font, 9.0);

        if i < states.len() - 1 {
            // Arrow →
            let arrow_x = sx + state_box_w + 1.0;
            let arrow_y = cy + state_box_h / 2.0;
            c.fill_rect(
                Rect { origin: Point { x: arrow_x, y: arrow_y - 1.0 }, size: Size { width: 5.0, height: 2.0 } },
                DIVIDER,
            );
            c.fill_circle(Point { x: arrow_x + 5.0, y: arrow_y }, 2.0, DIVIDER);
        }
    }
    cy += state_box_h + 14.0;

    // ── WsMessage type cards ──────────────────────────────────────────────────
    lbl(c, font, "WsMessage variants (RFC 6455):", ix, cy);
    cy += 13.0;

    // Use real WsMessage API
    let messages: &[(WsMessage, &str)] = &[
        (WsMessage::Text("hello".into()),    "opcode 0x01 — UTF-8 payload"),
        (WsMessage::Binary(vec![0x00, 0xFF]), "opcode 0x02 — raw bytes"),
        (WsMessage::Ping(vec![]),             "opcode 0x09 — keepalive"),
        (WsMessage::Pong(vec![]),             "opcode 0x0A — response to Ping"),
        (WsMessage::Close(Some(1000)),        "opcode 0x08 — normal closure"),
    ];

    for (msg, desc) in messages {
        let opcode = msg.opcode();
        let label = match msg {
            WsMessage::Text(s)   => format!("Text(\"{s}\")"),
            WsMessage::Binary(b) => format!("Binary([{} bytes])", b.len()),
            WsMessage::Ping(_)   => "Ping([])".to_string(),
            WsMessage::Pong(_)   => "Pong([])".to_string(),
            WsMessage::Close(c)  => format!("Close({})", c.map_or("None".to_string(), |v| v.to_string())),
        };
        let color = match msg {
            WsMessage::Text(_)   => ACCENT2,
            WsMessage::Binary(_) => ACCENT4,
            WsMessage::Ping(_)   => ACCENT,
            WsMessage::Pong(_)   => ACCENT3,
            WsMessage::Close(_)  => ACCENT5,
        };

        card_box(c, ix, cy, 310.0, 32.0, CARD_BG, color, 1.0);
        c.draw_text(&label, Point { x: ix + 6.0, y: cy + 6.0 }, color, font, 10.0);
        c.draw_text(
            &format!("0x{:02X}  {}", opcode, desc),
            Point { x: ix + 6.0, y: cy + 20.0 },
            TEXT_MUTED, font, 8.0,
        );
        cy += 34.0;
    }

    // ── WsStream::failed demo ─────────────────────────────────────────────────
    cy += 4.0;
    lbl(c, font, "WsStream::failed() demo:", ix, cy);
    cy += 13.0;

    let stream = WsStream::failed(WsError::Connect("localhost:9000: connection refused".into()));
    let state_display = match stream.state() {
        WsState::Error(e) => format!("Error(\"{}\")", e),
        WsState::Open       => "Open".to_string(),
        WsState::Connecting => "Connecting".to_string(),
        WsState::Closing    => "Closing".to_string(),
        WsState::Closed     => "Closed".to_string(),
    };

    card_box(c, ix, cy, 310.0, 44.0, Color::rgb(28, 14, 14), ACCENT5, 1.5);
    c.draw_text("WsStream::failed(WsError::Connect(...))", Point { x: ix + 6.0, y: cy + 6.0 }, TEXT_MUTED, font, 8.0);
    c.draw_text(
        &format!("state = {}", state_display.chars().take(42).collect::<String>()),
        Point { x: ix + 6.0, y: cy + 22.0 },
        ACCENT5, font, 9.0,
    );
    c.draw_text("is_open() = false", Point { x: ix + 6.0, y: cy + 34.0 }, TEXT_MUTED, font, 8.0);
    cy += 52.0;

    // ── PinchRecognizer panel ─────────────────────────────────────────────────
    c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: 310.0, height: 1.0 } }, DIVIDER);
    cy += 10.0;

    lbl(c, font, "PinchRecognizer — zoom bar:", ix, cy);
    cy += 13.0;

    // Scale visualization: 5 circles at different scales
    let scale_points = [0.5_f32, 0.75, 1.0, 1.5, 2.0];
    let circle_y = cy + 26.0;
    let spacing = 58.0_f32;
    let base_x = ix + 20.0;

    for (i, &scale) in scale_points.iter().enumerate() {
        let cx_val = base_x + i as f32 * spacing;
        let radius = (scale * 20.0).clamp(3.0, 40.0);
        let color = if scale < 1.0 { ACCENT3 } else if (scale - 1.0).abs() < 0.01 { ACCENT4 } else { ACCENT2 };
        c.fill_circle(Point { x: cx_val, y: circle_y }, radius, color);
        c.draw_text(
            &format!("{:.2}\u{00D7}", scale),
            Point { x: cx_val - 14.0, y: circle_y + radius + 4.0 },
            TEXT_MUTED, font, 8.0,
        );
    }

    cy += 80.0;

    lbl(c, font, "Scroll \u{2191} = zoom in, Scroll \u{2193} = zoom out", ix, cy);
    cy += 16.0;

    // Simulate 3 scroll-up events
    let mut pinch = PinchRecognizer::new();
    let scroll = InputEvent::Scroll { x: 350.0, y: 100.0, delta_y: -5.0 };
    pinch.on_event(&scroll, 0.0);
    pinch.on_event(&scroll, 0.0);
    pinch.on_event(&scroll, 0.0);
    let final_scale = pinch.accumulated_scale();

    lbl(c, font, "After 3 scroll-up events (delta_y=-5.0):", ix, cy);
    cy += 13.0;

    card_box(c, ix, cy, 310.0, 40.0, CARD_BG, ACCENT2, 1.5);
    c.draw_text(
        "pinch.accumulated_scale()",
        Point { x: ix + 8.0, y: cy + 8.0 },
        TEXT_MUTED, font, 9.0,
    );
    c.draw_text(
        &format!("= {:.4} (each step: 1 - (-5) * 0.01 = 1.05)", final_scale),
        Point { x: ix + 8.0, y: cy + 24.0 },
        ACCENT2, font, 9.0,
    );
    cy += 48.0;

    // Scale bar for final_scale
    let bar_bg_w = 280.0_f32;
    let bar_filled = (final_scale / 2.0 * bar_bg_w).clamp(0.0, bar_bg_w);
    c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: bar_bg_w, height: 10.0 } }, DIVIDER);
    c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: bar_filled, height: 10.0 } }, ACCENT2);
    c.draw_text(
        &format!("{:.4}\u{00D7} / 2.0\u{00D7}", final_scale),
        Point { x: ix, y: cy + 14.0 },
        TEXT_MUTED, font, 9.0,
    );

    // Stats footer
    let footer_y = y + PANEL_H - 28.0;
    lbl(c, font, "43 + 50 + 22 + 12 tests", ix, footer_y);
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() {
    let font = FontCache::system_mono().expect("system font not found");
    let mut c = SkiaCanvas::new(W, H);

    // Global background
    c.clear(BG);

    // ── Header bar (80px) ─────────────────────────────────────────────────────
    c.fill_rect(
        Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: W as f32, height: HEADER_H } },
        Color::rgb(13, 15, 26),
    );
    // Accent top line
    c.fill_rect(
        Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: W as f32, height: 3.0 } },
        ACCENT,
    );
    c.draw_text(
        "TEZZERA \u{2014} Phase 7 Showcase",
        Point { x: 28.0, y: 28.0 },
        TEXT_PRIMARY, &font, 22.0,
    );
    c.draw_text(
        "Glyph Metrics  \u{2022}  RTL Text  \u{2022}  Clipboard  \u{2022}  WebSocket + Pinch",
        Point { x: 28.0, y: 56.0 },
        TEXT_MUTED, &font, 11.0,
    );

    // ── Panels ────────────────────────────────────────────────────────────────
    let panel_y = HEADER_H;
    panel_glyph_metrics(&mut c, &font, px(0), panel_y);
    panel_rtl_text     (&mut c, &font, px(1), panel_y);
    panel_clipboard    (&mut c, &font, px(2), panel_y);
    panel_ws_pinch     (&mut c, &font, px(3), panel_y);

    // ── Panel dividers (1px vertical lines) ──────────────────────────────────
    for i in 1..4 {
        let dx = px(i);
        c.fill_rect(
            Rect { origin: Point { x: dx, y: panel_y }, size: Size { width: 1.0, height: PANEL_H } },
            DIVIDER,
        );
    }

    // ── Status bar ────────────────────────────────────────────────────────────
    let sb_y = H as f32 - 14.0;
    c.fill_rect(
        Rect { origin: Point { x: 0.0, y: sb_y - 4.0 }, size: Size { width: W as f32, height: 18.0 } },
        Color::rgb(8, 10, 16),
    );
    c.draw_text(
        "TEZZERA  \u{2022}  Phase 7  \u{2022}  Metrics \u{2713}  RTL \u{2713}  Clipboard \u{2713}  WebSocket \u{2713}  Pinch \u{2713}",
        Point { x: W as f32 / 2.0 - 300.0, y: sb_y - 1.0 },
        TEXT_MUTED, &font, 10.0,
    );

    // Encode and write
    let png = c.encode_png().expect("png encode failed");
    std::fs::write("phase7_demo.png", &png).expect("write phase7_demo.png");
    println!("Saved phase7_demo.png ({}x{})", W, H);
}
