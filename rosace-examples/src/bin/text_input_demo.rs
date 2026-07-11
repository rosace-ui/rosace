//! D112/Phase 28 Step 1 verification: real keyboard editing.
//!
//! Two real `TextInput`s bound to `ctx.state(String)` atoms via
//! `.value()`/`.on_change()` — click to focus, type, arrow-navigate,
//! Shift+arrow select, Home/End, Cmd+A select-all, Cmd+C/X/V clipboard,
//! Tab between fields. A live label below echoes the first field's atom
//! value on every keystroke, proving `on_change` really reaches app state
//! (not just that the widget paints something).

use rosace::prelude::*;

struct TextInputDemo;

impl Component for TextInputDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let name: Atom<String> = ctx.state(String::new());
        let email: Atom<String> = ctx.state(String::from("prefilled@example.com"));

        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 20.0))
                .child(Text::new("Click a field, type, Tab to the next one.").align(TextAlign::Center))
                .child(Spacer::gap(0.0, 16.0))
                .child(
                    TextInput::new()
                        .placeholder("Name")
                        .value(name.get())
                        .width(320.0)
                        .on_change({
                            let name = name.clone();
                            move |v| name.set(v)
                        }),
                )
                .child(Spacer::gap(0.0, 10.0))
                .child(
                    TextInput::new()
                        .placeholder("Email")
                        .value(email.get())
                        .width(320.0)
                        .on_change({
                            let email = email.clone();
                            move |v| email.set(v)
                        }),
                )
                .child(Spacer::gap(0.0, 20.0))
                .child(Text::new(format!("name atom = {:?}", name.get())).align(TextAlign::Center))
                .child(Text::new(format!("email atom = {:?}", email.get())).align(TextAlign::Center)),
        )
        .app_bar(AppBar::new("text_input_demo"))
        .into_element()
    }
}

fn main() {
    env_logger::init();
    App::new().title("text_input_demo").size(480, 360).launch(TextInputDemo);
}
