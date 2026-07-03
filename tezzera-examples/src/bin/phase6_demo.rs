use tezzera::prelude::*;
use tezzera_state::Atom;
use tezzera::i18n::{MessageBundle, set_locale, t};
use tezzera::i18n::locale::Locale;

#[derive(Debug, Clone, PartialEq)]
enum Panel { Gestures, RichText, Network, I18n }

#[derive(Debug, Clone, PartialEq)]
enum Lang { En, Fr, Es }

struct Phase6Demo;

impl Component for Phase6Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let panel: Atom<Panel>   = ctx.state(Panel::Gestures);
        let lang: Atom<Lang>     = ctx.state(Lang::En);
        let tap_count: Atom<u32> = ctx.state(0u32);

        let p1 = panel.clone(); let p2 = panel.clone();
        let p3 = panel.clone(); let p4 = panel.clone();
        let nav_row = Row::new()
            .child(Button::new("Gestures").on_press(move || p1.set(Panel::Gestures)))
            .child(Button::new("Rich Text").on_press(move || p2.set(Panel::RichText)))
            .child(Button::new("Network").on_press(move || p3.set(Panel::Network)))
            .child(Button::new("i18n").on_press(move || p4.set(Panel::I18n)));

        let body: Box<dyn Widget> = match panel.get() {
            Panel::Gestures => {
                let tc = tap_count.clone();
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Gesture Recognition"))
                        .child(
                            Row::new()
                                .child(Card::new(
                                    Column::new()
                                        .child(Text::heading("Tap"))
                                        .child(Text::caption("Single/double within 300ms"))
                                        .child(Text::label(format!("Taps: {}", tc.get())))
                                        .child(Button::new("Tap me")
                                            .on_press(move || tap_count.set(tap_count.get() + 1)))
                                ))
                                .child(Card::new(
                                    Column::new()
                                        .child(Text::heading("Swipe"))
                                        .child(Text::caption("Min 80px, >200px/s"))
                                        .child(Text::label("Left / Right / Up / Down"))
                                ))
                        )
                        .child(
                            Row::new()
                                .child(Card::new(
                                    Column::new()
                                        .child(Text::heading("Drag"))
                                        .child(Text::caption("Begin / Move / End phases"))
                                        .child(Text::label("Delta: (dx, dy) per frame"))
                                ))
                                .child(Card::new(
                                    Column::new()
                                        .child(Text::heading("Pinch"))
                                        .child(Text::caption("WASM: TouchEvent scale"))
                                        .child(Text::label("Desktop: scroll wheel zoom"))
                                ))
                        )
                )
            }

            Panel::RichText => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Rich Text"))
                        .child(Text::label("Mixed bold / colored / italic spans:"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("TextSpan { text, font_size, color, bold, italic, underline }"))
                                .child(Text::label("→ RichText: list of TextSpans forming a paragraph"))
                                .child(Text::label("→ TextLayout: wraps to max_width via greedy word_wrap"))
                                .child(Text::label("→ TextCursor { line, col }: advance / backspace"))
                        ))
                        .child(Text::label("Word-wrap demo:"))
                        .child(Card::new(
                            Text::new("The quick brown fox jumps over the lazy dog. Rust UI framework with declarative composition, spring animations, and typed navigation.")
                                .size(13.0)
                        ))
                )
            }

            Panel::Network => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Network Images"))
                        .child(Text::label("RemoteImage — LoadState machine:"))
                        .child(
                            Row::new()
                                .child(load_state_card("Idle", Color::rgb(80, 80, 80)))
                                .child(load_state_card("Loading", Color::rgb(60, 120, 200)))
                                .child(load_state_card("Loaded", Color::rgb(40, 160, 80)))
                                .child(load_state_card("Failed", Color::rgb(180, 60, 60)))
                        )
                        .child(Text::caption("HTTP via std::net::TcpStream — no reqwest dep"))
                        .child(Text::caption("ImageLoader: std::thread + mpsc channel"))
                        .child(Text::caption("Cache keyed by URL string — no double-fetch"))
                )
            }

            Panel::I18n => {
                let cur_lang = lang.get();
                let bundle = match &cur_lang {
                    Lang::En => MessageBundle::from_str(
                        Locale::english(),
                        "greeting = Hello\nfarewell = Goodbye\naction = Submit",
                    ),
                    Lang::Fr => MessageBundle::from_str(
                        Locale::french(),
                        "greeting = Bonjour\nfarewell = Au revoir\naction = Soumettre",
                    ),
                    Lang::Es => MessageBundle::from_str(
                        Locale::new("es"),
                        "greeting = Hola\nfarewell = Adios\naction = Enviar",
                    ),
                };
                set_locale(bundle);

                let l1 = lang.clone(); let l2 = lang.clone(); let l3 = lang.clone();
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Localization (i18n)"))
                        .child(
                            Row::new()
                                .child(Button::new("EN").on_press(move || l1.set(Lang::En)))
                                .child(Button::new("FR").on_press(move || l2.set(Lang::Fr)))
                                .child(Button::new("ES").on_press(move || l3.set(Lang::Es)))
                        )
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading(t("greeting")))
                                .child(Text::label(t("farewell")))
                                .child(Button::new(t("action")))
                        ))
                        .child(Text::caption("t(key) looks up active MessageBundle, falls back to key"))
                        .child(Text::caption("Bundle format: key = value (plain text, no TOML/JSON)"))
                )
            }
        };

        Scaffold::new(body)
            .app_bar(AppBar::new("Phase 6 — Gestures · Rich Text · Network · i18n"))
            .into_element()
    }
}

fn load_state_card(state: &str, color: Color) -> impl Widget {
    Card::new(
        Column::new()
            .child(Container::new().background(color).size(60.0, 60.0))
            .child(Text::label(state))
    )
}

fn main() {
    App::run(Phase6Demo);
}
