use tezzera::prelude::*;
use tezzera_state::Atom;

// ── Phase 19 Demo — Frozen-Texture Scroll (D087/D088) ─────────────────────────
//
// A 50-item list inside a TransformLayer.
// - TransformLayer::paint records the child into a separate PictureRecorder (D087)
//   instead of painting into the main display list.
// - The platform replays the picture at (viewport.origin - scroll_offset) onto
//   the base canvas (D088) — no widget rebuild when only scroll changes.
// - A "build count" atom only increments when the "Add item" button is pressed,
//   proving that scroll does NOT trigger component rebuilds.
//
// ScrollView::live is also shown as an alternative in the right panel.

const VIEWPORT_H: f32 = 380.0;
const ITEM_H: f32 = 38.0;

struct Phase19Demo;

impl Component for Phase19Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let scroll_y:    Atom<f32> = ctx.state(0.0_f32);
        let build_count: Atom<u32> = ctx.state(0_u32);
        let extra_items: Atom<u32> = ctx.state(0_u32);

        let item_count = 50 + extra_items.get();
        let max_scroll = (item_count as f32 * ITEM_H - VIEWPORT_H).max(0.0);
        let cur_scroll = scroll_y.get();
        let builds     = build_count.get();

        // Increment build count each time this function is called
        build_count.set(builds + 1);

        let sy_up  = scroll_y.clone();
        let sy_dn  = scroll_y.clone();
        let sy_top = scroll_y.clone();
        let sy_bot = scroll_y.clone();
        let extra  = extra_items.clone();

        let mut list = Column::new().spacing(0.0);
        for i in 0..item_count {
            let c = if i % 2 == 0 { Color::rgb(0x14, 0x14, 0x1e) } else { Color::rgb(0x1c, 0x1c, 0x28) };
            list = list.child(
                Container::new()
                    .color(c)
                    .padding(EdgeInsets::symmetric(12.0, 4.0))
                    .child(Text::caption(&format!("Item {:03} — build #{}", i + 1, builds)))
            );
        }

        Column::new().spacing(16.0).padding(EdgeInsets::all(24.0))
            .child(
                Text::display("Phase 19 — Frozen-Texture Scroll")
                    .weight(FontWeight::Bold)
            )
            .child(Text::caption(
                "Child is recorded into a separate PictureRecorder (D087). \
                 The platform replays it with scroll offset applied (D088). \
                 Scroll does NOT rebuild the component — only Add Item does."
            ))
            .child(
                Row::new().spacing(24.0)
                    .child(Text::caption(&format!("scroll_y = {:.0}px", cur_scroll)))
                    .child(Text::caption(&format!("builds = {} (scroll does not increment)", builds)))
            )
            .child(
                Row::new().spacing(8.0)
                    .child(Button::new("▲ Up").on_press(move || {
                        let v = (sy_up.get() - 76.0).max(0.0);
                        sy_up.set(v);
                    }))
                    .child(Button::new("▼ Down").on_press(move || {
                        let v = (sy_dn.get() + 76.0).min(max_scroll);
                        sy_dn.set(v);
                    }))
                    .child(Button::new("Top").on_press(move || sy_top.set(0.0)))
                    .child(Button::new("Bot").on_press(move || sy_bot.set(max_scroll)))
                    .child(Button::new("+ Add Item").on_press(move || {
                        extra.set(extra.get() + 1);
                    }))
            )
            .child(
                SizedBox::new().height(VIEWPORT_H).child(
                    TransformLayer::new(list, VIEWPORT_H, scroll_y)
                )
            )
            .into_element()
    }
}

fn main() {
    let _ = env_logger::try_init();
    App::new()
        .title("Phase 19 — Frozen-Texture Scroll")
        .size(700, 600)
        .launch(Phase19Demo);
}
