//! The counter screen. `count` is app-wide state owned by the root component,
//! so it survives navigating away and back.

use rosace::prelude::*;

pub fn counter_screen(count: &Atom<i32>) -> impl Widget {
    let c = count.clone();
    Column::new()
        .spacing(16.0)
        .padding(EdgeInsets::all(24.0))
        .child(Spacer::gap(0.0, 48.0))
        .child(Text::display(count.get().to_string()).align(TextAlign::Center))
        .child(Text::new("Tap to change the count").align(TextAlign::Center))
        .child(Spacer::gap(0.0, 24.0))
        .child(
            Row::new()
                .main_axis_alignment(MainAxisAlignment::Center)
                .spacing(12.0)
                .child(
                    Button::new("\u{2212}")
                        .variant(ButtonVariant::Ghost)
                        .width(44.0)
                        .on_press({
                            let c = c.clone();
                            move || c.set(c.get() - 1)
                        }),
                )
                .child(Button::new("Reset").width(140.0).on_press({
                    let c = c.clone();
                    move || c.set(0)
                }))
                .child(
                    Button::new("+")
                        .variant(ButtonVariant::Ghost)
                        .width(44.0)
                        .on_press({
                            let c = c.clone();
                            move || c.set(c.get() + 1)
                        }),
                ),
        )
}
