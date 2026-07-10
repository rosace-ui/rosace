//! Button variants, sizes, icon, and disabled state. Press/hover feedback
//! (D108/Phase 26 Step 1) is automatic — no wiring needed here.

use rosace::prelude::*;

pub fn buttons_screen(taps: &Atom<i32>) -> impl Widget {
    let t = taps.clone();
    Column::new()
        .padding(EdgeInsets::all(16.0))
        .spacing(16.0)
        .child(Text::caption(format!("Taps so far: {}", taps.get())))
        .child(Text::heading("Variants"))
        .child(
            Wrap::new()
                .spacing(8.0)
                .child(Button::new("Primary").on_press({ let t = t.clone(); move || t.set(t.get() + 1) }))
                .child(Button::new("Secondary").variant(ButtonVariant::Secondary).on_press({ let t = t.clone(); move || t.set(t.get() + 1) }))
                .child(Button::new("Ghost").variant(ButtonVariant::Ghost).on_press({ let t = t.clone(); move || t.set(t.get() + 1) }))
                .child(Button::new("Danger").variant(ButtonVariant::Danger).on_press({ let t = t.clone(); move || t.set(t.get() + 1) }))
                .child(Button::new("Success").variant(ButtonVariant::Success).on_press({ let t = t.clone(); move || t.set(t.get() + 1) }))
                .child(Button::new("Link").variant(ButtonVariant::Link).on_press({ let t = t.clone(); move || t.set(t.get() + 1) })),
        )
        .child(Text::heading("Sizes"))
        .child(
            Row::new()
                .spacing(8.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(Button::new("Small").width(80.0).height(28.0).font_size(11.0).on_press({ let t = t.clone(); move || t.set(t.get() + 1) }))
                .child(Button::new("Regular").on_press({ let t = t.clone(); move || t.set(t.get() + 1) }))
                .child(Button::new("Large").width(160.0).height(52.0).font_size(16.0).on_press({ let t = t.clone(); move || t.set(t.get() + 1) })),
        )
        .child(Text::heading("Icon + disabled"))
        .child(
            Row::new()
                .spacing(8.0)
                .child(Button::new("Save").icon(Icon::new(IconKind::Check)).on_press({ let t = t.clone(); move || t.set(t.get() + 1) }))
                .child(Button::new("Unavailable").disabled()),
        )
        .scrollable()
}
