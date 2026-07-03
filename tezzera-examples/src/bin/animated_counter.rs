use tezzera::prelude::*;
use tezzera::animate::use_spring;

struct AnimatedCounter;

impl Component for AnimatedCounter {
    fn build(&self, ctx: &mut Context) -> Element {
        let count = ctx.state(0i32);

        // Spring-animated display value — chases the integer count each frame.
        let (display, ctrl) = use_spring(ctx, count.get() as f32);
        ctrl.animate_to(count.get() as f32);

        let displayed = format!("{:.0}", display.get());

        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 80.0))
                .child(
                    Text::display(&displayed)
                        .align(TextAlign::Center),
                )
                .child(Spacer::gap(0.0, 8.0))
                .child(
                    Text::new("spring-animated counter")
                        .align(TextAlign::Center),
                )
                .child(Spacer::gap(0.0, 40.0))
                .child(
                    Row::new()
                        .main_axis_alignment(MainAxisAlignment::Center)
                        .spacing(16.0)
                        .child(
                            Button::new("−")
                                .variant(ButtonVariant::Ghost)
                                .width(52.0)
                                .on_press({
                                    let count = count.clone();
                                    move || count.set(count.get() - 1)
                                }),
                        )
                        .child(
                            Button::new("Reset")
                                .variant(ButtonVariant::Secondary)
                                .width(100.0)
                                .on_press({
                                    let count = count.clone();
                                    let ctrl = ctrl.clone();
                                    move || {
                                        count.set(0);
                                        ctrl.snap_to(0.0);
                                    }
                                }),
                        )
                        .child(
                            Button::new("+")
                                .variant(ButtonVariant::Primary)
                                .width(52.0)
                                .on_press({
                                    let count = count.clone();
                                    move || count.set(count.get() + 1)
                                }),
                        ),
                ),
        )
        .app_bar(AppBar::new("Animated Counter"))
        .into_element()
    }
}

fn main() {
    App::new()
        .title("Animated Counter — TEZZERA Phase 2")
        .size(420, 340)
        .dark()
        .launch(AnimatedCounter);
}
