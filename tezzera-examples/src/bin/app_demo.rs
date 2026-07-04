// ── App Demo — the canonical TEZZERA feature gallery ─────────────────────────
//
// Home is an index; every feature lives on its own route (steering demo
// policy). Add a Screen variant + a tile + a screen fn for each new feature.
//
// Run: cargo run --release -p tezzera-examples --bin app_demo

use tezzera::prelude::*;
use tezzera::theme::{set_theme, set_animations};
use tezzera_state::Atom;

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Home,
    Typography,
    Scrolling,
    Overlays,
    VirtualList,
    Gallery,
    Showcase,
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
            Screen::Showcase    => "New Widgets",
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
        let press_count: Atom<i32>  = ctx.state(0);
        let switch_on:   Atom<bool> = ctx.state(false);
        let slider_v:    Atom<f32>  = ctx.state(0.6f32);
        let radio_sel:   Atom<usize> = ctx.state(0);
        let seg_sel:     Atom<usize> = ctx.state(1);
        let drop_open:   Atom<bool> = ctx.state(false);
        let drop_sel:    Atom<usize> = ctx.state(0);
        let exp_open:    Atom<bool> = ctx.state(true);
        let anim_on:     Atom<bool> = ctx.state(true);

        let screen = nav.current().unwrap_or(Screen::Home);

        let body: BoxedWidget = match screen {
            Screen::Home        => Box::new(home_screen(&nav)),
            Screen::Typography  => Box::new(ScrollView::new(typography_screen())),
            Screen::Scrolling   => Box::new(ScrollView::controlled(
                scrolling_screen(), page_ctrl.clone(),
            )),
            Screen::Overlays    => Box::new(ScrollView::new(overlays_screen(
                &dialog_open, &menu_open, &sheet_open, &toast_open,
            ))),
            Screen::VirtualList => Box::new(virtual_list_screen()),
            Screen::Gallery     => Box::new(ScrollView::new(gallery_screen(&check_on, &switch_on, &slider_v, &press_count))),
            Screen::Showcase    => Box::new(ScrollView::new(showcase_screen(&radio_sel, &seg_sel, &drop_open, &drop_sel, &exp_open, &anim_on))),
        };

        // ── AppBar: back appears off-Home; ⬆ Top only where it acts ──────
        let mut bar = AppBar::new(screen.title()).back_button(&nav);
        if screen == Screen::Scrolling {
            let top = page_ctrl.clone();
            bar = bar.action(Button::new("⬆ Top")
                .variant(ButtonVariant::Ghost)
                .on_press(move || top.scroll_to_top()));
        }
        let label = if is_dark.get() { "☀ Light" } else { "☾ Dark" };
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
        .child(tile("New Widgets", "Shapes, grid, spinner, radio, dropdown…", Screen::Showcase, nav))
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

fn gallery_screen(check_on: &Atom<bool>, switch_on: &Atom<bool>, slider_v: &Atom<f32>, press_count: &Atom<i32>) -> impl Widget {
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
        .child(Text::caption("Click OR drag the slider"))
        .child(Slider::new(slider_v.get()).on_change(move |v| sl.set(v)))
        .child(ProgressBar::new(slider_v.get()))
        .child(Text::heading("Pressable anything"))
        .child(Row::new().spacing(12.0)
            .child(
                Card::new(Text::label("This whole card is .on_press()"))
                    .radius(10.0).elevation(3.0)
                    .on_press({ let p = press_count.clone(); move || p.set(p.get() + 1) }),
            )
            .child(Text::label(format!("pressed {} times", press_count.get()))))
        .child(Text::heading("Bits"))
        .child(Row::new().spacing(8.0)
            .child(Chip::new("Chip"))
            .child(Avatar::new("TZ"))
            .child(Badge::new("3")))
        .child(Text::heading("Hover & long-press"))
        .child(Row::new().spacing(12.0)
            .child(Tooltip::new("I appear on hover!", Button::new("Hover me")))
            .child(
                Card::new(Text::label("Long-press me (500ms)"))
                    .radius(10.0).elevation(2.0)
                    .on_long_press({ let p = press_count.clone(); move || p.set(p.get() + 100) }),
            ))
}


fn showcase_screen(radio_sel: &Atom<usize>, seg_sel: &Atom<usize>, drop_open: &Atom<bool>, drop_sel: &Atom<usize>, exp_open: &Atom<bool>, anim_on: &Atom<bool>) -> impl Widget {
    let an = anim_on.clone();
    let rs = radio_sel.clone(); let rs2 = radio_sel.clone(); let rs3 = radio_sel.clone();
    let ss = seg_sel.clone();
    let ds = drop_sel.clone();
    Column::new().spacing(18.0).padding(EdgeInsets::all(24.0))
        .child(Row::new().spacing(10.0).cross_axis_alignment(CrossAxisAlignment::Center)
            .child(Text::label("Animations (theme global)"))
            .child(Switch::new(an.get()).on_change(move |v| { an.set(v); set_animations(v); })))
        .child(Text::caption("Toggle above, then flip a switch/checkbox/radio below — they ease when on, snap when off."))
        .child(Text::heading("Container shapes"))
        .child(Wrap::new().spacing(12.0).run_spacing(12.0)
            .child(Container::new().size(56.0, 56.0).background(Color::rgb(120, 90, 220)).circle())
            .child(Container::new().size(120.0, 40.0).background(Color::rgb(60, 170, 120)).stadium()
                .align(Alignment::Center).child(Text::label("Stadium")))
            .child(Container::new().size(120.0, 56.0).gradient(Color::rgb(255, 110, 90), Color::rgb(150, 60, 220)).radius(12.0))
            .child(Container::new().size(56.0, 56.0).background(Color::rgb(90, 130, 210)).radius(10.0)
                .border(Color::WHITE, 2.0).clip()))
        .child(Text::heading("Grid & Wrap"))
        .child(Grid::new(4).spacing(8.0).run_spacing(8.0)
            .children((0..8).map(|i| Box::new(
                Container::new().height(40.0).radius(8.0)
                    .background(Color::rgb(50 + i*20, 60, 110)).align(Alignment::Center)
                    .child(Text::caption(format!("{}", i+1)))) as BoxedWidget).collect()))
        .child(Wrap::new().spacing(8.0).run_spacing(8.0)
            .children(["design","rust","ui","fast","native","reactive","themeable"].iter()
                .map(|t| Box::new(Chip::new(*t)) as BoxedWidget).collect()))
        .child(Text::heading("Progress & Skeleton"))
        .child(Wrap::new().spacing(20.0).run_spacing(12.0)
            .child(CircularProgress::new(0.65))
            .child(CircularProgress::spinner())
            .child(Container::new().width(160.0).child(Column::new().spacing(8.0)
                .child(Skeleton::new().height(14.0))
                .child(Skeleton::new().width(110.0).height(14.0)))))
        .child(Text::heading("AspectRatio 16:9"))
        .child(Container::new().width(240.0).child(
            AspectRatio::new(16.0/9.0, Container::new().gradient(Color::rgb(40,50,90), Color::rgb(20,24,44)).radius(10.0)
                .align(Alignment::Center).child(Text::caption("16:9")))))
        .child(Text::heading("Radio · Segmented · Dropdown"))
        .child(Row::new().spacing(8.0).cross_axis_alignment(CrossAxisAlignment::Center)
            .child(Radio::new(rs.get()==0).on_select({let r=rs.clone(); move|| r.set(0)}))
            .child(Text::label("One"))
            .child(Radio::new(rs2.get()==1).on_select({let r=rs2.clone(); move|| r.set(1)}))
            .child(Text::label("Two"))
            .child(Radio::new(rs3.get()==2).on_select({let r=rs3.clone(); move|| r.set(2)}))
            .child(Text::label("Three")))
        .child(SegmentedControl::new(vec!["Day","Week","Month"], ss.get())
            .on_change({let s=ss.clone(); move|i| s.set(i)}))
        .child(Dropdown::new(vec!["Rust","Swift","Kotlin","Dart"], ds.get(), drop_open.clone())
            .on_change({let d=ds.clone(); move|i| d.set(i)}))
        .child(Text::heading("Expander"))
        .child(Expander::new("Show details", exp_open.clone(),
            Text::caption("Collapsible body content revealed while expanded. Tap the header row to toggle.")))
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
