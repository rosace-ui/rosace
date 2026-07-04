// ── App Demo — the canonical TEZZERA feature gallery ─────────────────────────
//
// Home is an index; every feature lives on its own route (steering demo
// policy). Add a Screen variant + a tile + a screen fn for each new feature.
//
// Run: cargo run --release -p tezzera-examples --bin app_demo

use tezzera::prelude::*;
use tezzera::theme::set_theme;
use tezzera_state::Atom;

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Home,
    Typography,
    Scrolling,
    Overlays,
    VirtualList,
    Gallery,
}

impl Screen {
    fn title(&self) -> &'static str {
        match self {
            Screen::Home        => "Tezzera Gallery",
            Screen::Typography  => "Typography & Wrapping",
            Screen::Scrolling   => "Scrolling",
            Screen::Overlays    => "Overlays",
            Screen::VirtualList => "Virtualized List",
            Screen::Gallery     => "Widget Gallery",
        }
    }
}

struct AppDemo;

impl Component for AppDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        // Hooks — unconditional, stable order.
        let nav = ScreenNav::new(ctx, Screen::Home);
        let page_ctrl = ScrollController::for_ctx(ctx);
        let is_dark:     Atom<bool> = ctx.state(true);
        let dialog_open: Atom<bool> = ctx.state(false);
        let menu_open:   Atom<bool> = ctx.state(false);
        let sheet_open:  Atom<bool> = ctx.state(false);
        let toast_open:  Atom<bool> = ctx.state(false);
        let check_on:    Atom<bool> = ctx.state(true);
        let switch_on:   Atom<bool> = ctx.state(false);
        let slider_v:    Atom<f32>  = ctx.state(0.6f32);

        let screen = nav.current().unwrap_or(Screen::Home);

        let body: BoxedWidget = match screen {
            Screen::Home        => Box::new(home_screen(&nav)),
            Screen::Typography  => Box::new(typography_screen()),
            Screen::Scrolling   => Box::new(ScrollView::controlled(
                scrolling_screen(), page_ctrl.clone(),
            )),
            Screen::Overlays    => Box::new(overlays_screen(
                &dialog_open, &menu_open, &sheet_open, &toast_open,
            )),
            Screen::VirtualList => Box::new(virtual_list_screen()),
            Screen::Gallery     => Box::new(gallery_screen(&check_on, &switch_on, &slider_v)),
        };

        // ── AppBar: back appears off-Home; ⬆ Top only where it acts ──────
        let mut bar = AppBar::new(screen.title()).back_button(&nav);
        if screen == Screen::Scrolling {
            let top = page_ctrl.clone();
            bar = bar.action(Button::new("⬆ Top")
                .variant(ButtonVariant::Ghost)
                .on_press(move || top.scroll_to_top()));
        }
        let label = if is_dark.get() { "☀ Light" } else { "🌙 Dark" };
        let is_dark_btn = is_dark.clone();
        bar = bar.action(Button::new(label).on_press(move || {
            let dark = !is_dark_btn.get();
            is_dark_btn.set(dark);
            set_theme(if dark { dark_theme() } else { light_theme() });
        }));

        Scaffold::new(body).app_bar(bar).into_element()
    }
}

// ── Home: the feature index ───────────────────────────────────────────────────

fn home_screen(nav: &ScreenNav<Screen>) -> impl Widget {
    let tile = |title: &'static str, subtitle: &'static str, to: Screen, nav: &ScreenNav<Screen>| {
        let nav = nav.clone();
        ListTile::new(title)
            .subtitle(subtitle)
            .on_press(move || { nav.push(to); })
    };

    Column::new()
        .padding(EdgeInsets::all(16.0))
        .child(tile("Typography & Wrapping", "Sizes, kerning, word-wrap, max_lines", Screen::Typography, nav))
        .child(tile("Scrolling", "Controlled page scroll, carousel, doctrine", Screen::Scrolling, nav))
        .child(tile("Overlays", "Dialog, dropdown menu, sheet, toast", Screen::Overlays, nav))
        .child(tile("Virtualized List", "10,000 rows, built on demand", Screen::VirtualList, nav))
        .child(tile("Widget Gallery", "Buttons, inputs, chips, progress", Screen::Gallery, nav))
}

// ── Feature screens ───────────────────────────────────────────────────────────

fn typography_screen() -> impl Widget {
    Column::new()
        .spacing(14.0)
        .padding(EdgeInsets::all(24.0))
        .child(Text::display("Display 40"))
        .child(Text::heading("Heading 22 — SemiBold"))
        .child(Text::title("Title 20 — Medium"))
        .child(Text::label("Label 16"))
        .child(Text::caption("Caption 14 — and a long paragraph to show wrapping: \
            the text engine measures with real kerned advances, wraps greedily, \
            and honors explicit line breaks.\nLike this one."))
        .child(Text::caption(
            "This caption is capped at two lines with max_lines(2); everything \
             past the second line is truncated away, which is exactly what \
             happens to this very sentence you are reading right now.",
        ).max_lines(2))
}

fn scrolling_screen() -> impl Widget {
    Column::new()
        .spacing(20.0)
        .padding(EdgeInsets::all(24.0))
        .child(Text::caption(
            "This page scrolls via a ScrollController (⬆ Top in the AppBar \
             jumps home programmatically). The carousel below scrolls \
             horizontally — vertical wheel over it still scrolls the page \
             (axis-aware routing).",
        ))
        .child(Container::new().height(140.0).child(
            Row::new().spacing(12.0)
                .child(feature_card("Reactive", "Atom<T> state — zero overhead on idle frames"))
                .child(feature_card("Composable", "Column · Row · Stack · Scaffold · ScrollView"))
                .child(feature_card("Themeable", "Dark / light with one set_theme() call"))
                .child(feature_card("Navigable", "ScreenNav stack — push, pop, replace"))
                .child(feature_card("Layered", "Dialog · Menu · Sheet · Toast overlays"))
                .scrollable(),
        ))
        .child(Text::heading("Tall content to scroll"))
        .children((1..=12).map(|i| {
            Box::new(
                Card::new(Text::label(format!("Section {i} — keep scrolling")))
                    .radius(10.0).elevation(3.0),
            ) as BoxedWidget
        }).collect())
        .child(Text::caption("The end — press ⬆ Top."))
}

fn overlays_screen(
    dialog_open: &Atom<bool>,
    menu_open: &Atom<bool>,
    sheet_open: &Atom<bool>,
    toast_open: &Atom<bool>,
) -> impl Widget {
    let d_open = dialog_open.clone();
    let d_cancel = dialog_open.clone();
    let d_confirm = dialog_open.clone();
    let d_toast = toast_open.clone();
    let m_open = menu_open.clone();
    let m_a = menu_open.clone();
    let m_b = menu_open.clone();
    let s_open = sheet_open.clone();
    let t_open = toast_open.clone();

    Column::new()
        .spacing(16.0)
        .padding(EdgeInsets::all(24.0))
        .child(Text::caption(
            "Scrim tap and Escape dismiss. The menu clamps inside the window \
             and closes on any outside tap.",
        ))
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
                                    .message("This action cannot be undone.")
                                    .action("Cancel", move || cancel.set(false))
                                    .destructive_action("Delete", move || {
                                        confirm.set(false);
                                        Toast::show(&toast, 2.5);
                                    }),
                            )
                        }),
                )
                .child(
                    Button::new("Menu ▾")
                        .variant(ButtonVariant::Secondary)
                        .on_press(move || m_open.set(true))
                        .dropdown(menu_open.clone(), move || {
                            let a = m_a.clone();
                            let b = m_b.clone();
                            Box::new(
                                Menu::new()
                                    .item("First action", move || a.set(false))
                                    .item("Second action", move || b.set(false)),
                            )
                        }),
                )
                .child(
                    Button::new("Bottom Sheet")
                        .variant(ButtonVariant::Ghost)
                        .on_press(move || s_open.set(true))
                        .sheet(sheet_open.clone(), || {
                            Box::new(Sheet::new(
                                Column::new().spacing(8.0)
                                    .child(Text::title("Sheet title"))
                                    .child(Text::caption("Tap the scrim or press Escape to dismiss.")),
                            ))
                        }),
                )
                .child(
                    Button::new("Toast")
                        .variant(ButtonVariant::Success)
                        .on_press(move || Toast::show(&t_open, 2.5))
                        .toast(toast_open.clone(), || {
                            Box::new(Toast::success("Action completed"))
                        }),
                ),
        )
}

fn virtual_list_screen() -> impl Widget {
    Column::new()
        .spacing(12.0)
        .padding(EdgeInsets::all(24.0))
        .child(Text::caption(
            "10,000 rows; only the visible window is built, laid out, and \
             painted each frame (RecyclerView model).",
        ))
        .child(Container::new().height(430.0).child(
            ListView::builder(10_000, 40.0, |i| {
                Box::new(
                    Container::new()
                        .padding(EdgeInsets::symmetric(12.0, 10.0))
                        .child(Text::label(format!("Row {i} — built on demand"))),
                )
            }),
        ))
}

fn gallery_screen(check_on: &Atom<bool>, switch_on: &Atom<bool>, slider_v: &Atom<f32>) -> impl Widget {
    let c = check_on.clone();
    let c2 = check_on.clone();
    let sw = switch_on.clone();
    let sw2 = switch_on.clone();
    let sl = slider_v.clone();

    Column::new()
        .spacing(16.0)
        .padding(EdgeInsets::all(24.0))
        .child(Text::heading("Buttons"))
        .child(Row::new().spacing(8.0)
            .child(Button::new("Primary"))
            .child(Button::new("Secondary").variant(ButtonVariant::Secondary))
            .child(Button::new("Ghost").variant(ButtonVariant::Ghost))
            .child(Button::new("Danger").variant(ButtonVariant::Danger))
            .child(Button::new("Disabled").disabled()))
        .child(Text::heading("Inputs"))
        .child(Row::new().spacing(16.0)
            .child(Checkbox::new(check_on.get()).on_change(move |v| c.set(v)))
            .child(Text::label(if c2.get() { "checked" } else { "unchecked" }))
            .child(Switch::new(switch_on.get()).on_change(move |v| sw.set(v)))
            .child(Text::label(if sw2.get() { "on" } else { "off" })))
        .child(Text::caption("Click the slider track to set (drag lands with gesture events)"))
        .child(Slider::new(slider_v.get()).on_change(move |v| sl.set(v)))
        .child(ProgressBar::new(slider_v.get()))
        .child(Text::heading("Bits"))
        .child(Row::new().spacing(8.0)
            .child(Chip::new("Chip"))
            .child(Avatar::new("TZ"))
            .child(Badge::new("3")))
}

// ── Shared bits ───────────────────────────────────────────────────────────────

fn feature_card(title: &str, body: &str) -> impl Widget {
    Container::new().width(240.0).child(
        Card::new(
            Column::new().spacing(6.0)
                .child(Text::label(title).weight(FontWeight::Bold))
                .child(Text::caption(body)),
        )
        .radius(12.0)
        .elevation(6.0)
        .padding(EdgeInsets::all(16.0)),
    )
}

fn main() {
    let _ = env_logger::try_init();
    App::new()
        .title("Tezzera — Gallery")
        .size(900, 640)
        .theme(dark_theme())
        .launch(AppDemo);
}
