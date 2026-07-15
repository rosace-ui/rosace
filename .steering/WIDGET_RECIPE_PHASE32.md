# Phase 32 Widget Recipe (hard-won, follow exactly)

The concrete patterns every new Phase 32 widget MUST follow. Derived from
BottomNavigationBar/FloatingActionButton/SearchBar/Snackbar/Expander
(all landed + live-verified). Read `WIDGET_AUTHORING_GUIDE.md` first for
the Widget trait taxonomy; this file is the practical checklist.

## Theme colors — the #1 gotcha
`ctx.theme.colors` fields are `rosace_theme::Color`. Painting APIs want
`rosace_render::Color`. CONVERT with `ctx.tc(t.primary)`. They are
DISTINCT types; `unwrap_or(t.surface)` will not compile.

## Borrow hoisting — the #2 gotcha
`let t = &ctx.theme.colors;` borrows ctx immutably; any `ctx.fill_rect`/
`draw_rounded_rect_pub(ctx, ...)` needs ctx mutably → conflict. HOIST all
theme reads into a tuple in a block that ends before painting:
```rust
let (bg, fg, outline) = {
    let t = &ctx.theme.colors;
    (self.bg.unwrap_or_else(|| ctx.tc(t.surface)),
     ctx.tc(t.on_surface), ctx.tc(t.outline))
};
```

## Interactive-by-identity (user directive)
Any widget a user can click/tap ALWAYS registers a hit region, even with
no callback wired — else the click falls through to drag-to-pan behind
it. Unwired = absorb: `ctx.register_hit(Arc::new(|| {}))` (or
`ctx.on_press(|| {})` / `ctx.on_press_at(|_,_| {})` for positional).

## Customization builders (D094, mandatory for every widget)
Expose where they apply: `.background(Color)`, `.color(Color)` (content),
`.border(Color, f32)`, `.radius(f32)`, `.padding(EdgeInsets)`, size,
`.elevation(f32)`. Defaults come from theme tokens, NOT hardcoded.

## Drawing helpers (rosace_widgets::tree)
- `draw_rounded_rect_pub(ctx, rect, color, radius)` — filled rrect
- `ctx.fill_rect(rect, color)`, `ctx.stroke_rrect(rect, radius, color, width)`
- `ctx.fill_circle(center, radius, color)`
- `ctx.draw_text_at(&str, Point, color, px)`, `ctx.font.measure_text(s, px)`,
  `ctx.font.line_height(px)`
- `ctx.child(rect) -> PaintCtx` for a sub-rect; `ctx.layout_ctx(constraints)`
- `ctx.animate_to(target, from)` — theme-eased 0..1 for hover/reveal
- `ctx.semantics(super::Semantics::new(Role::X).label(..))`

## Registration (each new widget)
1. `tree/<name>.rs` — the widget + `#[cfg(test)] mod tests` with a layout test
2. `tree/mod.rs`: `pub mod <name>;` AND `pub use <name>::{Type, ...};`
3. `lib.rs`: `pub use tree::{Type, ...};`
4. `prelude.rs`: add `Type` to the big `pub use crate::{ ... }` block

## API signatures you'll reach for (exact)
- `Container::new().child(w).background(c).radius(r).padding(EdgeInsets::all(n))`
- `Button::new("x").width(n).disabled()` — `.disabled()` takes NO arg
- `Column::new().spacing(n).child(w).children(vec![Box::new(w) as BoxedWidget])`
- `Row::new().spacing(n).main_axis_alignment(MainAxisAlignment::Center)`
- `Text::new(s).size(n).color(c).weight(FontWeight::Bold).align(TextAlign::Center)`
- `Icon::new(IconKind::Star).size(n)`
- `Constraints::loose(w, h)` / `::tight(w, h)`; `avail_w(c)` / `avail_h(c)`

## Layout test pattern
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;
    #[test]
    fn lays_out_as_expected() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let size = MyWidget::new().layout(&ctx);
        assert_eq!(size.width, 400.0);
    }
}
```

## Before you finish
`cargo test -p rosace-widgets` and `cargo clippy -p rosace-widgets
--all-targets -- -D warnings` MUST be clean. Do NOT run app demos or
screenshot (the parent does live verification). Do NOT edit any file
outside your assigned scope + the 4 registration files.
