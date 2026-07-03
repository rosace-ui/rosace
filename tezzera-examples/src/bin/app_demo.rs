// ── App Demo — the canonical TEZZERA showcase ─────────────────────────────────
//
// One app exercising every core system:
// - Scaffold + AppBar (leading back button, dropdown menu, theme toggle)
// - Routing: ScreenNav<Screen> push/pop
// - ScrollView: vertical page scroll + a horizontal card carousel
// - Text: word wrap, max_lines, kerned sizes
// - Canvas quality: rounded cards, real shadows, circles, image blits
// - Overlays: Dialog, Menu, Sheet, Toast (scrim tap-to-dismiss)
// - Theme: dark/light toggle via set_theme()
//
// Run: cargo run --release -p tezzera-examples --bin app_demo

use tezzera::prelude::*;
use tezzera::theme::set_theme;
use tezzera_state::Atom;

#[derive(Clone, PartialEq)]
enum Screen {
    Home,
    About,
}

struct AppDemo;

impl Component for AppDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        // Hooks — unconditional, stable order.
        let nav = ScreenNav::new(ctx, Screen::Home);
        let is_dark:     Atom<bool> = ctx.state(true);
        let scroll_y:    Atom<f32>  = ctx.state(0.0f32);
        let carousel_x:  Atom<f32>  = ctx.state(0.0f32);
        let dialog_open: Atom<bool> = ctx.state(false);
        let menu_open:   Atom<bool> = ctx.state(false);
        let sheet_open:  Atom<bool> = ctx.state(false);
        let toast_open:  Atom<bool> = ctx.state(false);

        let screen = nav.current().unwrap_or(Screen::Home);

        // ── Body per screen ──────────────────────────────────────────────
        let body: BoxedWidget = match screen {
            Screen::Home => Box::new(ScrollView::live(
                home_content(&nav, &carousel_x, &dialog_open, &sheet_open, &toast_open),
                scroll_y.clone(),
            )),
            Screen::About => Box::new(about_content()),
        };

        // ── AppBar ───────────────────────────────────────────────────────
        let title = match screen {
            Screen::Home  => "Tezzera Demo",
            Screen::About => "About",
        };
        let mut bar = AppBar::new(title);

        if nav.can_pop() {
            let nav_back = nav.clone();
            bar = bar.leading(
                Button::new("← Back")
                    .variant(ButtonVariant::Ghost)
                    .on_press(move || { nav_back.pop(); }),
            );
        }

        // Dropdown menu in the AppBar.
        let m_open = menu_open.clone();
        let m_close = menu_open.clone();
        let m_nav = nav.clone();
        let m_close2 = menu_open.clone();
        bar = bar.action(
            Button::new("Menu ▾")
                .variant(ButtonVariant::Secondary)
                .on_press(move || m_open.set(true))
                .dropdown(menu_open.clone(), move || {
                    let close = m_close.clone();
                    let close2 = m_close2.clone();
                    let nav = m_nav.clone();
                    Box::new(
                        Menu::new()
                            .item("About this demo", move || {
                                close.set(false);
                                if nav.current() != Some(Screen::About) {
                                    nav.push(Screen::About);
                                }
                            })
                            .item("Close", move || close2.set(false)),
                    )
                }),
        );

        // Theme toggle.
        let label = if is_dark.get() { "☀ Light" } else { "🌙 Dark" };
        let is_dark_btn = is_dark.clone();
        bar = bar.action(Button::new(label).on_press(move || {
            let new_dark = !is_dark_btn.get();
            is_dark_btn.set(new_dark);
            set_theme(if new_dark { dark_theme() } else { light_theme() });
        }));

        Scaffold::new(body).app_bar(bar).into_element()
    }
}

// ── Home screen ───────────────────────────────────────────────────────────────

fn home_content(
    nav: &ScreenNav<Screen>,
    carousel_x: &Atom<f32>,
    dialog_open: &Atom<bool>,
    sheet_open: &Atom<bool>,
    toast_open: &Atom<bool>,
) -> impl Widget {
    // Overlay atom clones for the closures below.
    let d_open = dialog_open.clone();
    let d_cancel = dialog_open.clone();
    let d_confirm = dialog_open.clone();
    let d_toast = toast_open.clone();
    let s_open = sheet_open.clone();
    let t_open = toast_open.clone();
    let nav_about = nav.clone();

    Column::new()
        .spacing(20.0)
        .padding(EdgeInsets::all(24.0))
        // Wrapped text — resize the window and the caption reflows.
        .child(Text::display("Tezzera UI"))
        .child(Text::caption(
            "A declarative Rust UI framework with a CPU compositor, reactive \
             atom-based state, and a full widget library. This caption word-wraps \
             with real kerned font metrics — resize the window to watch it reflow \
             onto the next line.",
        ))
        // Horizontal carousel — scroll sideways (trackpad swipe / shift+wheel).
        .child(Text::heading("Feature carousel — scrolls horizontally"))
        .child(SizedBox::new().height(140.0).child(
            ScrollView::new(
                Row::new().spacing(12.0)
                    .child(feature_card("Reactive", "Atom<T> state — zero overhead on idle frames"))
                    .child(feature_card("Composable", "Column · Row · Stack · Scaffold · ScrollView"))
                    .child(feature_card("Themeable", "Dark / light with one set_theme() call"))
                    .child(feature_card("Navigable", "ScreenNav stack — push, pop, replace"))
                    .child(feature_card("Layered", "Dialog · Menu · Sheet · Toast overlays")),
            )
            .live_x(carousel_x.clone())
            .axis(ScrollAxis::Horizontal),
        ))
        // Overlay triggers.
        .child(Text::heading("Overlays"))
        .child(
            Row::new().spacing(8.0)
                .child(
                    Button::new("Delete…")
                        .variant(ButtonVariant::Danger)
                        .on_press(move || d_open.set(true))
                        .dialog(dialog_open.clone(), move || {
                            let cancel = d_cancel.clone();
                            let confirm = d_confirm.clone();
                            let toast = d_toast.clone();
                            Box::new(
                                Dialog::new("Delete item?")
                                    .message("This action cannot be undone. Tap the scrim to dismiss.")
                                    .action("Cancel", move || cancel.set(false))
                                    .destructive_action("Delete", move || {
                                        confirm.set(false);
                                        Toast::show(&toast, 2.5);
                                    }),
                            )
                        }),
                )
                .child(
                    Button::new("Bottom Sheet")
                        .variant(ButtonVariant::Secondary)
                        .on_press(move || s_open.set(true))
                        .sheet(sheet_open.clone(), || {
                            Box::new(Sheet::new(
                                Column::new()
                                    .spacing(8.0)
                                    .child(Text::title("Sheet title"))
                                    .child(Text::caption(
                                        "Full-width bottom sheet with rounded top corners \
                                         and a grab handle. Tap the scrim to dismiss.",
                                    )),
                            ))
                        }),
                )
                .child(
                    Button::new("Show Toast")
                        .variant(ButtonVariant::Success)
                        .on_press(move || Toast::show(&t_open, 2.5))
                        .toast(toast_open.clone(), || {
                            Box::new(Toast::success("Action completed"))
                        }),
                ),
        )
        // Routing.
        .child(Text::heading("Navigation"))
        .child(
            Row::new().spacing(8.0)
                .child(Button::new("About this demo →")
                    .on_press(move || { nav_about.push(Screen::About); })),
        )
        // Image blits — placeholder grid (bilinear-scaled).
        .child(Text::heading("Images"))
        .child(
            Row::new().spacing(8.0)
                .child(Image::placeholder(Color::rgb(70, 100, 170)).width(180.0).height(120.0))
                .child(Image::placeholder(Color::rgb(170, 70, 100)).width(180.0).height(120.0))
                .child(Image::placeholder(Color::rgb(70, 170, 100)).width(180.0).height(120.0)),
        )
}

fn feature_card(title: &str, body: &str) -> impl Widget {
    SizedBox::new().width(240.0).child(
        Card::new(
            Column::new()
                .spacing(6.0)
                .child(Text::label(title).weight(FontWeight::Bold))
                .child(Text::caption(body)),
        )
        .radius(12.0)
        .elevation(6.0)
        .padding(EdgeInsets::all(16.0)),
    )
}

// ── About screen ──────────────────────────────────────────────────────────────

fn about_content() -> impl Widget {
    Column::new()
        .spacing(16.0)
        .padding(EdgeInsets::all(24.0))
        .child(Text::display("About"))
        .child(Text::new(
            "This screen was pushed onto the ScreenNav stack — the ← Back button \
             in the AppBar pops it. Screen state lives in an Atom<Vec<Screen>> so \
             the owning component rebuilds automatically on push and pop.\n\n\
             Explicit newlines are honored by the Text widget, and long paragraphs \
             wrap using greedy word-wrap with real kerned font metrics.",
        ).size(16.0))
        .child(Text::caption(
            "This caption is capped at two lines with max_lines(2), no matter how \
             much text is put into it — everything past the second line is simply \
             truncated away, which is exactly what happens to this sentence.",
        ).max_lines(2))
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let _ = env_logger::try_init();
    App::new()
        .title("Tezzera — App Demo")
        .size(900, 640)
        .theme(dark_theme())
        .launch(AppDemo);
}
