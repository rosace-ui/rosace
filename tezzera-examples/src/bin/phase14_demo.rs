use tezzera::prelude::*;
use tezzera_state::Atom;

// ── Phase 14 Demo — Focus + Navigation + RepaintBoundary ─────────────────────
//
// Three panels (select via top nav):
// 1. FOCUS:   Tab through 4 buttons; focused one shows a 2px blue ring
// 2. NAVIGATOR: push/pop between two screens using ScreenNav
// 3. REPAINT BOUNDARY: static content wrapped in RepaintBoundary;
//    updating a counter beside it proves the boundary is cached separately

#[derive(Debug, Clone, PartialEq)]
enum Panel { Focus, Navigator, Repaint }

#[derive(Clone, PartialEq)]
enum AppScreen { Home, Detail }

struct Phase14Demo;

impl Component for Phase14Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let tab: Atom<Panel> = ctx.state(Panel::Focus);
        let count: Atom<i32> = ctx.state(0_i32);
        let nav = ScreenNav::new(ctx, AppScreen::Home);

        let t0 = tab.clone(); let t1 = tab.clone(); let t2 = tab.clone();
        let nav_row = Row::new().spacing(8.0)
            .child(Button::new("Focus").on_press(move || t0.set(Panel::Focus)))
            .child(Button::new("Navigator").on_press(move || t1.set(Panel::Navigator)))
            .child(Button::new("RepaintBoundary").on_press(move || t2.set(Panel::Repaint)));

        let body: Box<dyn Widget> = match tab.get() {
            Panel::Focus => {
                let f1 = FocusNode::new();
                let f2 = FocusNode::new();
                let f3 = FocusNode::new();
                let f4 = FocusNode::new();
                Box::new(
                    Column::new().spacing(16.0)
                        .child(Text::heading("Focus Navigation (Tab / Shift+Tab)"))
                        .child(Text::caption("Press Tab to cycle through the four buttons."))
                        .child(Text::caption("Focused button shows a 2px accent-blue focus ring."))
                        .child(Text::caption("Shift+Tab moves focus backwards. FocusNode IDs are per-frame stable."))
                        .child(
                            Row::new().spacing(12.0)
                                .child(Button::new("Button A").focus_node(f1))
                                .child(Button::new("Button B").focus_node(f2))
                                .child(Button::new("Button C").focus_node(f3))
                                .child(Button::new("Button D").focus_node(f4))
                        )
                )
            }
            Panel::Navigator => {
                match nav.current().unwrap_or(AppScreen::Home) {
                    AppScreen::Home => {
                        let nav2 = nav.clone();
                        Box::new(
                            Column::new().spacing(16.0)
                                .child(Text::heading("Screen: Home"))
                                .child(Text::caption("ScreenNav stores the route stack in a ctx.state() atom."))
                                .child(Text::caption("push() adds a screen; the component rebuilds reactively."))
                                .child(Text::caption("pop() removes the top screen and restores the one below."))
                                .child(
                                    Button::new("Go to Detail Screen →")
                                        .on_press(move || nav2.push(AppScreen::Detail))
                                )
                        )
                    }
                    AppScreen::Detail => {
                        let nav2 = nav.clone();
                        Box::new(
                            Column::new().spacing(16.0)
                                .child(Text::heading("Screen: Detail"))
                                .child(Text::caption("This is the Detail screen. Click Back to pop it."))
                                .child(Text::caption("Home screen atom state is preserved — not cleared."))
                                .child(Text::caption("Route stack depth = 2."))
                                .child(
                                    Button::new("← Back to Home")
                                        .on_press(move || { nav2.pop(); })
                                )
                        )
                    }
                }
            }
            Panel::Repaint => {
                let cnt = count.get();
                let inc = count.clone();

                let static_content = Column::new().spacing(8.0)
                    .child(Text::heading("RepaintBoundary (cached)"))
                    .child(Text::caption("This column is inside RepaintBoundary."))
                    .child(Text::caption("Its Picture is cached after frame 1."))
                    .child(Text::caption("Clicking Increment does NOT repaint this."))
                    .child(Text::caption("Zero widget.paint() calls for this subtree."));

                Box::new(
                    Column::new().spacing(20.0)
                        .child(Text::heading("RepaintBoundary Demo"))
                        .child(
                            Row::new().spacing(24.0)
                                .child(
                                    Column::new().spacing(12.0)
                                        .child(Text::heading("Counter (repaints every click)"))
                                        .child(Text::display(&cnt.to_string()).weight(FontWeight::Bold))
                                        .child(Button::new("Increment").on_press(move || inc.set(count.get() + 1)))
                                        .child(Text::caption("This column repaints on each Increment click."))
                                )
                                .child(RepaintBoundary::new(static_content))
                        )
                )
            }
        };

        Column::new().spacing(16.0).padding(EdgeInsets::all(24.0))
            .child(
                Text::display("Phase 14 — Focus + Navigation + RepaintBoundary")
                    .weight(FontWeight::Bold)
            )
            .child(nav_row)
            .child(body)
            .into_element()
    }
}

fn main() {
    App::new()
        .title("Phase 14 — Focus + Navigation + RepaintBoundary")
        .size(1000, 640)
        .launch(Phase14Demo);
}
