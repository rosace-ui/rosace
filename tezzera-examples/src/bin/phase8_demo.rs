use tezzera::prelude::*;
use tezzera_state::Atom;
use tezzera::bidi::{bidi_class, paragraph_level, BidiParagraph};
use tezzera::ime::{ImeState, ImeEvent};
use tezzera::media::{AudioPlayer, MediaError};

#[derive(Debug, Clone, PartialEq)]
enum Panel { Renderer, Ime, Bidi, Media }

struct Phase8Demo;

impl Component for Phase8Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let panel: Atom<Panel>      = ctx.state(Panel::Renderer);
        let ime_text: Atom<String>  = ctx.state(String::new());

        let p1 = panel.clone(); let p2 = panel.clone();
        let p3 = panel.clone(); let p4 = panel.clone();
        let nav_row = Row::new()
            .child(Button::new("Renderer").on_press(move || p1.set(Panel::Renderer)))
            .child(Button::new("IME").on_press(move || p2.set(Panel::Ime)))
            .child(Button::new("Bidi").on_press(move || p3.set(Panel::Bidi)))
            .child(Button::new("Media").on_press(move || p4.set(Panel::Media)));

        let body: Box<dyn Widget> = match panel.get() {
            Panel::Renderer => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Renderer Abstraction"))
                        .child(Text::label("Renderer trait — object-safe (Box<dyn Renderer> compiles):"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("Renderer trait"))
                                .child(Text::caption("clear(color)"))
                                .child(Text::caption("fill_rect / stroke_rect / fill_circle"))
                                .child(Text::caption("draw_text(text, origin, color, font, px)"))
                                .child(Text::caption("encode_png() → Vec<u8>"))
                                .child(Text::caption("width() / height() → u32"))
                        ))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("SkiaRenderer"))
                                .child(Text::caption("Wraps SkiaCanvas from tezzera-render"))
                                .child(Text::caption("Implements Renderer trait"))
                                .child(Text::caption("D032: swap to skia-safe here when ready"))
                                .child(Text::caption("RendererBackend: TinySkia (default) | SkiaSafe"))
                        ))
                )
            }

            Panel::Ime => {
                let it = ime_text.clone();
                let it2 = ime_text.clone();
                let it3 = ime_text.clone();

                let mut state = ImeState::default();
                state.transition(&ImeEvent::Enabled);
                state.transition(&ImeEvent::Preedit {
                    text: "pree".into(),
                    cursor_range: Some((0, 4)),
                });

                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("IME Input (Stub)"))
                        .child(Text::label("ImeState machine: Idle → Enabled → Composing → Committed → Enabled"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading(format!("State: {:?}", state)))
                                .child(Text::caption("ImeEvent::Preedit { text, cursor_range }"))
                                .child(Text::caption("ImeEvent::Commit(String)"))
                                .child(Text::caption("ImeEvent::Enabled / Disabled"))
                        ))
                        .child(Text::label(format!("Committed text atom: \"{}\"", ime_text.get())))
                        .child(
                            Row::new()
                                .child(Button::new("Simulate Commit 'Hello'")
                                    .on_press(move || it.set("Hello".into())))
                                .child(Button::new("Simulate Commit '日本語'")
                                    .on_press(move || it2.set("日本語".into())))
                                .child(Button::new("Clear")
                                    .on_press(move || it3.set(String::new())))
                        )
                        .child(Text::caption("NoopIme converts Commit(s) to a String — no OS IME integration yet"))
                )
            }

            Panel::Bidi => {
                let samples = [
                    "Hello, World!",
                    "مرحبا بالعالم",
                    "Hello مرحبا World",
                    "שלום",
                ];
                let mut col = Column::new()
                    .child(nav_row)
                    .child(Text::display("Unicode Bidi"))
                    .child(Text::label("paragraph_level + per-char BidiClass:"));

                for text in &samples {
                    let level = paragraph_level(text);
                    let para = BidiParagraph::new(*text);
                    let first_class = text.chars().next()
                        .map(|c| format!("{:?}", bidi_class(c)))
                        .unwrap_or_default();
                    col = col.child(Card::new(
                        Column::new()
                            .child(Text::label(*text))
                            .child(Text::caption(format!(
                                "base_level={} first_class={} rtl_chars={}",
                                level, first_class, para.rtl_char_count()
                            )))
                    ));
                }
                Box::new(col)
            }

            Panel::Media => {
                let mut player = AudioPlayer::new();
                let audio_result = player.load("sample.wav");
                let error_str = match &audio_result {
                    Err(MediaError::PlatformUnavailable) => "PlatformUnavailable (expected)",
                    Err(MediaError::Unsupported)         => "Unsupported format",
                    Err(MediaError::NotFound(p))         => p,
                    Err(MediaError::DecodeFailed(e))     => e,
                    Ok(_)                                => "Loaded",
                    Err(_)                               => "Error",
                };

                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Media Stubs"))
                        .child(Text::label("AudioPlayer::load → MediaError::PlatformUnavailable:"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("AudioPlayer"))
                                .child(Text::caption(format!("load(\"sample.wav\"): {}", error_str)))
                                .child(Text::caption("play / pause / stop / volume(f32)"))
                                .child(Text::caption("Real decoding: v1.0 (rodio/cpal)"))
                        ))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("VideoDecoder"))
                                .child(Text::caption("open(path) → Err(PlatformUnavailable)"))
                                .child(Text::caption("next_frame() → Option<VideoFrame { w, h, data, ts_ms }>"))
                                .child(Text::caption("Real decoding: v1.0 (ffmpeg bindings)"))
                        ))
                        .child(
                            Row::new()
                                .child(format_card("WAV"))
                                .child(format_card("MP3"))
                                .child(format_card("MP4"))
                                .child(format_card("WebM"))
                        )
                )
            }
        };

        Scaffold::new(body)
            .app_bar(AppBar::new("Phase 8 — Renderer · IME · Bidi · Media"))
            .into_element()
    }
}

fn format_card(fmt: &str) -> impl Widget {
    Card::new(Text::label(fmt))
}

fn main() {
    App::run(Phase8Demo);
}
