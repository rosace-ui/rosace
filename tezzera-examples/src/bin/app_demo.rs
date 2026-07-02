// ── App Demo — canonical app structure ───────────────────────────────────────
//
// App { Scaffold { AppBar { ThemeButton }, Body { Column { Image, … } } } }
//
// Exercises: theme toggle, Scaffold layout, AppBar actions, Image widget,
// ScrollView body, Column spacing/padding. Run it to surface rendering issues.

use tezzera::prelude::*;
use tezzera::theme::{set_theme};
use tezzera_state::Atom;

struct AppDemo;

impl Component for AppDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let is_dark: Atom<bool> = ctx.state(true);
        let dark = is_dark.get();
        // Scroll position must live in component state (not the widget) so it
        // survives rebuilds. ScrollView::live registers the wheel handler;
        // ScrollView::new is a fixed-offset snapshot view and does not scroll.
        let scroll_y: Atom<f32> = ctx.state(0.0f32);
        let scroll_x: Atom<f32> = ctx.state(0.0f32);

        // Toggle label
        let label = if dark { "☀ Light" } else { "🌙 Dark" };
        let is_dark_btn = is_dark.clone();

        Scaffold::new(
            // ── Body ─────────────────────────────────────────────────────────
            ScrollView::live(
                Column::new()
                    .spacing(20.0)
                    .padding(EdgeInsets::all(24.0))
                    // Hero image (uses app_showcase.png if present, else placeholder)
                    .child(
                        Image::file("tezzera-examples/app_showcase.png")
                            .width(640.0)
                            .height(320.0)
                            .fit(ImageFit::Contain)
                    )
                    // Section header
                    .child(Text::display("Tezzera UI Framework").weight(FontWeight::Bold))
                    .child(Text::caption(
                        "A declarative Rust UI framework with a CPU compositor, \
                         reactive atom-based state, and a full widget library."
                    ))
                    // Feature cards row
                    .child(
                        Row::new().spacing(12.0)
                            .child(feature_card("Reactive", "Atom<T> state — zero overhead on idle frames"))
                            .child(feature_card("Composable", "Column · Row · Stack · Scaffold · ScrollView"))
                            .child(feature_card("Themeable", "Dark / light with one set_theme() call"))
                    )
                    // Interactive row
                    .child(
                        Row::new().spacing(12.0)
                            .child(Button::new("Primary Action").variant(ButtonVariant::Primary))
                            .child(Button::new("Secondary").variant(ButtonVariant::Secondary))
                            .child(Button::new("Ghost").variant(ButtonVariant::Ghost))
                    )
                    // Placeholder image grid
                    .child(Text::caption("Sample image grid"))
                    .child(
                        Row::new().spacing(8.0)
                            .child(Image::placeholder(Color::rgb(70, 100, 170)).width(180.0).height(120.0))
                            .child(Image::placeholder(Color::rgb(170, 70, 100)).width(180.0).height(120.0))
                            .child(Image::placeholder(Color::rgb(70, 170, 100)).width(180.0).height(120.0))
                    ),
                scroll_y,
            )
            .live_x(scroll_x)
            .axis(ScrollAxis::Both)
        )
        .app_bar(
            AppBar::new("My App")
                .action(
                    Button::new(label)
                        .on_press(move || {
                            let new_dark = !is_dark_btn.get();
                            // 1. Mark component dirty → triggers rebuild + repaint
                            is_dark_btn.set(new_dark);
                            // 2. Write to global theme store → picked up by lib.rs
                            //    via use_theme() on the very next frame
                            set_theme(if new_dark {
                                dark_theme()
                            } else {
                                light_theme()
                            });
                        })
                )
        )
        .into_element()
    }
}

fn feature_card(title: &str, body: &str) -> impl Widget {
    Container::new()
        .padding(EdgeInsets::all(16.0))
        .child(
            Column::new()
                .spacing(6.0)
                .child(Text::label(title).weight(FontWeight::Bold))
                .child(Text::caption(body))
        )
}

fn main() {
    let _ = env_logger::try_init();
    App::new()
        .title("Tezzera — App Demo")
        .size(800, 600)
        .theme(dark_theme())
        .launch(AppDemo);
}
