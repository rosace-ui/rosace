# ROSACE — WRITING A WIDGET
> Phase 21 Step 6. The practical companion to `WIDGET_PROTOCOL.md` (the
> planning spec, D098–D100) — this document describes the API as it actually
> shipped, verified against the current code, not the aspirational one.
> Compiling proof: `rosace-examples/src/bin/widget_authoring_demo.rs`.

---

## Do you need this document?

Almost certainly not yet. 95% of app code only ever writes `Component`s
composing built-in widgets (`Column`, `Button`, `ScrollView`, ...) — that's
just Rust structs and `impl Component`, no `Widget` trait involved. This
guide is for the remaining 5%: someone building a genuinely new visual
primitive that isn't a composition of existing widgets.

Before implementing `Widget`, check the escape-hatch ladder in §5 — the
answer is often a lower rung than "implement Widget from scratch."

## 1. Two concepts, not three

| Concept | Answers | You implement | Rebuilds |
|---|---|---|---|
| `Component` | *what* to show | `build(&self, ctx) -> Element` | when its atoms change |
| `Widget` | *how* to size / draw / respond to input | `layout` / `paint` (+ `children`, `flex_factor`) | never — repainted when dirty |

`Element`, `RenderTree`, and the reconciler are internal. You never touch
them except calling `.into_element()` at the boundary where a `Widget`
becomes something a `Component::build()` can return.

## 2. The real trait

```rust
pub enum Children<'a> {
    None,                    // leaf — draws, has no children
    One(&'a dyn Widget),     // single-child wrapper
    Many(&'a [BoxedWidget]), // multi-child container
}

pub trait Widget: Send + Sync {
    fn children(&self) -> Children<'_> { Children::None }
    fn layout(&self, ctx: &LayoutCtx) -> Size { /* default per children() */ }
    fn paint(&self, ctx: &mut PaintCtx) { /* default per children() */ }
    fn flex_factor(&self) -> f32 { /* default per children() */ }
}
```

`children()` is a **declaration**, not a data structure the framework walks
for you at layout/paint time — it exists so the DEFAULT implementations of
`layout`/`paint`/`flex_factor` know what to do. That's the whole taxonomy:

| Writing a… | `children()` | Must implement | Gets defaulted |
|---|---|---|---|
| Leaf | `None` (default — omit `children()` entirely) | `layout` + `paint` | `flex_factor` = 0 |
| Single-child wrapper | `One(&self.child)` | whichever of `layout`/`paint` you're changing | the other one delegates to the child; `flex_factor` delegates to the child |
| Multi-child container | `Many(&self.items)` | `layout` (no universal multi-child arrangement exists) | nothing — `paint`'s default just stacks everything in the same rect, which is rarely what you want, so containers override `paint` too |

**Important divergence from the original planning doc** (`WIDGET_PROTOCOL.md`
§3): there is no framework-owned `layout_child`/`position_child`/
`paint_child` memoization today. That part of D099 was descoped during
Phase 21 (per-widget content equality can't be detected safely without
widget reconciliation, which doesn't exist yet — see D099's Phase 21 progress
notes). In practice: a multi-child container measures its children by
calling `item.layout(ctx)` directly and positions them itself in `paint()`
(see `EvenColumn` below, or real built-ins like `Column`, which still keeps
its own private measure cache). If you're writing a container, expect to
measure-then-position by hand, not lean on a framework cache.

There is also no `hit()` or `semantics()` trait method. Both are runtime
**declarations** made during `paint()` via `PaintCtx` methods (§3), not
separate trait overrides — see below.

## 3. Interaction and semantics (declared in `paint`, not separate methods)

```rust
// Interaction — call these inside paint():
ctx.register_hit(callback);           // raw click region = ctx.rect
ctx.on_press(|| { ... });              // sugar over register_hit, fires once per click
ctx.on_press_at(|x, y| { ... });       // POSITIONAL — becomes an active drag grab,
                                        // streamed MouseMove until release (sliders, knobs)
ctx.on_scroll(|dx, dy| { ... });       // wheel/trackpad over ctx.rect
ctx.on_long_press(|| { ... });         // 500ms, 8px slop
ctx.hoverable();                       // participates in hover tracking (ctx.hovered())

// Semantics (D099) — the accessibility tree the frame derives every frame:
ctx.semantics(Semantics::new(Role::Button).label(&self.label));
```

These persist on the widget's render-tree node (D091) — you declare them
every `paint()` call, but they don't vanish on cache-hit/clean frames the way
a naive "re-register every frame" scheme would suggest; the render tree
retains them structurally. You don't need to think about caching here at
all — just declare what's true this frame.

## 4. Three worked examples — one per taxonomy row

All three compile and run today — see
`rosace-examples/src/bin/widget_authoring_demo.rs` (`cargo run --bin
widget_authoring_demo`).

### Leaf: `Dot`

```rust
struct Dot { radius: f32, color: Color }

impl Widget for Dot {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let d = self.radius * 2.0;
        ctx.constraints.constrain(Size { width: d, height: d })
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let center = Point {
            x: ctx.rect.origin.x + ctx.rect.size.width / 2.0,
            y: ctx.rect.origin.y + ctx.rect.size.height / 2.0,
        };
        ctx.fill_circle(center, self.radius, self.color);
    }
}
```
No `children()` override needed — `Children::None` is the default. Nothing
delegates; you own both `layout` and `paint` completely.

### Single-child wrapper: `Highlight`

```rust
struct Highlight { child: Box<dyn Widget>, glow: Color }

impl Widget for Highlight {
    fn children(&self) -> Children<'_> { Children::One(&*self.child) }
    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.fill_shadow_rrect(ctx.rect, 12.0, self.glow, 16.0);
        let rect = ctx.rect;
        self.child.paint(&mut ctx.child(rect));
    }
    // layout: defaulted — Children::One means "my size = my child's size".
}
```
Declaring `children() -> One` gets you free `flex_factor` delegation (a
`Highlight` inside a `Row`'s flex slot behaves like whatever it wraps) even
though you still write `paint` by hand. `ctx.child(rect)` is the real
child-paint-context constructor — there's no `paint_child(0)` shortcut today.

### Multi-child container: `EvenColumn`

```rust
struct EvenColumn { items: Vec<Box<dyn Widget>>, gap: f32 }

impl Widget for EvenColumn {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let (mut y, mut w) = (0.0f32, 0.0f32);
        for item in &self.items {
            let s = item.layout(ctx);
            y += s.height + self.gap;
            w = w.max(s.width);
        }
        ctx.constraints.constrain(Size { width: w, height: (y - self.gap).max(0.0) })
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let mut y = ctx.rect.origin.y;
        for item in &self.items {
            let s = item.layout(&ctx.layout_ctx(Constraints::loose(ctx.rect.size.width, f32::INFINITY)));
            let child_rect = Rect {
                origin: Point { x: ctx.rect.origin.x, y },
                size: Size { width: ctx.rect.size.width, height: s.height },
            };
            item.paint(&mut ctx.child(child_rect));
            y += s.height + self.gap;
        }
    }
}
```
No default exists for `Many`'s layout (there's no universal multi-child
arrangement) — you always override it. This is also the shape real built-in
containers take (`Column`, `Row`, `Stack`) — measure each child, decide
positions, paint at those positions. `Column` still keeps a private
`Mutex<measure_cache>` for this today; that's the honest current state, not
an anti-pattern you need to avoid copying.

## 5. Escape hatches — reach for the lowest rung that solves your problem

1. **Compose built-ins in a `Component`.** Most needs. No `Widget` involved.
2. **Implement `Widget`** with defaults (this document). A genuinely new
   visual primitive that composition can't express.
3. **`CustomPaint::new(|ctx, size| { ... })`** — a `Leaf` with a closure
   instead of a struct, when you don't need reusable state/config (D100).
4. **`ctx.record(DrawCommand::...)`** — raw display-list access for an
   existing `DrawCommand` variant `fill_rect`/`fill_circle` etc. don't cover.
5. **`DrawCommand::BlitRgba`** — bring your own pixel buffer.
6. **A new `DrawCommand` variant** — a framework contribution, not an app
   concern; extends the vocabulary every renderer must implement.

Each rung stays inside the retained pipeline (D091) — caching, damage-rects,
and hit-testing all keep working. There is no "drop to raw canvas access"
escape hatch that bypasses this; that was an explicit non-goal (D100).

## Where the rest lives

- `WIDGET_PROTOCOL.md` — the original planning spec (D098–D100) this guide
  implements. Read it for the *reasoning*; read this guide for the *current
  API*. Where they disagree (framework-owned child geometry, `hit()`/
  `semantics()` as trait methods), this document is correct — the plan was
  descoped in the ways noted above.
- `DECISIONS.md` D098–D100 — the locked decisions.
- `rosace-examples/src/bin/widget_authoring_demo.rs` — the compiling,
  running proof of the three examples above.
