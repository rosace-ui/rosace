use tezzera::prelude::*;
use tezzera_state::Atom;

#[derive(Debug, Clone, PartialEq)]
enum Panel { Images, Overlays, Transitions, Templates }

struct Phase5Demo;

impl Component for Phase5Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let panel: Atom<Panel>     = ctx.state(Panel::Images);
        let show_modal: Atom<bool> = ctx.state(false);
        let show_dialog: Atom<bool> = ctx.state(false);

        let p1 = panel.clone(); let p2 = panel.clone();
        let p3 = panel.clone(); let p4 = panel.clone();
        let nav_row = Row::new()
            .child(Button::new("Images").on_press(move || p1.set(Panel::Images)))
            .child(Button::new("Overlays").on_press(move || p2.set(Panel::Overlays)))
            .child(Button::new("Transitions").on_press(move || p3.set(Panel::Transitions)))
            .child(Button::new("Templates").on_press(move || p4.set(Panel::Templates)));

        let body: Box<dyn Widget> = match panel.get() {
            Panel::Images => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Image Rendering"))
                        .child(Text::label("Placeholder — three fit modes:"))
                        .child(
                            Row::new()
                                .child(
                                    Column::new()
                                        .child(Text::caption("Contain"))
                                        .child(
                                            Image::placeholder(Color::rgb(60, 100, 200))
                                                .width(180.0).height(120.0)
                                                .fit(ImageFit::Contain)
                                        )
                                )
                                .child(
                                    Column::new()
                                        .child(Text::caption("Cover"))
                                        .child(
                                            Image::placeholder(Color::rgb(180, 60, 80))
                                                .width(180.0).height(120.0)
                                                .fit(ImageFit::Cover)
                                        )
                                )
                                .child(
                                    Column::new()
                                        .child(Text::caption("Fill"))
                                        .child(
                                            Image::placeholder(Color::rgb(60, 160, 80))
                                                .width(180.0).height(120.0)
                                                .fit(ImageFit::Fill)
                                        )
                                )
                        )
                        .child(Text::label("Tooltip (always visible):"))
                        .child(
                            Tooltip::new(
                                "ImageCache prevents re-decoding each frame",
                                Image::placeholder(Color::rgb(100, 80, 160))
                                    .width(200.0).height(100.0),
                            )
                            .visible(true)
                        )
                )
            }

            Panel::Overlays => {
                let sm = show_modal.clone();
                let sd = show_dialog.clone();
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Overlays"))
                        .child(
                            Row::new()
                                .child(Button::new("Show Modal").on_press(move || sm.set(true)))
                                .child(Button::new("Show Dialog").on_press(move || sd.set(true)))
                        )
                        .child(Text::label("Toast notifications appear at bottom-center."))
                        .child(Card::new(
                            Column::new()
                                .child(Text::label("Toast lifetime: 3.0s"))
                                .child(Text::caption("ToastQueue auto-dismisses expired toasts"))
                        ))
                        .child(Text::caption(
                            if show_modal.get() { "Modal: VISIBLE" } else { "Modal: hidden" }
                        ))
                        .child(Text::caption(
                            if show_dialog.get() { "Dialog: VISIBLE" } else { "Dialog: hidden" }
                        ))
                )
            }

            Panel::Transitions => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Screen Transitions"))
                        .child(Text::label("ScreenTransition — spring-based offsets:"))
                        .child(
                            Row::new()
                                .child(transition_card("Slide →", "SlideDirection::Right"))
                                .child(transition_card("Slide ←", "SlideDirection::Left"))
                                .child(transition_card("Slide ↑", "SlideDirection::Up"))
                        )
                        .child(
                            Row::new()
                                .child(transition_card("Slide ↓", "SlideDirection::Down"))
                                .child(transition_card("Fade", "progress_spring 0→1"))
                                .child(transition_card("Scale", "scale 0.85→1.0"))
                        )
                        .child(Text::caption("NavigatorAnimated<R> wraps Navigator<R> + ScreenTransition"))
                )
            }

            Panel::Templates => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Project Templates"))
                        .child(Text::label("`tzr new my_app --template <name>`"))
                        .child(
                            Row::new()
                                .child(template_card("counter", "Default. Button + state."))
                                .child(template_card("nav-app", "3-screen Navigator app."))
                        )
                        .child(
                            Row::new()
                                .child(template_card("form-app", "Login form + validation."))
                                .child(template_card("dashboard", "Stats cards + charts."))
                        )
                )
            }
        };

        Scaffold::new(body)
            .app_bar(AppBar::new("Phase 5 Demo — Images · Overlays · Transitions · Templates"))
            .into_element()
    }
}

fn transition_card(name: &str, detail: &str) -> impl Widget {
    Card::new(
        Column::new()
            .child(Text::label(name))
            .child(Text::caption(detail))
    )
}

fn template_card(name: &str, desc: &str) -> impl Widget {
    Card::new(
        Column::new()
            .child(Text::heading(name))
            .child(Text::caption(desc))
    )
}

fn main() {
    App::run(Phase5Demo);
}
