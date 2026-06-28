//! Phase 6 Demo — 1400×900 static PNG showcasing Phase 6 systems.
//!
//! Four panels (350 wide each, y=80–900):
//!   Panel 1 — Gestures         (TapRecognizer, SwipeRecognizer, DragRecognizer)
//!   Panel 2 — Rich Text        (word_wrap, TextCursor, styled spans)
//!   Panel 3 — Network Images   (LoadState, RemoteImage, CustomPainter)
//!   Panel 4 — i18n             (MessageBundle, Locale, set_locale, t())
//!
//! Run:    cargo run -p tezzera-examples --bin phase6_demo
//! Output: phase6_demo.png (1400×900)

use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::{Color, FontCache, SkiaCanvas};

// ── New Phase 6 crate imports ─────────────────────────────────────────────────

// Gesture recognizers
use tezzera_gesture::{DragRecognizer, SwipeRecognizer, TapRecognizer};

// Rich text
use tezzera_text::word_wrap_simple;

// Network images
use tezzera_net::LoadState;

// i18n
use tezzera_i18n::{MessageBundle, Locale, set_locale, t};

// Custom painter
use tezzera_widgets::painter::{CustomPainter, PainterContext, PainterWidget};

// ── Canvas dimensions ─────────────────────────────────────────────────────────
const W: u32 = 1400;
const H: u32 = 900;

// ── Color palette ─────────────────────────────────────────────────────────────
const BG:           Color = Color::rgb(12,  14,  22);
const PANEL_BG:     Color = Color::rgb(18,  20,  32);
const DIVIDER:      Color = Color::rgb(40,  45,  65);
const ACCENT:       Color = Color::rgb(107, 80, 200);
const ACCENT2:      Color = Color::rgb( 72, 199, 116);
const ACCENT3:      Color = Color::rgb(255, 160,  60);
const ACCENT4:      Color = Color::rgb( 80, 180, 255);
const TEXT_PRIMARY: Color = Color::rgb(230, 230, 245);
const TEXT_MUTED:   Color = Color::rgb(130, 135, 165);
const CARD_BG:      Color = Color::rgb(24,  26,  42);

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

// ── Panel 1 — Gestures ───────────────────────────────────────────────────────

fn panel_gestures(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
    // Panel background
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: PANEL_W, height: PANEL_H } }, PANEL_BG);

    let ix = x + 20.0;
    let mut cy = y + 14.0;

    // Section header
    section_label(c, font, "Gesture Recognizers", ix, cy, ACCENT);
    cy += 24.0;

    // ── Instantiate recognizers to prove they compile ─────────────────────────
    let _tap_rec = TapRecognizer::new();
    let _double_tap_rec = TapRecognizer::new();
    let _swipe_rec = SwipeRecognizer::new();
    let _drag_rec = DragRecognizer::new();

    // ── 1. Tap Zone ───────────────────────────────────────────────────────────
    {
        let bw = 310.0_f32;
        let bh = 80.0_f32;
        card_box(c, ix, cy, bw, bh, CARD_BG, ACCENT, 2.0);
        c.draw_text("TAP ZONE", Point { x: ix + bw / 2.0 - 32.0, y: cy + 28.0 }, TEXT_MUTED, font, 11.0);
        cy += bh + 6.0;
        c.draw_text("Last: Tap at (155, 90)", Point { x: ix, y: cy }, ACCENT2, font, 10.0);
        cy += 20.0;
    }

    // ── 2. Double-Tap card ────────────────────────────────────────────────────
    {
        let bw = 310.0_f32;
        let bh = 80.0_f32;
        card_box(c, ix, cy, bw, bh, CARD_BG, ACCENT, 2.0);
        c.draw_text("DOUBLE-TAP ZONE", Point { x: ix + bw / 2.0 - 56.0, y: cy + 28.0 }, TEXT_MUTED, font, 11.0);
        cy += bh + 6.0;
        c.draw_text("Last: DoubleTap at (155, 90)", Point { x: ix, y: cy }, ACCENT3, font, 10.0);
        cy += 20.0;
    }

    // ── 3. Swipe indicators ───────────────────────────────────────────────────
    {
        lbl(c, font, "SWIPE DIRECTIONS", ix, cy);
        cy += 16.0;

        let cx_center = ix + 155.0;
        let cy_center = cy + 50.0;
        let arm = 38.0_f32;

        // Left arrow (←)
        let ax = cx_center - arm;
        c.fill_rect(Rect { origin: Point { x: ax, y: cy_center - 1.0 }, size: Size { width: arm - 8.0, height: 2.0 } }, TEXT_MUTED);
        c.fill_circle(Point { x: ax, y: cy_center }, 4.0, TEXT_MUTED);
        c.draw_text("Left", Point { x: ax - 12.0, y: cy_center + 12.0 }, TEXT_MUTED, font, 9.0);

        // Right arrow (→)
        let ax = cx_center + 8.0;
        c.fill_rect(Rect { origin: Point { x: ax, y: cy_center - 1.0 }, size: Size { width: arm - 8.0, height: 2.0 } }, TEXT_MUTED);
        c.fill_circle(Point { x: ax + arm - 8.0, y: cy_center }, 4.0, TEXT_MUTED);
        c.draw_text("Right", Point { x: ax + arm - 10.0, y: cy_center + 12.0 }, TEXT_MUTED, font, 9.0);

        // Up arrow (↑)
        let ay = cy_center - arm;
        c.fill_rect(Rect { origin: Point { x: cx_center - 1.0, y: ay + 8.0 }, size: Size { width: 2.0, height: arm - 8.0 } }, TEXT_MUTED);
        c.fill_circle(Point { x: cx_center, y: ay + 8.0 }, 4.0, TEXT_MUTED);
        c.draw_text("Up", Point { x: cx_center - 6.0, y: ay - 2.0 }, TEXT_MUTED, font, 9.0);

        // Down arrow (↓)
        let ay = cy_center;
        c.fill_rect(Rect { origin: Point { x: cx_center - 1.0, y: ay }, size: Size { width: 2.0, height: arm - 8.0 } }, TEXT_MUTED);
        c.fill_circle(Point { x: cx_center, y: ay + arm - 8.0 }, 4.0, TEXT_MUTED);
        c.draw_text("Down", Point { x: cx_center - 9.0, y: ay + arm }, TEXT_MUTED, font, 9.0);

        cy += 120.0;
    }

    // ── 4. Drag trace ─────────────────────────────────────────────────────────
    {
        lbl(c, font, "DRAG TRACE", ix, cy);
        cy += 14.0;

        // Sinusoidal trace
        let trace_w = 310.0_f32;
        let trace_baseline = cy + 30.0;
        let steps = 30usize;
        for i in 0..steps {
            let t_val = i as f32 / (steps - 1) as f32;
            let px_val = ix + t_val * trace_w;
            let py_val = trace_baseline + (t_val * std::f32::consts::PI * 3.0).sin() * 18.0;
            c.fill_circle(Point { x: px_val, y: py_val }, 4.0, ACCENT4);
        }

        cy += 60.0;
        c.draw_text("Phase: Move  \u{0394}(8.2, 3.1)", Point { x: ix, y: cy }, ACCENT4, font, 10.0);
        cy += 20.0;
    }

    // ── 5. LongPress ─────────────────────────────────────────────────────────
    {
        let bw = 310.0_f32;
        let bh = 56.0_f32;
        card_box(c, ix, cy, bw, bh, CARD_BG, ACCENT, 1.5);
        c.draw_text("HOLD TO TRIGGER", Point { x: ix + bw / 2.0 - 54.0, y: cy + 14.0 }, TEXT_MUTED, font, 11.0);

        // Progress bar 60%
        let bar_y = cy + bh - 18.0;
        let bar_w = bw - 20.0;
        c.fill_rect(Rect { origin: Point { x: ix + 10.0, y: bar_y }, size: Size { width: bar_w, height: 8.0 } }, DIVIDER);
        c.fill_rect(Rect { origin: Point { x: ix + 10.0, y: bar_y }, size: Size { width: bar_w * 0.6, height: 8.0 } }, ACCENT);
        cy += bh + 6.0;

        c.draw_text("0.3s / 0.5s", Point { x: ix, y: cy }, TEXT_MUTED, font, 10.0);
        cy += 20.0;
    }

    // ── Stats footer ──────────────────────────────────────────────────────────
    let footer_y = y + PANEL_H - 28.0;
    lbl(c, font, "31 tests \u{2022} 4 recognizer types", ix, footer_y);
}

// ── Panel 2 — Rich Text ───────────────────────────────────────────────────────

fn panel_rich_text(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: PANEL_W, height: PANEL_H } }, PANEL_BG);

    let ix = x + 20.0;
    let mut cy = y + 14.0;

    section_label(c, font, "Rich Text Layout", ix, cy, ACCENT2);
    cy += 24.0;

    // ── 1. Span showcase ──────────────────────────────────────────────────────
    {
        c.draw_text("Hello ", Point { x: ix, y: cy }, Color::WHITE, font, 20.0);
        c.draw_text("World", Point { x: ix + 44.0, y: cy }, ACCENT, font, 20.0);
        c.draw_text("!", Point { x: ix + 106.0, y: cy }, ACCENT3, font, 24.0);
        cy += 38.0;
    }

    // ── 2. Word-wrapped paragraph ─────────────────────────────────────────────
    {
        lbl(c, font, "Word Wrap (max 300px):", ix, cy);
        cy += 14.0;

        let sentence = "The quick brown fox jumps over the lazy dog near the riverbank";
        let lines = word_wrap_simple(sentence, 300.0, 8.0);
        for line in &lines {
            c.draw_text(line, Point { x: ix, y: cy }, TEXT_PRIMARY, font, 13.0);
            cy += 18.0;
        }
        cy += 10.0;
    }

    // ── 3. Mixed styles block ─────────────────────────────────────────────────
    {
        lbl(c, font, "Mixed Styles:", ix, cy);
        cy += 14.0;

        // Line 1: Status
        c.draw_text("Status: ", Point { x: ix, y: cy }, Color::WHITE, font, 12.0);
        c.draw_text("ACTIVE", Point { x: ix + 54.0, y: cy }, ACCENT2, font, 12.0);
        cy += 18.0;

        // Line 2: Priority
        c.draw_text("Priority: ", Point { x: ix, y: cy }, Color::WHITE, font, 12.0);
        c.draw_text("HIGH", Point { x: ix + 62.0, y: cy }, ACCENT3, font, 12.0);
        cy += 18.0;

        // Line 3: Version
        c.draw_text("Version: ", Point { x: ix, y: cy }, Color::WHITE, font, 12.0);
        c.draw_text("v0.6.0", Point { x: ix + 57.0, y: cy }, ACCENT, font, 12.0);
        cy += 30.0;
    }

    // ── 4. Cursor visualization ───────────────────────────────────────────────
    {
        lbl(c, font, "Text Cursor (line: 1, col: 5):", ix, cy);
        cy += 14.0;

        // 3 lines of simulated monospace text
        let line_h = 18.0_f32;
        let char_w = 8.0_f32;
        let editor_lines = ["Hello World", "const x = 42;", "fn main() {}"];
        for (i, line_text) in editor_lines.iter().enumerate() {
            let lx = ix;
            let ly = cy + i as f32 * line_h;
            // Draw text background box
            let text_w = line_text.len() as f32 * char_w;
            c.fill_rect(Rect { origin: Point { x: lx, y: ly }, size: Size { width: text_w + 4.0, height: line_h - 2.0 } }, CARD_BG);
            c.draw_text(line_text, Point { x: lx + 2.0, y: ly + 2.0 }, TEXT_MUTED, font, 11.0);
            // Draw cursor at col=5 on line 1
            if i == 1 {
                let cursor_x = lx + 2.0 + 5.0 * char_w;
                c.fill_rect(
                    Rect { origin: Point { x: cursor_x, y: ly }, size: Size { width: 2.0, height: 14.0 } },
                    ACCENT,
                );
            }
        }
        cy += 3.0 * line_h + 8.0;
        lbl(c, font, "TextCursor { line: 1, col: 5 }", ix, cy);
        cy += 20.0;
    }

    // ── 5. Font metrics bar ───────────────────────────────────────────────────
    {
        lbl(c, font, "Estimated Width:", ix, cy);
        cy += 14.0;

        // "Hello World" — 11 chars × 7.7px = 84.7px
        let est_w = 11.0 * 7.7_f32; // ~84.7
        let bar_bg_w = 280.0_f32;
        c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: bar_bg_w, height: 12.0 } }, DIVIDER);
        c.fill_rect(Rect { origin: Point { x: ix, y: cy }, size: Size { width: est_w, height: 12.0 } }, ACCENT2);
        cy += 16.0;
        c.draw_text("11 chars x 7.7px = 84.7px", Point { x: ix, y: cy }, TEXT_MUTED, font, 10.0);
        cy += 20.0;
    }

    // ── Stats footer ──────────────────────────────────────────────────────────
    let footer_y = y + PANEL_H - 28.0;
    lbl(c, font, "34 tests \u{2022} word_wrap \u{2022} TextCursor", ix, footer_y);
}

// ── Panel 3 — Network Images + Custom Painter ─────────────────────────────────

// A painter that draws a checkerboard of colored squares.
struct CheckerPainter;

impl CustomPainter for CheckerPainter {
    fn paint(&self, ctx: &mut PainterContext) {
        ctx.fill_background(CARD_BG);

        let cols = 8usize;
        let rows = 5usize;
        let sq = 32.0_f32;
        let gap = 2.0_f32;

        for row in 0..rows {
            for col in 0..cols {
                let sx = col as f32 * (sq + gap) + 4.0;
                let sy = row as f32 * (sq + gap) + 20.0;
                let color = if (row + col) % 2 == 0 { ACCENT } else { ACCENT2 };
                ctx.fill_rect(
                    Rect { origin: Point { x: sx, y: sy }, size: Size { width: sq, height: sq } },
                    color,
                );
            }
        }

        ctx.draw_text("PainterWidget  \u{2022}  310x200", Point { x: 4.0, y: 4.0 }, Color::WHITE, 9.0);
    }
}

fn panel_net_images(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: PANEL_W, height: PANEL_H } }, PANEL_BG);

    let ix = x + 20.0;
    let mut cy = y + 14.0;

    section_label(c, font, "Network Images & Painters", ix, cy, ACCENT3);
    cy += 24.0;

    // Instantiate LoadState variants to prove they compile
    let state_loading: LoadState<Vec<u8>> = LoadState::Loading;
    let state_loaded: LoadState<Vec<u8>> = LoadState::Loaded(vec![0u8; 49152]); // 48 KB
    let state_failed: LoadState<Vec<u8>> = LoadState::Failed("connection refused".into());

    // ── Card A — Loading ──────────────────────────────────────────────────────
    {
        let card_h = 80.0_f32;
        let card_w = 310.0_f32;
        card_box(c, ix, cy, card_w, card_h, CARD_BG, DIVIDER, 1.0);

        // Spinner — arc drawn as small circles in a ring
        let sc = Point { x: ix + 30.0, y: cy + card_h / 2.0 };
        let radius = 14.0_f32;
        let dot_count = 8usize;
        for i in 0..dot_count {
            let angle = i as f32 / dot_count as f32 * std::f32::consts::TAU;
            let alpha = ((i as f32 / dot_count as f32) * 255.0) as u8;
            let dpx = sc.x + angle.cos() * radius;
            let dpy = sc.y + angle.sin() * radius;
            c.fill_circle(Point { x: dpx, y: dpy }, 3.0, Color::rgba(130, 135, 165, alpha));
        }

        c.draw_text(
            if state_loading.is_loading() { "Loading..." } else { "?" },
            Point { x: ix + 58.0, y: cy + 22.0 },
            TEXT_MUTED, font, 11.0,
        );
        c.draw_text("http://example.com/photo1.jpg", Point { x: ix + 58.0, y: cy + 42.0 }, TEXT_MUTED, font, 9.0);
        cy += card_h + 10.0;
    }

    // ── Card B — Loaded ───────────────────────────────────────────────────────
    {
        let card_h = 80.0_f32;
        let card_w = 310.0_f32;
        card_box(c, ix, cy, card_w, card_h, CARD_BG, DIVIDER, 1.0);

        // Image placeholder rect (semi-transparent)
        c.fill_rect(
            Rect { origin: Point { x: ix + 4.0, y: cy + 4.0 }, size: Size { width: 46.0, height: 72.0 } },
            Color::rgba(ACCENT.r, ACCENT.g, ACCENT.b, 50),
        );
        c.stroke_rect(
            Rect { origin: Point { x: ix + 4.0, y: cy + 4.0 }, size: Size { width: 46.0, height: 72.0 } },
            ACCENT, 1.0,
        );

        // Green checkmark
        c.draw_text("\u{2713}", Point { x: ix + 58.0, y: cy + 16.0 }, ACCENT2, font, 18.0);

        let kb_label = if let Some(bytes) = state_loaded.value() {
            format!("Loaded  \u{2022}  {} KB", bytes.len() / 1024)
        } else {
            "Loaded".to_string()
        };
        c.draw_text(&kb_label, Point { x: ix + 82.0, y: cy + 22.0 }, ACCENT2, font, 11.0);
        c.draw_text("http://example.com/photo2.jpg", Point { x: ix + 58.0, y: cy + 42.0 }, TEXT_MUTED, font, 9.0);
        cy += card_h + 10.0;
    }

    // ── Card C — Failed ───────────────────────────────────────────────────────
    {
        let card_h = 80.0_f32;
        let card_w = 310.0_f32;
        card_box(c, ix, cy, card_w, card_h, CARD_BG, DIVIDER, 1.0);

        // X marks the spot — two crossing thin rects
        let ex = ix + 20.0;
        let ey = cy + card_h / 2.0;
        let arm = 10.0_f32;
        let error_color = Color::rgb(255, 80, 80);
        c.fill_rect(Rect { origin: Point { x: ex - arm, y: ey - 1.5 }, size: Size { width: arm * 2.0, height: 3.0 } }, error_color);
        // Vertical part of X (diagonal simulated with two tilted rects)
        c.fill_rect(Rect { origin: Point { x: ex - 1.5, y: ey - arm }, size: Size { width: 3.0, height: arm * 2.0 } }, error_color);

        let err_msg = state_failed.error().unwrap_or("unknown error");
        c.draw_text(
            &format!("Failed: {}", err_msg),
            Point { x: ix + 46.0, y: cy + 22.0 },
            error_color, font, 10.0,
        );
        c.draw_text("http://example.com/photo3.jpg", Point { x: ix + 46.0, y: cy + 42.0 }, TEXT_MUTED, font, 9.0);
        cy += card_h + 12.0;
    }

    // Divider
    c.fill_rect(
        Rect { origin: Point { x: ix, y: cy }, size: Size { width: 310.0, height: 1.0 } },
        DIVIDER,
    );
    cy += 10.0;

    // ── Custom Painter showcase ───────────────────────────────────────────────
    {
        section_label(c, font, "Custom Painter (D034):", ix, cy, ACCENT3);
        cy += 16.0;

        let painter_w = 310.0_f32;
        let painter_h = 200.0_f32;

        let pw = PainterWidget::new(painter_w, painter_h, CheckerPainter);
        pw.render(c, font, ix, cy);

        // Stroke outer border
        c.stroke_rect(
            Rect { origin: Point { x: ix, y: cy }, size: Size { width: painter_w, height: painter_h } },
            ACCENT3, 1.0,
        );
        cy += painter_h + 8.0;
    }

    // ── Stats footer ──────────────────────────────────────────────────────────
    let footer_y = y + PANEL_H - 28.0;
    lbl(c, font, "28 tests \u{2022} LoadState<T> \u{2022} CustomPainter", ix, footer_y);
}

// ── Panel 4 — i18n ───────────────────────────────────────────────────────────

fn panel_i18n(c: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
    c.fill_rect(Rect { origin: Point { x, y }, size: Size { width: PANEL_W, height: PANEL_H } }, PANEL_BG);

    let ix = x + 20.0;
    let mut cy = y + 14.0;

    section_label(c, font, "Internationalization (i18n)", ix, cy, ACCENT4);
    cy += 24.0;

    // Set up and use actual i18n API
    let mut bundle = MessageBundle::new(Locale::english().with_region("US"));
    bundle.insert("greeting", "Hello");
    bundle.insert("farewell", "Goodbye");
    bundle.insert("app_name", "TEZZERA");
    bundle.insert("status_ok", "Active");
    bundle.insert("status_err", "Error");
    set_locale(bundle);

    let greeting_val = t("greeting");
    let missing_val = t("missing_key");

    // ── Locale cards ──────────────────────────────────────────────────────────
    {
        let locales: &[(&str, &str, &str, Color)] = &[
            ("en-US", "EN", "Hello, World!", ACCENT4),
            ("fr-FR", "FR", "Bonjour, le Monde!", ACCENT),
            ("es-ES", "ES", "\u{00A1}Hola, Mundo!", ACCENT2),
            ("ja-JP", "JA", "konnichiwa sekai", ACCENT3),
        ];
        for (locale_str, code, greeting_text, flag_color) in locales {
            let row_h = 52.0_f32;
            card_box(c, ix, cy, 310.0, row_h, CARD_BG, DIVIDER, 1.0);

            // Flag placeholder rect
            c.fill_rect(
                Rect { origin: Point { x: ix + 6.0, y: cy + 8.0 }, size: Size { width: 28.0, height: 20.0 } },
                *flag_color,
            );
            c.draw_text(code, Point { x: ix + 8.0, y: cy + 11.0 }, Color::BLACK, font, 9.0);

            c.draw_text(locale_str, Point { x: ix + 42.0, y: cy + 8.0 }, TEXT_MUTED, font, 10.0);
            c.draw_text(greeting_text, Point { x: ix + 42.0, y: cy + 26.0 }, TEXT_PRIMARY, font, 11.0);
            cy += row_h + 4.0;
        }
        cy += 8.0;
    }

    // ── MessageBundle viewer ──────────────────────────────────────────────────
    {
        lbl(c, font, "MessageBundle (key=value):", ix, cy);
        cy += 14.0;

        let box_w = 310.0_f32;
        let box_h = 100.0_f32;
        card_box(c, ix, cy, box_w, box_h, CARD_BG, DIVIDER, 1.0);

        let bundle_lines = [
            "# English bundle",
            "greeting = Hello",
            "farewell = Goodbye",
            "app_name = TEZZERA",
            "status_ok = Active",
            "status_err = Error",
        ];
        for (i, line) in bundle_lines.iter().enumerate() {
            c.draw_text(line, Point { x: ix + 8.0, y: cy + 6.0 + i as f32 * 14.0 }, TEXT_MUTED, font, 10.0);
        }
        cy += box_h + 12.0;
    }

    // ── t() function demo ─────────────────────────────────────────────────────
    {
        lbl(c, font, "t() lookup:", ix, cy);
        cy += 16.0;

        // Row 1: found key
        c.draw_text("t(\"greeting\")", Point { x: ix, y: cy }, ACCENT, font, 11.0);
        c.draw_text("\u{2192}", Point { x: ix + 100.0, y: cy }, TEXT_MUTED, font, 11.0);
        c.draw_text(&greeting_val, Point { x: ix + 116.0, y: cy }, ACCENT2, font, 11.0);
        cy += 18.0;

        // Row 2: missing key (fallback)
        c.draw_text("t(\"missing_key\")", Point { x: ix, y: cy }, ACCENT, font, 11.0);
        c.draw_text("\u{2192}", Point { x: ix + 122.0, y: cy }, TEXT_MUTED, font, 11.0);
        c.draw_text(&missing_val, Point { x: ix + 138.0, y: cy }, ACCENT3, font, 11.0);
        c.draw_text("(fallback)", Point { x: ix + 220.0, y: cy }, TEXT_MUTED, font, 9.0);
        cy += 28.0;
    }

    // ── Locale switcher ───────────────────────────────────────────────────────
    {
        let pill_labels = ["EN", "FR", "ES", "JA"];
        let pill_w = 44.0_f32;
        let pill_h = 24.0_f32;
        let pill_gap = 8.0_f32;

        for (i, label) in pill_labels.iter().enumerate() {
            let px_val = ix + i as f32 * (pill_w + pill_gap);
            if i == 0 {
                // Active pill — filled
                c.fill_rect(
                    Rect { origin: Point { x: px_val, y: cy }, size: Size { width: pill_w, height: pill_h } },
                    ACCENT4,
                );
                c.draw_text(label, Point { x: px_val + 12.0, y: cy + 6.0 }, Color::BLACK, font, 10.0);
            } else {
                // Inactive — outline only
                c.stroke_rect(
                    Rect { origin: Point { x: px_val, y: cy }, size: Size { width: pill_w, height: pill_h } },
                    TEXT_MUTED, 1.0,
                );
                c.draw_text(label, Point { x: px_val + 12.0, y: cy + 6.0 }, TEXT_MUTED, font, 10.0);
            }
        }
        cy += pill_h + 8.0;
        c.draw_text("Active: en-US", Point { x: ix, y: cy }, ACCENT4, font, 10.0);
    }

    // ── Stats footer ──────────────────────────────────────────────────────────
    let footer_y = y + PANEL_H - 28.0;
    lbl(c, font, "28 tests \u{2022} Locale \u{2022} MessageBundle \u{2022} t()", ix, footer_y);
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
        Color::rgb(15, 17, 28),
    );
    // Accent top line
    c.fill_rect(
        Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: W as f32, height: 3.0 } },
        ACCENT,
    );
    c.draw_text(
        "TEZZERA \u{2014} Phase 6 Showcase",
        Point { x: 28.0, y: 28.0 },
        TEXT_PRIMARY, &font, 22.0,
    );
    c.draw_text(
        "Gestures  \u{2022}  Rich Text  \u{2022}  Network Images  \u{2022}  i18n",
        Point { x: 28.0, y: 56.0 },
        TEXT_MUTED, &font, 11.0,
    );

    // ── Panels ────────────────────────────────────────────────────────────────
    let panel_y = HEADER_H;
    panel_gestures  (&mut c, &font, px(0), panel_y);
    panel_rich_text (&mut c, &font, px(1), panel_y);
    panel_net_images(&mut c, &font, px(2), panel_y);
    panel_i18n      (&mut c, &font, px(3), panel_y);

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
        Color::rgb(10, 12, 18),
    );
    c.draw_text(
        "TEZZERA  \u{2022}  Phase 6  \u{2022}  Gestures \u{2713}  Rich Text \u{2713}  Network Images \u{2713}  i18n \u{2713}",
        Point { x: W as f32 / 2.0 - 280.0, y: sb_y - 1.0 },
        TEXT_MUTED, &font, 10.0,
    );

    // Encode and write
    let png = c.encode_png().expect("png encode failed");
    std::fs::write("phase6_demo.png", &png).expect("write phase6_demo.png");
    println!("Saved phase6_demo.png ({}x{})", W, H);
}
