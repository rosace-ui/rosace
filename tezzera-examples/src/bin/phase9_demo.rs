use tezzera::prelude::*;
use tezzera_state::Atom;
use tezzera::shaping::{FallbackShaper, ShapingEngine};
use tezzera::style::{StyleSheet, StyleRule, StyleValue, StyleProperty, Selector, ComputedStyle};
use tezzera::theme::Color as ThemeColor;
use tezzera::text::direction::TextDirection;

#[derive(Debug, Clone, PartialEq)]
enum Panel { Shaping, Styles, Cli, Overview }

struct Phase9Demo;

impl Component for Phase9Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let panel: Atom<Panel> = ctx.state(Panel::Shaping);

        let p1 = panel.clone(); let p2 = panel.clone();
        let p3 = panel.clone(); let p4 = panel.clone();
        let nav_row = Row::new()
            .child(Button::new("Shaping").on_press(move || p1.set(Panel::Shaping)))
            .child(Button::new("Styles").on_press(move || p2.set(Panel::Styles)))
            .child(Button::new("CLI").on_press(move || p3.set(Panel::Cli)))
            .child(Button::new("Overview").on_press(move || p4.set(Panel::Overview)));

        let body: Box<dyn Widget> = match panel.get() {
            Panel::Shaping => {
                let samples = [
                    ("Hello", TextDirection::Ltr),
                    ("World!", TextDirection::Ltr),
                    ("مرحبا", TextDirection::Rtl),
                ];
                let mut col = Column::new()
                    .child(nav_row)
                    .child(Text::display("Text Shaping"));
                if let Some(shaper) = FallbackShaper::system() {
                    col = col.child(Text::label("FallbackShaper (fontdue-backed) — GlyphRun:"));
                    for (text, dir) in &samples {
                        let run = shaper.shape(text, 14.0, *dir);
                        col = col.child(Card::new(
                            Row::new()
                                .child(Text::label(format!("\"{}\"", text)))
                                .child(Text::caption(format!(
                                    "{} glyphs  total_advance={:.1}px  script={:?}",
                                    run.glyph_count(), run.total_advance(), run.script
                                )))
                        ));
                    }
                } else {
                    col = col.child(Text::label("No system font available — FallbackShaper::system() returned None"));
                }
                col = col.child(Text::caption("HarfBuzz slots into ShapingEngine trait in v1.0"));
                Box::new(col)
            }

            Panel::Styles => {
                let mut sheet = StyleSheet::new();
                sheet.add_rule(
                    StyleRule::new(Selector::class("btn"))
                        .set(StyleProperty::Background, StyleValue::color(
                            ThemeColor::rgb(0.31, 0.47, 0.78)
                        ))
                        .set(StyleProperty::Padding, StyleValue::px(12.0))
                        .set(StyleProperty::BorderRadius, StyleValue::px(6.0))
                );
                sheet.add_rule(
                    StyleRule::new(Selector::class("danger"))
                        .set(StyleProperty::Background, StyleValue::color(
                            ThemeColor::rgb(0.78, 0.24, 0.24)
                        ))
                );
                let computed = ComputedStyle::resolve(&sheet, &Selector::class("btn"), None);
                let padding = computed.padding_px().unwrap_or(0.0);

                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Style System"))
                        .child(Text::label("StyleSheet rules for .btn and .danger:"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading(".btn"))
                                .child(Text::caption(format!("padding={:.0}px, border-radius=6px", padding)))
                                .child(Text::caption("background=rgb(80,120,200)"))
                        ))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading(".danger"))
                                .child(Text::caption("background=rgb(200,60,60)"))
                        ))
                        .child(Text::caption("ComputedStyle::resolve(sheet, selector, inline) → merged result"))
                        .child(Text::caption("Selector: Id | Class | Element | Any"))
                )
            }

            Panel::Cli => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("CLI Commands"))
                        .child(
                            Row::new()
                                .child(cli_card("tzr check", "cargo check --workspace"))
                                .child(cli_card("tzr test", "cargo test --workspace"))
                        )
                        .child(
                            Row::new()
                                .child(cli_card("tzr lint", "cargo clippy -- -D warnings"))
                                .child(cli_card("tzr fmt", "cargo fmt --check"))
                        )
                        .child(
                            Row::new()
                                .child(cli_card("tzr analyze", "Workspace health report"))
                                .child(cli_card("tzr snapshot", "Run example → PNG"))
                        )
                        .child(cli_card("tzr package", "cargo build --release + manifest.json"))
                )
            }

            Panel::Overview => {
                let layers = [
                    ("Foundation", "tezzera-core  tezzera-state  tezzera-trace"),
                    ("Layout",     "tezzera-layout  tezzera-render  tezzera-platform"),
                    ("Widgets",    "tezzera-widgets  tezzera-theme  tezzera-scroll"),
                    ("Nav & Anim", "tezzera-nav  tezzera-nav-anim  tezzera-anim  tezzera-animate"),
                    ("Input",     "tezzera-gesture  tezzera-text  tezzera-clipboard  tezzera-ime"),
                    ("Network",   "tezzera-net  tezzera-ws  tezzera-i18n"),
                    ("System",    "tezzera-bidi  tezzera-style  tezzera-shaping  tezzera-renderer"),
                    ("DX",        "tezzera-macros  tezzera-cli  tezzera-devtools  tezzera-test-utils"),
                    ("Media",     "tezzera-media  tezzera-a11y  tezzera-forms  tezzera-hot-reload"),
                ];
                let mut col = Column::new()
                    .child(nav_row)
                    .child(Text::display("Framework Overview — 27 Crates"));
                for (layer, crates) in &layers {
                    col = col.child(Card::new(
                        Column::new()
                            .child(Text::heading(*layer))
                            .child(Text::caption(*crates))
                    ));
                }
                Box::new(col)
            }
        };

        Scaffold::new(body)
            .app_bar(AppBar::new("Phase 9 — Shaping · Styles · CLI · Overview"))
            .into_element()
    }
}

fn cli_card(cmd: &str, desc: &str) -> impl Widget {
    Card::new(
        Column::new()
            .child(Text::heading(cmd))
            .child(Text::caption(desc))
    )
}

fn main() {
    App::run(Phase9Demo);
}
