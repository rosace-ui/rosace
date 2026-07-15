//! Widget gallery (Phase 32/D115): every major built-in widget with a
//! minimal customization example — the living catalog the customization
//! sweep checks itself against.
//!
//! Run: `cargo run -p rosace-examples --bin widget_gallery`

use rosace::prelude::*;
use rosace::render::Color;

fn section(title: &str) -> impl Widget {
    Text::title(title).size(15.0)
}

struct Gallery;

impl Component for Gallery {
    fn build(&self, ctx: &mut Context) -> Element {
        let checked = ctx.state(true);
        let switch_on = ctx.state(true);
        let seg = ctx.state(0usize);
        let dd_open = ctx.state(false);
        let expanded = ctx.state(true);
        let search = ctx.state(String::new());
        let snack_open = ctx.state(false);
        let fab_count = ctx.state(0i32);
        let slider_val = ctx.state(0.4f32);
        let typed = ctx.state(String::new());

        let mut col = Column::new()
            .padding(EdgeInsets::all(20.0))
            .spacing(10.0)
            // ── Buttons ────────────────────────────────────────────
            .child(section("Buttons — variants, radius, disabled"))
            .child(
                Row::new()
                    .spacing(8.0)
                    .child(Button::new("Primary").width(90.0))
                    .child(Button::new("Ghost").variant(ButtonVariant::Ghost).width(80.0))
                    .child(Button::new("Link").variant(ButtonVariant::Link).width(60.0))
                    .child(Button::new("Round").width(80.0).radius(17.0))
                    .child(Button::new("Off").width(60.0).disabled()),
            )
            .child(
                Row::new()
                    .spacing(12.0)
                    .child(FloatingActionButton::new().size(40.0).on_press({
                        let c = fab_count.clone();
                        move || c.set(c.get() + 1)
                    }))
                    .child(FloatingActionButton::new().size(40.0).radius(10.0).background(Color::rgb(72, 199, 116)).label("OK"))
                    .child(Text::new(format!("FAB presses: {}", fab_count.get()))),
            )
            // ── Text & badges ──────────────────────────────────────
            .child(section("Text, Icon, Avatar, Badge, Chip"))
            .child(
                Row::new()
                    .spacing(10.0)
                    .child(Text::new("body"))
                    .child(Text::new("bold").weight(FontWeight::Bold))
                    .child(Text::new("colored").color(Color::rgb(187, 134, 252)))
                    .child(Icon::new(IconKind::Star).size(16.0))
                    .child(Avatar::new("R").size(24.0))
                    .child(Badge::count(7))
                    .child(Badge::label("beta").color(Color::rgb(40, 60, 90)).text_color(Color::rgb(120, 180, 255)))
                    .child(Chip::new("chip"))
                    .child(Chip::new("selected").selected()),
            )
            // ── Selection controls ─────────────────────────────────
            .child(section("Checkbox, Switch, Slider, SegmentedControl"))
            .child(
                Row::new()
                    .spacing(14.0)
                    .child(Checkbox::new(checked.get()).on_change({
                        let c = checked.clone();
                        move |v| c.set(v)
                    }))
                    .child(Switch::new(switch_on.get()).on_change({
                        let s = switch_on.clone();
                        move |v| s.set(v)
                    }))
                    .child(Slider::new(slider_val.get()).width(140.0).on_change({
                        let s = slider_val.clone();
                        move |v| s.set(v)
                    }))
                    .child(
                        SegmentedControl::new(vec!["Day", "Week", "Month"], seg.get()).on_change({
                            let s = seg.clone();
                            move |i| s.set(i)
                        }),
                    ),
            )
            // ── Progress ───────────────────────────────────────────
            .child(section("ProgressBar, CircularProgress, Skeleton"))
            .child(
                Row::new()
                    .spacing(14.0)
                    .child(ProgressBar::new(0.65).color(Color::rgb(72, 199, 116)))
                    .child(CircularProgress::spinner().diameter(22.0))
                    .child(Skeleton::new().width(120.0).height(16.0)),
            )
            // ── Inputs ─────────────────────────────────────────────
            .child(section("TextInput, SearchBar"))
            .child(
                Row::new()
                    .spacing(14.0)
                    .child(TextInput::new().value(typed.get()).placeholder("plain input").width(170.0).on_change({
                        let t = typed.clone();
                        move |v| t.set(v)
                    }))
                    .child(
                        SearchBar::new()
                            .value(search.get())
                            .width(170.0)
                            .on_change({
                                let s = search.clone();
                                move |v| s.set(v)
                            })
                            .on_clear({
                                let s = search.clone();
                                move || s.set(String::new())
                            }),
                    ),
            )
            // ── Structure ──────────────────────────────────────────
            .child(section("Card, Container, Grid, Expander, Dropdown"))
            .child(
                Row::new()
                    .spacing(12.0)
                    .child(Card::new(Text::new("in a Card")))
                    .child(
                        Container::new()
                            .child(Text::new("custom container"))
                            .background(Color::rgb(30, 34, 60))
                            .radius(12.0)
                            .padding(EdgeInsets::all(10.0)),
                    )
                    .child(Dropdown::new(vec!["One", "Two", "Three"], 0, dd_open.clone()).width(110.0)),
            )
            .child(
                Grid::new(4)
                    .spacing(6.0)
                    .children((0..4).map(|i| {
                        Box::new(
                            Container::new()
                                .child(Text::new(format!("cell {i}")).size(11.0))
                                .background(Color::rgb(24 + i as u8 * 8, 28, 52))
                                .radius(6.0)
                                .padding(EdgeInsets::all(8.0)),
                        ) as BoxedWidget
                    }).collect()),
            )
            .child(Expander::new("Expander — click to toggle", expanded.clone(), Text::new("expanded body content")))
            // ── Feedback ───────────────────────────────────────────
            .child(section("Snackbar (press to show), Tooltip"))
            .child(
                Row::new()
                    .spacing(12.0)
                    .child(Button::new("Show snackbar").width(130.0).on_press({
                        let o = snack_open.clone();
                        move || Snackbar::show(&o, 2.5)
                    }))
                    .child(Tooltip::new("hover me", Text::new("hover me (tooltip)"))),
            );

        if snack_open.get() {
            col = col.child(Snackbar::new("Item archived").action("UNDO", || {}));
        }

        // Bottom navigation showcased in its natural Scaffold slot.
        let bar = BottomNavigationBar::new()
            .item(BottomNavItem::new("Widgets").icon(Icon::new(IconKind::Home).size(18.0)).active().badge(2))
            .item(BottomNavItem::new("Themes").icon(Icon::new(IconKind::Settings).size(18.0)))
            .item(BottomNavItem::new("About").icon(Icon::new(IconKind::User).size(18.0)));

        Scaffold::new(ScrollView::new(col))
            .app_bar(AppBar::new("Widget Gallery"))
            .bottom_bar(bar)
            .into_element()
    }
}

fn main() {
    App::new().title("Widget Gallery").size(760, 860).launch(Gallery);
}
