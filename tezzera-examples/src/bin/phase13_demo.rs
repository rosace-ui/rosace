use tezzera::prelude::*;
use tezzera_state::Atom;

// ── Phase 13 Demo — Persistent RenderNode Tree ────────────────────────────────
//
// Shows Phase 13's dirty-flag rendering in three panels:
//
// • COUNTER: Click to increment. Only this panel repaints when count changes.
// • STATIC A / STATIC B: Never-changing text panels — cached after frame 1.
//
// The key invariant: after the first frame, clicking the button only causes
// the counter panel's native widgets to repaint. The static panels' Pictures
// are replayed from cache at zero widget-work cost.

struct Phase13Demo;

impl Component for Phase13Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let count: Atom<i32> = ctx.state(0_i32);
        let cnt = count.get();

        let inc = count.clone();
        let rst = count.clone();

        let counter_panel = Card::new(
            Column::new().spacing(10.0)
                .child(Text::heading("Counter Panel"))
                .child(Text::caption("This panel repaints on every increment."))
                .child(Text::display(&cnt.to_string()))
                .child(
                    Row::new().spacing(8.0)
                        .child(Button::new("Increment").on_press(move || inc.set(count.get() + 1)))
                        .child(Button::new("Reset").on_press(move || rst.set(0)))
                )
        );

        let static_a = Card::new(
            Column::new().spacing(10.0)
                .child(Text::heading("Static Panel A"))
                .child(Text::caption("This content never changes."))
                .child(Text::caption("After frame 1 its Picture is cached."))
                .child(Text::caption("Phase 13: zero widget work per frame."))
                .child(Text::caption("The reconciler confirms: same tag, same"))
                .child(Text::caption("constraints, no dirty flag → cache hit."))
        );

        let static_b = Card::new(
            Column::new().spacing(10.0)
                .child(Text::heading("Static Panel B"))
                .child(Text::caption("Another static panel — also cached."))
                .child(Text::caption("Unchanged components are skipped by"))
                .child(Text::caption("the element cache: build() is not"))
                .child(Text::caption("called again for non-dirty components."))
                .child(Text::caption("Constraint check skips layout too."))
        );

        Column::new().spacing(24.0)
            .padding(EdgeInsets::all(32.0))
            .child(Text::display("Phase 13 — Persistent RenderNode Tree").weight(FontWeight::Bold))
            .child(
                Text::caption(
                    "Dirty-flag rendering: only the counter panel repaints when count changes. \
                     Static panels replay from their cached Picture."
                )
            )
            .child(
                Row::new().spacing(20.0)
                    .child(counter_panel)
                    .child(static_a)
                    .child(static_b)
            )
            .child(
                Card::new(
                    Column::new().spacing(6.0)
                        .child(Text::heading("Phase 13 Internals"))
                        .child(Text::caption("RenderNode per native widget: caches last_constraints, cached_size, cached_picture, cached_rect"))
                        .child(Text::caption("Component element cache: skips build() for non-dirty components"))
                        .child(Text::caption("Layout skip: constraints unchanged + !dirty → return cached_size"))
                        .child(Text::caption("Paint skip: !paint_dirty + cached_picture + rect matches → push commands, skip widget.paint()"))
                        .child(Text::caption("Dirty propagation: atom.set() → mark_dirty(subscribers) → subtree_dirty flag in walk"))
                )
            )
            .into_element()
    }
}

fn main() {
    App::new()
        .title("Phase 13 — Persistent RenderNode Tree")
        .size(1100, 700)
        .launch(Phase13Demo);
}
