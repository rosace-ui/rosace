//! Dialog, Sheet, Toast, Tooltip, Menu — ROSACE's overlay system
//! (`OverlayApi`, D058).

use rosace::prelude::*;

#[allow(clippy::too_many_arguments)]
pub fn overlays_screen(
    dialog_open: &Atom<bool>,
    sheet_open: &Atom<bool>,
    toast_open: &Atom<bool>,
    menu_open: &Atom<bool>,
) -> impl Widget {
    let dialog_open = dialog_open.clone();
    let sheet_open = sheet_open.clone();
    let toast_open = toast_open.clone();
    let menu_open = menu_open.clone();

    Column::new()
        .padding(EdgeInsets::all(16.0))
        .spacing(16.0)
        .child(Text::heading("Dialog"))
        .child(
            Button::new("Open dialog")
                .on_press({ let o = dialog_open.clone(); move || o.set(true) })
                .dialog(dialog_open.clone(), {
                    let dialog_open = dialog_open.clone();
                    move || {
                        let o1 = dialog_open.clone();
                        let o2 = dialog_open.clone();
                        Box::new(
                            Dialog::new("Delete item?")
                                .message("This can't be undone.")
                                .action("Cancel", move || o1.set(false))
                                .destructive_action("Delete", move || o2.set(false)),
                        )
                    }
                }),
        )
        .child(Text::heading("Sheet"))
        .child(
            Button::new("Open sheet")
                .on_press({ let o = sheet_open.clone(); move || o.set(true) })
                .sheet(sheet_open.clone(), {
                    let sheet_open = sheet_open.clone();
                    move || {
                        let o = sheet_open.clone();
                        Box::new(
                            Column::new()
                                .padding(EdgeInsets::all(20.0))
                                .spacing(12.0)
                                .child(Text::title("Bottom sheet"))
                                .child(Text::new("Slides up from the bottom edge."))
                                .child(Button::new("Close").on_press(move || o.set(false))),
                        )
                    }
                }),
        )
        .child(Text::heading("Toast"))
        .child(
            Button::new("Show toast")
                .on_press({ let o = toast_open.clone(); move || Toast::show(&o, 2.5) })
                .toast(toast_open.clone(), || Box::new(Toast::success("Saved!"))),
        )
        .child(Text::heading("Tooltip"))
        .child(Tooltip::new("I'm a tooltip — hover me", Button::new("Hover me")))
        .child(Text::heading("Menu"))
        .child(
            Button::new("File")
                .on_press({ let o = menu_open.clone(); move || o.set(true) })
                .dropdown(menu_open.clone(), {
                    let menu_open = menu_open.clone();
                    move || {
                        let o1 = menu_open.clone();
                        let o2 = menu_open.clone();
                        Box::new(
                            Menu::new()
                                .item("New", move || o1.set(false))
                                .item("Open...", move || o2.set(false)),
                        )
                    }
                }),
        )
        .scrollable()
}
