# ROSACE — WIDGET PROTOCOL
> The authoring contract for custom widgets. Decisions D098–D100.
> This is what Phase 21 builds; the API-consistency sweep (Phase 22)
> then migrates built-ins onto it.

---

## 1. The two-concept model (D098)

Flutter makes authors learn three trees (Widget → Element → RenderObject)
and three subclass families. ROSACE exposes exactly **two concepts**:

| Concept | Question it answers | Author implements | Rebuild cadence |
|---|---|---|---|
| **`Component`** | *what* to show | `build(&self, ctx) -> Element` | when its atoms change |
| **`Widget`** | *how* to size / draw / behave | `layout` / `paint` / `semantics` / `hit` | never rebuilt — repainted when dirty |

Everything else — `Element`, `RenderTree`, reconciler, picture caches — is
**internal**. Documentation rule: a user never types `Element` or `RenderNode`
except `into_element()` at a component boundary.

95% of users only ever write `Component`s composing built-in widgets.
The remaining 5% implement `Widget` — and that protocol is this document.

## 2. Taxonomy: one trait, defaults do the work (D098)

Flutter's Leaf/SingleChild/MultiChild classes exist because Dart resolves
behavior by inheritance. In Rust, three taxonomy traits with blanket
`impl Widget` collide with coherence rules. Instead: **one trait, one
structure accessor, smart defaults keyed off it**:

```rust
pub enum Children<'a> {
    None,                       // leaf
    One(&'a dyn Widget),        // single-child wrapper
    Many(&'a [BoxedWidget]),    // multi-child container
}

pub trait Widget: 'static {
    /// Structure declaration — drives every default below.
    fn children(&self) -> Children<'_> { Children::None }

    /// Default: leaf → constraint-min; One → child's size (padded by nothing);
    /// Many → must override (there is no universal multi-child layout).
    fn layout(&self, cx: &mut LayoutCx) -> Size { /* default per children() */ }

    /// Default: paint children at the positions stored during layout.
    /// Leaves paint nothing by default.
    fn paint(&self, cx: &mut PaintCx) { /* default per children() */ }

    /// Default: own rect + children, back-to-front (structural z-order).
    fn hit(&self, cx: &mut HitCx) { /* default */ }

    /// Default: no role; children recursed. Declarative — see §5.
    fn semantics(&self, cx: &mut SemanticsCx) { /* default */ }

    /// Flex weight inside Row/Column. Default 0 (not flexible).
    fn flex(&self) -> f32 { 0.0 }
}
```

The taxonomy is then a **decision table, not a type choice**:

| You are writing a… | Implement | Everything else |
|---|---|---|
| Leaf (draws something) | `layout` + `paint` | defaulted |
| Single-child wrapper | `children()` → `One` + the ONE method you change | defaulted — delegation is free |
| Multi-child container | `children()` → `Many` + `layout` | `paint`/`hit` defaulted from stored positions |

## 3. Framework-owned child geometry (D099) — the big win

Today every container privately re-measures children and hand-rolls a
`Mutex<measure_cache>`; positions are recomputed in `paint()`; layout and
paint can drift apart. The protocol moves child geometry into the framework
(stored on the widget's RenderTree node, per D091):

```rust
impl LayoutCx<'_> {
    fn constraints(&self) -> Constraints;
    fn child_count(&self) -> usize;
    /// Measures child `i`. MEMOIZED by the framework per (child, constraints)
    /// on the render tree — Column's Mutex cache dies, and unchanged
    /// subtrees skip re-layout automatically.
    fn layout_child(&mut self, i: usize, c: Constraints) -> Size;
    /// Stores child i's offset (relative to this widget's origin).
    fn position_child(&mut self, i: usize, at: Point);
    // text metrics + theme access as today
}

impl PaintCx<'_> {
    fn rect(&self) -> Rect;
    /// Paints child i at its stored position (clip- and offset-aware).
    fn paint_child(&mut self, i: usize);
    // canvas vocabulary as today: fill_rect, fill_rrect, stroke_rrect,
    // text, fill_shadow_rrect, push_clip/pop_clip, record(DrawCommand)
}
```

Consequences (each one kills a current pain):
1. `layout()` runs once; `paint()` reads stored positions — measure/paint
   drift becomes impossible (the Text-overflow bug class, structurally gone).
2. Per-widget `Mutex<Option<(Constraints, Vec<Size>)>>` caches deleted.
3. Because children are addressable by the framework, per-child picture
   caching and damage-rects (Phase 20 Steps 1/5) get their tree for free —
   this protocol IS the missing bridge.
4. Default `paint`/`hit` can exist at all (they need positions).

A full custom multi-child container becomes:

```rust
struct EvenColumn { items: Vec<BoxedWidget>, gap: f32 }

impl Widget for EvenColumn {
    fn children(&self) -> Children<'_> { Children::Many(&self.items) }
    fn layout(&self, cx: &mut LayoutCx) -> Size {
        let (mut y, mut w) = (0.0, 0.0f32);
        for i in 0..cx.child_count() {
            let s = cx.layout_child(i, cx.constraints().loosen());
            cx.position_child(i, Point { x: 0.0, y });
            y += s.height + self.gap;
            w = w.max(s.width);
        }
        cx.constraints().constrain(Size { width: w, height: y - self.gap })
    }
    // paint, hit, semantics: defaults
}
```

And a single-child effect wrapper:

```rust
struct Glow { child: BoxedWidget, color: Color }

impl Widget for Glow {
    fn children(&self) -> Children<'_> { Children::One(&*self.child) }
    fn paint(&self, cx: &mut PaintCx) {
        cx.fill_shadow_rrect(cx.rect(), 8.0, self.color, 12.0);
        cx.paint_child(0);
    }
    // layout (size = child), hit, semantics: defaults
}
```

Compare today: `Glow` would hand-write `layout` delegation, `flex_factor`
delegation, a manual `ctx.child(rect)` computation, and get no semantics.

## 4. Interaction protocol

- `PaintCx::on_press(cb)` / `on_scroll(cb)` declare regions onto the render
  tree node (Phase 20 mechanics — persistence and z-order are framework
  guarantees, not author responsibilities).
- `hit()` override is for non-rectangular targets only (circular knob,
  path-shaped). Default = own declared regions + children, back-to-front.

## 5. Semantics protocol (D099, activates D035/D064)

`SemanticNode`/`Role` exist today but nothing produces them. The protocol
makes semantics a declarative hook that writes to the widget's render-tree
node (single-owner, D091), so the a11y tree is DERIVED like hit-testing:

```rust
fn semantics(&self, cx: &mut SemanticsCx) {
    cx.role(Role::Button)
      .label(&self.label)
      .enabled(!self.disabled)
      .action(SemanticsAction::Tap);
}
```

- Defaults: `Text` declares `Role::Text` + its content automatically;
  containers recurse; authors override only for interactive/custom widgets.
- Platform bridges (VoiceOver/ARIA/etc.) consume the derived tree in a
  later phase; the authoring API ships NOW so widgets accumulate semantics
  from day one instead of retrofitting hundreds later.

## 6. CustomPaint (amends D034 → D100)

With this protocol, CustomPaint stops being special machinery:

```rust
CustomPaint::new(|cx: &mut PaintCx, size: Size| {
    cx.fill_circle(...);           // records commands — never touches pixels
})
.repaint_when(atom)                // dirty-couples to reactive state
```

It is a `Leaf` with a closure — recorded into the display list like every
widget, so caching/replay/damage all apply. D034's "full SkiaCanvas access"
wording is superseded: direct pixel access would bypass the retained
pipeline (the D091 bug class). Pixel-level needs use `BlitRgba`.

## 7. Escape hatches (because "we can't think what they need")

In increasing power, all retained-pipeline-safe:
1. Compose built-ins in a `Component`
2. Implement `Widget` with defaults (this protocol)
3. `CustomPaint` closure
4. `cx.record(DrawCommand)` — raw display-list access
5. `DrawCommand::BlitRgba` — bring your own pixels
6. New `DrawCommand` variant (framework contribution — vocabulary rule,
   API_DESIGN.md)

## 8. Non-goals

- No user-facing Element subclassing, no render-object registration, no
  three-tree authoring. If a need arises that this protocol cannot express,
  the answer is extending the protocol, not exposing internals.
