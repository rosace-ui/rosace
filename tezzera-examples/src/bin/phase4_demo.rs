use tezzera::prelude::*;
use tezzera_state::Atom;

#[derive(Debug, Clone, PartialEq)]
enum Panel { WidgetGallery, Images, Forms, Navigation }

struct Phase4Demo;

impl Component for Phase4Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let panel: Atom<Panel> = ctx.state(Panel::WidgetGallery);

        let p1 = panel.clone(); let p2 = panel.clone();
        let p3 = panel.clone(); let p4 = panel.clone();
        let nav_row = Row::new()
            .child(Button::new("Widgets").on_press(move || p1.set(Panel::WidgetGallery)))
            .child(Button::new("Images").on_press(move || p2.set(Panel::Images)))
            .child(Button::new("Forms").on_press(move || p3.set(Panel::Forms)))
            .child(Button::new("Nav").on_press(move || p4.set(Panel::Navigation)));

        let body: Box<dyn Widget> = match panel.get() {
            Panel::WidgetGallery => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Widget Gallery"))
                        .child(
                            Row::new()
                                .child(Button::new("Primary").variant(ButtonVariant::Primary))
                                .child(Button::new("Secondary").variant(ButtonVariant::Secondary))
                                .child(Button::new("Ghost").variant(ButtonVariant::Ghost))
                        )
                        .child(
                            Row::new()
                                .child(Chip::new("Rust"))
                                .child(Chip::new("UI"))
                                .child(Chip::new("Native"))
                        )
                        .child(ProgressBar::new(0.65))
                        .child(Slider::new(0.4))
                        .child(
                            Row::new()
                                .child(Checkbox::new(true))
                                .child(Text::label("Checkbox"))
                                .child(Switch::new(false))
                                .child(Text::label("Switch"))
                        )
                )
            }
            Panel::Images => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Images"))
                        .child(Text::label("Placeholder (no file loaded):"))
                        .child(
                            Image::placeholder(Color::rgb(80, 100, 180))
                                .width(320.0).height(180.0)
                        )
                        .child(Text::label("Tooltip (always visible):"))
                        .child(
                            Tooltip::new("This is a tooltip", Spacer::gap(120.0, 40.0))
                                .visible(true).font_size(12.0)
                        )
                )
            }
            Panel::Forms => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Forms"))
                        .child(TextInput::new().placeholder("Username"))
                        .child(TextInput::new().placeholder("Password"))
                        .child(
                            Row::new()
                                .child(Button::new("Sign In"))
                                .child(Button::new("Cancel").variant(ButtonVariant::Ghost))
                        )
                )
            }
            Panel::Navigation => {
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Navigation"))
                        .child(Text::label("Click the buttons above to switch panels."))
                        .child(Card::new(Text::label("Panel switching via Atom<Panel>.")))
                )
            }
        };

        Scaffold::new(body)
            .app_bar(AppBar::new("Phase 4 Demo — Image · Tooltip · Nav"))
            .into_element()
    }
}

fn main() {
    App::run(Phase4Demo);
}
