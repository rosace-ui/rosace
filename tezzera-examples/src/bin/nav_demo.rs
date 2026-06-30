use tezzera::prelude::*;
use tezzera::nav::Navigator;
use tezzera::nav::Route;
use tezzera_state::Atom;

#[derive(Debug, Clone, PartialEq)]
enum Screen { Home, Profile, Settings }
impl Route for Screen {}

struct NavDemo {
    nav: std::sync::Arc<Navigator<Screen>>,
}

impl NavDemo {
    fn new() -> Self {
        Self { nav: std::sync::Arc::new(Navigator::new(Screen::Home)) }
    }
}

impl Component for NavDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let name_atom: Atom<String> = ctx.state("".to_string());
        let nav = self.nav.clone();
        let current = nav.current().unwrap_or(Screen::Home);

        let body: Box<dyn Widget> = match current {
            Screen::Home => {
                let nav2 = nav.clone();
                let nav3 = nav.clone();
                Box::new(
                    Column::new()
                        .child(Text::display("Home"))
                        .child(Text::label("Welcome to the nav demo."))
                        .child(
                            Button::new("Go to Profile")
                                .on_press(move || { nav2.push(Screen::Profile); })
                        )
                        .child(
                            Button::new("Go to Settings")
                                .on_press(move || { nav3.push(Screen::Settings); })
                        )
                )
            }
            Screen::Profile => {
                let nav2 = nav.clone();
                let name = name_atom.get();
                Box::new(
                    Column::new()
                        .child(Text::display("Profile"))
                        .child(Text::label(if name.is_empty() { "Name: (empty)" } else { "Name set" }))
                        .child(TextInput::new().placeholder("Enter your name"))
                        .child(
                            Button::new("Back")
                                .on_press(move || { nav2.pop(); })
                        )
                )
            }
            Screen::Settings => {
                let nav2 = nav.clone();
                Box::new(
                    Column::new()
                        .child(Text::display("Settings"))
                        .child(Text::label("Adjust your preferences here."))
                        .child(
                            Row::new()
                                .child(Text::label("Dark mode"))
                                .child(Switch::new(true))
                        )
                        .child(
                            Button::new("Back")
                                .on_press(move || { nav2.pop(); })
                        )
                )
            }
        };

        Scaffold::new(body)
            .app_bar(AppBar::new("Nav Demo"))
            .into_element()
    }
}

fn main() {
    App::run(NavDemo::new());
}
