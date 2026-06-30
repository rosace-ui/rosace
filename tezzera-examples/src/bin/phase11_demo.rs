use tezzera::prelude::*;
use tezzera_state::Atom;

#[derive(Debug, Clone, PartialEq)]
enum Panel { Macros, Analyze, Snapshot, DxSummary }

struct Phase11Demo;

impl Component for Phase11Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let panel: Atom<Panel> = ctx.state(Panel::Macros);

        let p1 = panel.clone(); let p2 = panel.clone();
        let p3 = panel.clone(); let p4 = panel.clone();
        let nav_row = Row::new()
            .child(Button::new("Macros").on_press(move || p1.set(Panel::Macros)))
            .child(Button::new("Analyze").on_press(move || p2.set(Panel::Analyze)))
            .child(Button::new("Snapshot").on_press(move || p3.set(Panel::Snapshot)))
            .child(Button::new("DX Summary").on_press(move || p4.set(Panel::DxSummary)));

        let body: Box<dyn Widget> = match panel.get() {
            Panel::Macros => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Proc-Macros"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("#[component]"))
                                .child(Text::caption("pub fn Greeting(name: String) -> Element { ... }"))
                                .child(Text::caption("↓ expands to impl Component for Greeting { ... }"))
                        ))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("#[state]"))
                                .child(Text::caption("pub count: i32 = 0;"))
                                .child(Text::caption("↓ pub count: Atom<i32> = Atom::new(0);"))
                        ))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("view! { }"))
                                .child(Text::caption("view! { Column { Text { \"Hello\" } Button { \"OK\" } } }"))
                                .child(Text::caption("↓ Column::new().child(Text::new(\"Hello\"))"))
                                .child(Text::caption("         .child(Button::new(\"OK\"))"))
                        ))
                )
            }

            Panel::Analyze => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("tzr analyze"))
                        .child(Text::label("Workspace health without running cargo:"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("AnalyzeReport"))
                                .child(Text::caption("Workspace: tezzera"))
                                .child(Text::caption("Crates:    27"))
                                .child(Text::caption("Status:    OK"))
                        ))
                        .child(
                            Row::new()
                                .child(stat_card("27", "Total Crates"))
                                .child(stat_card("11", "Phases Complete"))
                                .child(stat_card("0", "Test Failures"))
                                .child(stat_card("0", "Warnings"))
                        )
                        .child(Text::caption("parse_members() reads [workspace] members = [...] from Cargo.toml"))
                        .child(Text::caption("No cargo_metadata dep — manual TOML string parse"))
                )
            }

            Panel::Snapshot => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("tzr snapshot"))
                        .child(Text::label("Run example → save PNG golden file:"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::caption("$ tzr snapshot --example phase11_demo --out snapshots/"))
                                .child(Text::caption("Running: cargo run -p tezzera-examples --bin phase11_demo"))
                                .child(Text::caption("Saved:   snapshots/phase11_demo.png"))
                        ))
                        .child(Text::label("SnapshotAssert flow:"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::caption("1. save_snapshot(name, png) → test_snapshots/<name>.png"))
                                .child(Text::caption("2. assert_snapshot(name, png) → pixel-by-pixel diff"))
                                .child(Text::caption("3. pixel_diff_count(a, b) → usize (panic if > threshold)"))
                        ))
                        .child(Text::caption("SnapshotOptions::from_args([\"--example\", \"...\", \"--out\", \"...\"])"))
                )
            }

            Panel::DxSummary => {
                let phases = [
                    ("Phase 1",  "Core, State, Layout, Render, Platform"),
                    ("Phase 2",  "Spring animations, Theme, more Widgets"),
                    ("Phase 3",  "Dev tools, tzr dev --watch, snapshots"),
                    ("Phase 4",  "Image, Tooltip, NavRail, tree widgets"),
                    ("Phase 5",  "Overlays, transitions, templates, a11y stubs"),
                    ("Phase 6",  "Gestures, Rich text, Network images, i18n"),
                    ("Phase 7",  "Glyph metrics, RTL, Clipboard, WS, Pinch"),
                    ("Phase 8",  "Renderer abstraction, IME, Bidi, Media stubs"),
                    ("Phase 9",  "Text shaping, Style system, CLI polish"),
                    ("Phase 10", "Animation, A11y tree, Test harness, Package"),
                    ("Phase 11", "Macros, tzr analyze, tzr snapshot, DX closure"),
                ];
                let mut col = Column::new()
                    .child(nav_row)
                    .child(Text::display("TEZZERA — DX Summary"));
                for (phase, desc) in &phases {
                    col = col.child(
                        Row::new()
                            .child(Text::label(*phase))
                            .child(Text::caption(*desc))
                    );
                }
                col = col.child(Text::caption("27 crates · 11 phases · 0 warnings · release clean"));
                Box::new(col)
            }
        };

        Scaffold::new(body)
            .app_bar(AppBar::new("Phase 11 — Macros · Analyze · Snapshot · DX"))
            .into_element()
    }
}

fn stat_card(value: &str, label: &str) -> impl Widget {
    Card::new(
        Column::new()
            .child(Text::display(value))
            .child(Text::caption(label))
    )
}

fn main() {
    App::run(Phase11Demo);
}
