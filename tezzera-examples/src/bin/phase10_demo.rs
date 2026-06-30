use tezzera::a11y::{A11yNode, A11yTree, FocusManager, Role};
use tezzera::anim::{AnimationController, Easing, Keyframe, Timeline, Tween};
use tezzera::prelude::*;
use tezzera_state::Atom;

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum Panel {
    Animation,
    Accessibility,
    TestHarness,
    Package,
}

struct Phase10Demo;

impl Component for Phase10Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let panel: Atom<Panel> = ctx.state(Panel::Animation);
        let tween_t: Atom<f32> = ctx.state(0.0f32);

        let p1 = panel.clone();
        let p2 = panel.clone();
        let p4 = panel.clone();
        let nav_row = Row::new()
            .child(Button::new("Animation").on_press(move || p1.set(Panel::Animation)))
            .child(Button::new("A11y").on_press(move || p2.set(Panel::Accessibility)))
            .child(Button::new("Package").on_press(move || p4.set(Panel::Package)));

        let body: Box<dyn Widget> = match panel.get() {
            Panel::Animation => {
                let t = tween_t.get().min(1.0);
                let tween = Tween::new(0.0_f32, 100.0, 1.0, Easing::EaseInOut);
                let mut ctrl = AnimationController::new(tween);
                let value = ctrl.tick(t);

                let timeline = Timeline::new()
                    .with_keyframe(Keyframe::new(0.0, 0.0_f32, Easing::Linear))
                    .with_keyframe(Keyframe::new(0.5, 60.0, Easing::EaseIn))
                    .with_keyframe(Keyframe::new(1.0, 100.0, Easing::EaseOut));
                let tl_value = timeline.sample(t);

                let tt = tween_t.clone();
                let tt2 = tween_t.clone();

                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Animation System"))
                        .child(Text::label(format!(
                            "t = {:.2}  Tween EaseInOut → {:.1}",
                            t, value
                        )))
                        .child(ProgressBar::new(value / 100.0))
                        .child(Text::label(format!(
                            "Timeline sample at t → {:.1}",
                            tl_value
                        )))
                        .child(ProgressBar::new(tl_value / 100.0))
                        .child(
                            Row::new()
                                .child(
                                    Button::new("t += 0.1")
                                        .on_press(move || tt.set((tt.get() + 0.1).min(1.0))),
                                )
                                .child(Button::new("Reset").on_press(move || tt2.set(0.0))),
                        )
                        .child(Text::caption(
                            "Easing: Linear EaseIn EaseOut EaseInOut CubicBezier Spring",
                        ))
                        .child(Text::caption("Tween<T: Lerp> — impl for f32, f64, Color")),
                )
            }

            Panel::Accessibility => {
                let mut tree = A11yTree::new(0);
                tree.add_node(A11yNode::new(0, Role::Dialog).with_label("Settings"));
                tree.add_child(0, A11yNode::new(1, Role::Button).with_label("Save"));
                tree.add_child(0, A11yNode::new(2, Role::Button).with_label("Cancel"));
                tree.add_child(
                    0,
                    A11yNode::new(3, Role::Checkbox)
                        .with_label("Dark mode")
                        .with_checked(true),
                );
                tree.add_child(
                    0,
                    A11yNode::new(4, Role::TextInput).with_label("Name field"),
                );

                let buttons = tree.find_by_role(Role::Button);
                let mut focus = FocusManager::new();
                focus.sync(&tree);

                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Accessibility Tree"))
                        .child(Text::label(format!(
                            "A11yTree: {} nodes  {} buttons  {} focusable",
                            tree.node_count(),
                            buttons.len(),
                            tree.focusable_nodes().len()
                        )))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("Dialog: Settings"))
                                .child(Text::caption("└ Button: Save"))
                                .child(Text::caption("└ Button: Cancel"))
                                .child(Text::caption("└ Checkbox: Dark mode [checked]"))
                                .child(Text::caption("└ TextInput: Name field")),
                        ))
                        .child(Text::label(format!(
                            "FocusManager tab_order: {} items",
                            focus.tab_order().len()
                        )))
                        .child(Text::caption(
                            "focus_next() / focus_prev() cycle focusable nodes",
                        )),
                )
            }

            Panel::TestHarness => {
                let events = tezzera::test_utils::EventSim::tap(100.0, 200.0);
                Box::new(
                    Column::new()
                        .child(nav_row)
                        .child(Text::display("Test Harness"))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("WidgetEnv"))
                                .child(Text::caption("new(width, height) → headless SkiaCanvas"))
                                .child(Text::caption("render_text / encode_png / pixel_at(x, y)"))
                                .child(Text::caption("Used in golden-file widget tests")),
                        ))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("EventSim"))
                                .child(Text::label(format!(
                                    "EventSim::tap(100, 200) → {} events",
                                    events.len()
                                )))
                                .child(Text::caption("type_text(s) → KeyDown + KeyUp per char"))
                                .child(Text::caption("scroll(x, y, delta) → Scroll event")),
                        ))
                        .child(Card::new(
                            Column::new()
                                .child(Text::heading("SnapshotAssert"))
                                .child(Text::caption("save_snapshot(name, png) → test_snapshots/"))
                                .child(Text::caption("assert_snapshot → pixel-by-pixel diff"))
                                .child(Text::caption("pixel_diff_count(a, b) → usize")),
                        )),
                )
            }

            Panel::Package => Box::new(
                Column::new()
                    .child(nav_row)
                    .child(Text::display("Package CLI"))
                    .child(Text::label("`tzr package` flow:"))
                    .child(Card::new(
                        Column::new()
                            .child(Text::caption("1. cargo build --release --workspace"))
                            .child(Text::caption("2. Collect binary paths"))
                            .child(Text::caption("3. Write manifest.json to output dir"))
                            .child(Text::caption("4. Report: crates, examples, built_at")),
                    ))
                    .child(Card::new(
                        Column::new()
                            .child(Text::heading("PackageManifest JSON"))
                            .child(Text::caption(
                                "{ \"crates\": [...], \"examples\": [...], \"built_at\": \"...\" }",
                            )),
                    ))
                    .child(Text::caption("tzr package --out dist/")),
            ),
        };

        Scaffold::new(body)
            .app_bar(AppBar::new(
                "Phase 10 — Animation · A11y · Test Harness · Package",
            ))
            .into_element()
    }
}

fn main() {
    App::run(Phase10Demo);
}
