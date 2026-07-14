//! D116/Phase 28 Step 4 verification: `TextArea` — wrapping, Enter for a
//! real newline, Up/Down goal-column movement across wrapped lines, and
//! mouse-wheel scrolling. Click the field, type a long paragraph and
//! watch it wrap, press Enter for new lines, use arrow keys to move
//! around (including vertically across wrapped lines).

use rosace::prelude::*;

struct TextAreaDemo;

impl Component for TextAreaDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let body: Atom<String> = ctx.state(String::from(
            "Type a long paragraph here and watch it wrap.\n\nPress Enter for a new line, and try Up/Down to move across wrapped lines."
        ));

        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 20.0))
                .child(Text::new("Click the field, type, Enter for a new line, Up/Down to navigate.").align(TextAlign::Center))
                .child(Spacer::gap(0.0, 16.0))
                .child(
                    TextArea::new()
                        .placeholder("Write something...")
                        .value(body.get())
                        .width(360.0)
                        .height(180.0)
                        .on_change({
                            let body = body.clone();
                            move |v| body.set(v)
                        }),
                ),
        )
        .app_bar(AppBar::new("text_area_demo"))
        .into_element()
    }
}

fn main() {
    env_logger::init();
    App::new().title("text_area_demo").size(480, 420).launch(TextAreaDemo);
}
