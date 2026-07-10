//! Form-style controls: Switch, Checkbox, Radio, Slider, SegmentedControl,
//! Dropdown. `TextInput` is shown too, but it's display-only today — full
//! keyboard wiring is the project's next priority after this phase.

use rosace::prelude::*;

#[allow(clippy::too_many_arguments)]
pub fn inputs_screen(
    switch_on: &Atom<bool>,
    checkbox_checked: &Atom<bool>,
    radio_selected: &Atom<bool>,
    slider_value: &Atom<f32>,
    segmented_selected: &Atom<usize>,
    dropdown_open: &Atom<bool>,
    dropdown_selected: &Atom<usize>,
) -> impl Widget {
    let sw = switch_on.clone();
    let cb = checkbox_checked.clone();
    let rd = radio_selected.clone();
    let sl = slider_value.clone();
    let seg = segmented_selected.clone();
    let dd_open = dropdown_open.clone();
    let dd_sel = dropdown_selected.clone();

    Column::new()
        .padding(EdgeInsets::all(16.0))
        .spacing(20.0)
        .child(Text::heading("Switch / Checkbox / Radio"))
        .child(
            Row::new()
                .spacing(24.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(Switch::new(switch_on.get()).on_change(move |v| sw.set(v)))
                .child(
                    Checkbox::new(checkbox_checked.get())
                        .label("Subscribe")
                        .on_change(move |v| cb.set(v)),
                )
                .child(Radio::new(radio_selected.get()).on_select(move || rd.set(true))),
        )
        .child(Text::heading("Slider"))
        .child(Text::caption(format!("Value: {:.0}", slider_value.get())))
        .child(
            Slider::new(slider_value.get())
                .range(0.0, 100.0, slider_value.get())
                .width(240.0)
                .on_change(move |v| sl.set(v)),
        )
        .child(Text::heading("SegmentedControl"))
        .child(
            SegmentedControl::new(vec!["Day", "Week", "Month"], segmented_selected.get())
                .on_change(move |i| seg.set(i)),
        )
        .child(Text::heading("Dropdown"))
        .child(
            Dropdown::new(vec!["Newest", "Oldest", "A-Z"], dropdown_selected.get(), dd_open)
                .on_change(move |i| dd_sel.set(i)),
        )
        .child(Text::heading("TextInput"))
        .child(TextInput::new().placeholder("Type here... (display-only for now)"))
        .scrollable()
}
