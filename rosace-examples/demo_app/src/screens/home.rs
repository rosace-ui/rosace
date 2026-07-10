//! The home screen — an index of the app's feature demos.

use rosace::prelude::*;

use crate::app::Screen;

pub fn home_screen(nav: &ScreenNav<Screen>) -> impl Widget {
    let nav = nav.clone();
    let nav_hero = nav.clone();
    Column::new()
        .padding(EdgeInsets::all(16.0))
        .spacing(4.0)
        .child(
            Row::new()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(16.0)
                .child(
                    Image::file("assets_photo.png")
                        .width(72.0)
                        .height(48.0)
                        .on_press({
                            let nav = nav_hero.clone();
                            move || { nav.push(Screen::HeroDetail); }
                        })
                        .hero_tag("cover-photo"),
                )
                .child(
                    Column::new()
                        .child(Text::title("Shared-element photo"))
                        .child(Text::caption("Tap the thumbnail — it morphs into the detail screen (D108 Hero)")),
                ),
        )
        .child(Spacer::gap(0.0, 12.0))
        .child(
            ListTile::new("Buttons")
                .subtitle("Variants, sizes, disabled state, press feedback")
                .on_press({ let nav = nav.clone(); move || { nav.push(Screen::Buttons); } }),
        )
        .child(
            ListTile::new("Gallery")
                .subtitle("Card, Chip, Avatar, Badge, progress indicators")
                .on_press({ let nav = nav.clone(); move || { nav.push(Screen::Gallery); } }),
        )
        .child(
            ListTile::new("Inputs")
                .subtitle("Switch, Checkbox, Radio, Slider, Dropdown, SegmentedControl")
                .on_press({ let nav = nav.clone(); move || { nav.push(Screen::Inputs); } }),
        )
        .child(
            ListTile::new("Overlays")
                .subtitle("Dialog, Sheet, Toast, Tooltip, Menu")
                .on_press({ let nav = nav.clone(); move || { nav.push(Screen::Overlays); } }),
        )
        .child(
            ListTile::new("Scroll")
                .subtitle("A long list — momentum/bounce scroll physics")
                .on_press({ let nav = nav.clone(); move || { nav.push(Screen::Scroll); } }),
        )
        .child(
            ListTile::new("Counter")
                .subtitle("A simple counter with + / \u{2212}")
                .no_divider()
                .on_press(move || {
                    nav.push(Screen::Counter);
                }),
        )
        .scrollable()
}
