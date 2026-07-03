use tezzera::prelude::*;
use tezzera_state::Atom;

// ── Phase 18 Demo — Live ScrollView (D084) ────────────────────────────────────
//
// Two side-by-side ScrollViews driven by independent Atom<f32> scroll offsets.
// A third static panel shows controls. Each panel is a fully independent scroll
// region; scrolling one never affects the other.
//
// ScrollView::live(child, atom) is the D084 reactive constructor. The atom is
// owned by the component; when its value changes the component rebuilds and
// ScrollView::paint reads the new offset.

const ITEMS: i32 = 30;
const ITEM_H: f32 = 36.0;
const VIEWPORT_H: f32 = 320.0;

struct Phase18Demo;

impl Component for Phase18Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let left_scroll:  Atom<f32> = ctx.state(0.0_f32);
        let right_scroll: Atom<f32> = ctx.state(0.0_f32);
        let max_scroll = (ITEMS as f32 * ITEM_H - VIEWPORT_H).max(0.0);

        // Clone atoms for button closures
        let ll = left_scroll.clone();
        let ld = left_scroll.clone();
        let lt = left_scroll.clone();
        let lb = left_scroll.clone();
        let rl = right_scroll.clone();
        let rd = right_scroll.clone();
        let rt = right_scroll.clone();
        let rb = right_scroll.clone();

        let left_val  = left_scroll.get();
        let right_val = right_scroll.get();

        // ── left list ─────────────────────────────────────────────────────────
        let mut left_list = Column::new().spacing(0.0);
        for i in 0..ITEMS {
            let c = if i % 2 == 0 { Color::rgb(0x12, 0x12, 0x18) } else { Color::rgb(0x1a, 0x1a, 0x22) };
            left_list = left_list.child(
                Container::new()
                    .background(c)
                    .padding(EdgeInsets::all(8.0))
                    .child(Text::caption(&format!("Left item {:02} — y={:.0}", i + 1, left_val)))
            );
        }

        // ── right list ────────────────────────────────────────────────────────
        let mut right_list = Column::new().spacing(0.0);
        for i in 0..ITEMS {
            let c = if i % 2 == 0 { Color::rgb(0x18, 0x12, 0x12) } else { Color::rgb(0x22, 0x1a, 0x1a) };
            right_list = right_list.child(
                Container::new()
                    .background(c)
                    .padding(EdgeInsets::all(8.0))
                    .child(Text::caption(&format!("Right item {:02} — y={:.0}", i + 1, right_val)))
            );
        }

        Column::new().spacing(20.0).padding(EdgeInsets::all(20.0))
            .child(
                Text::display("Phase 18 — Live ScrollView × Atom<f32> (D084)")
                    .weight(FontWeight::Bold)
            )
            .child(Text::caption(
                "Two independent scroll regions driven by Atom<f32>. \
                 ScrollView::live(child, atom) reads the atom at paint time. \
                 Each region scrolls independently with no coupling."
            ))
            // ── panels row ──────────────────────────────────────────────────
            .child(
                Row::new().spacing(16.0)
                    // Left panel
                    .child(
                        Expanded::new(
                            Column::new().spacing(8.0)
                                .child(Text::caption(&format!("Left  scroll_y = {:.0}px", left_val)))
                                .child(
                                    Row::new().spacing(6.0)
                                        .child(Button::new("▲").on_press(move || {
                                            let v = (ll.get() - 72.0).max(0.0);
                                            ll.set(v);
                                        }))
                                        .child(Button::new("▼").on_press(move || {
                                            let v = (ld.get() + 72.0).min(max_scroll);
                                            ld.set(v);
                                        }))
                                        .child(Button::new("Top").on_press(move || lt.set(0.0)))
                                        .child(Button::new("Bot").on_press(move || lb.set(max_scroll)))
                                )
                                .child(
                                    Container::new().height(VIEWPORT_H).child(
                                        ScrollView::live(left_list, left_scroll)
                                    )
                                )
                        )
                    )
                    // Right panel
                    .child(
                        Expanded::new(
                            Column::new().spacing(8.0)
                                .child(Text::caption(&format!("Right scroll_y = {:.0}px", right_val)))
                                .child(
                                    Row::new().spacing(6.0)
                                        .child(Button::new("▲").on_press(move || {
                                            let v = (rl.get() - 72.0).max(0.0);
                                            rl.set(v);
                                        }))
                                        .child(Button::new("▼").on_press(move || {
                                            let v = (rd.get() + 72.0).min(max_scroll);
                                            rd.set(v);
                                        }))
                                        .child(Button::new("Top").on_press(move || rt.set(0.0)))
                                        .child(Button::new("Bot").on_press(move || rb.set(max_scroll)))
                                )
                                .child(
                                    Container::new().height(VIEWPORT_H).child(
                                        ScrollView::live(right_list, right_scroll)
                                    )
                                )
                        )
                    )
            )
            .into_element()
    }
}

fn main() {
    let _ = env_logger::try_init();
    App::new()
        .title("Phase 18 — Live ScrollView")
        .size(800, 600)
        .launch(Phase18Demo);
}
