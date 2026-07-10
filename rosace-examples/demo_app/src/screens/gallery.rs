//! A gallery of visual components: Card, Chip, Avatar, Badge, progress
//! indicators, Skeleton.

use rosace::prelude::*;

pub fn gallery_screen(chip_selected: &Atom<bool>, progress: &Atom<f32>) -> impl Widget {
    let chip = chip_selected.clone();
    let p = progress.clone();
    Column::new()
        .padding(EdgeInsets::all(16.0))
        .spacing(20.0)
        .child(Text::heading("Card"))
        .child(
            Card::new(
                Column::new()
                    .spacing(4.0)
                    .child(Text::title("Elevated card"))
                    .child(Text::caption("Card::new(child).elevation(8.0)")),
            )
            .elevation(8.0)
            .padding(EdgeInsets::all(16.0)),
        )
        .child(Text::heading("Chip / Avatar / Badge"))
        .child(
            Row::new()
                .spacing(12.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child({
                    let selected = chip.get();
                    let mut c = Chip::new(if selected { "Selected" } else { "Tap me" });
                    if selected { c = c.selected(); }
                    c.on_press(move || chip.set(!chip.get()))
                })
                .child(Avatar::new("TZ"))
                .child(Badge::count(7))
                .child(Badge::dot()),
        )
        .child(Text::heading("Progress"))
        .child(ProgressBar::new(progress.get()).width(240.0))
        .child(
            Row::new()
                .spacing(12.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(CircularProgress::new(progress.get()).diameter(36.0))
                .child(CircularProgress::spinner().diameter(28.0))
                .child(Button::new("+10%").width(90.0).on_press(move || {
                    let next = (p.get() + 0.1).min(1.0);
                    p.set(if next >= 0.999 { 0.0 } else { next });
                })),
        )
        .child(Text::heading("Skeleton (loading placeholder)"))
        .child(Skeleton::new().width(240.0).height(16.0))
        .scrollable()
}
