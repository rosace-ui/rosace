use rosace::prelude::*;

struct Counter;

impl Component for Counter {
    fn build(&self, ctx: &mut Context) -> Element {
        let count = ctx.state(0i32);

        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 60.0))
                .child(Text::display(count.get().to_string()).align(TextAlign::Center))
                .child(Spacer::gap(0.0, 8.0))
                .child(Text::new("click to increment").align(TextAlign::Center))
                .child(Spacer::gap(0.0, 32.0))
                .child(
                    Row::new()
                        .main_axis_alignment(MainAxisAlignment::Center)
                        .spacing(12.0)
                        .child(Button::new("−").variant(ButtonVariant::Ghost).width(44.0)
                            .on_press({
                                let count = count.clone();
                                move || count.set(count.get() - 1)
                            }))
                        .child(Button::new("Increment").width(140.0)
                            .on_press({
                                let count = count.clone();
                                move || count.set(count.get() + 1)
                            }))
                        .child(Button::new("+").variant(ButtonVariant::Ghost).width(44.0)
                            .on_press({
                                let count = count.clone();
                                move || count.set(count.get() + 1)
                            })),
                ),
        )
        .app_bar(AppBar::new("Counter"))
        .into_element()
    }
}

fn main() {
    App::new()
        .title("Counter")
        .size(400, 300)
        .launch(Counter);
}
