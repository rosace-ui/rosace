use tezzera::prelude::*;
use tezzera_state::Atom;
use tezzera::text::direction::detect_direction;

#[derive(Debug, Clone, PartialEq)]
enum Panel { GlyphMetrics, RtlText, Clipboard, WsAndPinch }

struct Phase7Demo;

impl Component for Phase7Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let panel: Atom<Panel>         = ctx.state(Panel::GlyphMetrics);
        let clipboard_out: Atom<String> = ctx.state(String::new());
        let pinch_scale: Atom<f32>     = ctx.state(1.0f32);

        let p1 = panel.clone(); let p2 = panel.clone();
        let p3 = panel.clone(); let p4 = panel.clone();
        let nav_row = Row::new()
            .child(Button::new("Glyph Metrics").on_press(move || p1.set(Panel::GlyphMetrics)))
            .child(Button::new("RTL Text").on_press(move || p2.set(Panel::RtlText)))
            .child(Button::new("Clipboard").on_press(move || p3.set(Panel::Clipboard)))
            .child(Button::new("WS & Pinch").on_press(move || p4.set(Panel::WsAndPinch)));

        let body: Box<dyn Widget> = match panel.get() {
            Panel::GlyphMetrics => {
                let samples = [
                    ("Hello", 14.0_f32),
                    ("World!", 18.0),
                    ("TEZZERA", 24.0),
                    ("the quick brown fox", 12.0),
                ];
                let mut col = Column::new().child(nav_row).child(Text::display("Glyph Metrics"));
                col = col.child(Text::label("fontdue measure_text vs heuristic (char × 0.55):"));
                for (text, px) in &samples {
                    let heuristic = text.len() as f32 * px * 0.55;
                    col = col.child(Card::new(
                        Row::new()
                            .child(Text::label(format!("\"{}\" @{}px", text, px)))
                            .child(Text::caption(format!("heuristic={:.1}px", heuristic)))
                    ));
                }
                Box::new(col)
            }

            Panel::RtlText => {
                let samples = [
                    ("Hello, World!", "Latin"),
                    ("مرحبا بالعالم", "Arabic"),
                    ("שלום עולם", "Hebrew"),
                    ("Mixed: مرحبا Hello", "Mixed"),
                ];
                let mut col = Column::new().child(nav_row).child(Text::display("RTL Text"));
                col = col.child(Text::label("detect_direction per sample:"));
                for (text, label) in &samples {
                    let dir = detect_direction(text);
                    col = col.child(Card::new(
                        Row::new()
                            .child(Text::label(*label))
                            .child(Text::caption(format!("{:?}", dir)))
                            .child(Text::label(*text))
                    ));
                }
                Box::new(col)
            }

            Panel::Clipboard => {
                let co = clipboard_out.clone();
                let co2 = clipboard_out.clone();
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Clipboard & Text Selection"))
                        .child(Text::label("TextInput with selection support:"))
                        .child(TextInput::new().placeholder("Type here, then copy"))
                        .child(
                            Row::new()
                                .child(Button::new("Write 'Tezzera'")
                                    .on_press(move || co.set("Tezzera".into())))
                                .child(Button::new("Clear")
                                    .on_press(move || co2.set(String::new())))
                        )
                        .child(Text::label(format!(
                            "Clipboard atom: \"{}\"",
                            clipboard_out.get()
                        )))
                        .child(Text::caption("SystemClipboard: pbcopy/pbpaste (macOS), xclip (Linux)"))
                        .child(Text::caption("TextSelection { anchor, focus } → .text(lines) extracts substring"))
                )
            }

            Panel::WsAndPinch => {
                let ps = pinch_scale.clone();
                let ps2 = pinch_scale.clone();
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("WebSocket & Pinch"))
                        .child(
                            Row::new()
                                .child(Card::new(
                                    Column::new()
                                        .child(Text::heading("WsClient"))
                                        .child(Text::label("LoadState: Idle → Loading → Loaded / Failed"))
                                        .child(Text::caption("RFC 6455 hand-rolled handshake"))
                                        .child(Text::caption("set_nonblocking(true) — safe to call each frame"))
                                        .child(Text::caption("WASM: returns Err(Connect(\"use web-sys\"))"))
                                ))
                                .child(Card::new(
                                    Column::new()
                                        .child(Text::heading("PinchRecognizer"))
                                        .child(Text::label(format!("Scale: {:.3}", pinch_scale.get())))
                                        .child(ProgressBar::new(
                                            (pinch_scale.get().clamp(0.5, 2.0) - 0.5) / 1.5
                                        ))
                                        .child(
                                            Row::new()
                                                .child(Button::new("Zoom In")
                                                    .on_press(move || ps.set((ps.get() * 1.1).min(2.0))))
                                                .child(Button::new("Zoom Out")
                                                    .on_press(move || ps2.set((ps2.get() * 0.9).max(0.5))))
                                        )
                                        .child(Text::caption("Desktop: scroll delta → scale × 0.01"))
                                ))
                        )
                )
            }
        };

        Scaffold::new(body)
            .app_bar(AppBar::new("Phase 7 — Glyph Metrics · RTL · Clipboard · WS & Pinch"))
            .into_element()
    }
}

fn main() {
    App::run(Phase7Demo);
}
