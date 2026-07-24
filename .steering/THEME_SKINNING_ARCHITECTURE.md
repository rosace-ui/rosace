# Theme Skinning Architecture — pluggable widget appearance (DRAFT / to lock)

> Status: PLANNED, **implementation deliberately DEFERRED** (2026-07-24, user
> call). It is an OPTIONAL, additive paint-override hook — non-breaking to add
> at any time — so we keep maturing widget *defaults* now and add the hook once
> as a clean, well-designed one-time layer after ~5–6 widgets are matured (the
> `*Visual` contract generalizes better designed against several widgets than
> one). Generalizes the proven `AppBarStyle`/`SelectionStyle` pattern.

## Refined model (user, 2026-07-24) — OPTIONAL override, not mandatory extraction
- The widget **paints itself by default** (its built-in Material-ish paint —
  the bodies already in `switch.rs`/`checkbox.rs`/`slider.rs` stay put).
- A skin is an **optional override**: at the top of `paint`, the widget asks the
  theme "is there a painter registered for me?" — if yes, build a `*Visual`
  descriptor and delegate; if no, run the built-in. **No per-widget rewrite.**
- **No Cupertino skin is built by us.** One standardized design system (what big
  companies do). The modern alternate look is glassmorphism, for which the
  primitives already exist (`ShaderMaterial`, glass selection). We ship the
  *capability*; third parties (or a future us) provide skins if wanted.
- **Fully-custom widgets already work** via the `Widget`/RenderObject trait — the
  skin hook is only for *reskinning an existing* widget, not for new ones.
- Per-widget cost is only: define + document that widget's `*Visual` contract.
  Widgets without one simply aren't skinnable yet (their default always runs) —
  and that's fine.

## The principle
**A widget owns its *behavior*; a *skin* owns its *appearance* (colors AND
shapes).** A dynamic theme therefore changes not just tokens but the actual
*form* of controls — a Cupertino skin makes a real iOS switch, not a recolored
Material one. Rejected outright: `if platform == Cupertino { … } else { … }`
scattered through widgets (unmaintainable, closed to third parties).

```
Widget  = WHAT it is  → state, geometry, interaction, focus, a11y, animation
Skin    = HOW it looks → pixels (and the metrics that drive size)
Theme   = tokens + one Arc<dyn Skin>
```

We ship **`MaterialSkin`** as the default. Anyone (us or a third party) can ship
another skin — `CupertinoSkin`, `FluentSkin` — as a plugin crate and drop it
into a `ThemeData`. The `Themes` bundle already maps `Platform → ThemeData`, so
`cupertino()` = Material tokens-adjusted + `Arc<CupertinoSkin>`.

## The Registry (user vision, 2026-07-24) — the middle ground
The whole point: a user who wants to change *something small* should NOT have to
build a RenderObject and redo the interaction/layout/animation math. They
**register an override**; the framework **calls it whenever it displays that
widget**. Everything is optional — register nothing, get all defaults.

Conceptually the framework "asks" per widget kind: *"Do you have a Switch style?
A Card style? A Radio style?"* — if the registry has one, use it; if not, "that's
fine, I have one" (the built-in default). Example: the default `Radio` is round
with an inner dot; a user who wants a **square** radio registers that once and we
call it everywhere a `Radio` renders — no RenderObject, no math.

### Two tiers of override per widget (both optional, pick your effort)
1. **Style data** — a plain-data struct the *default* painter reads
   (`SwitchStyle { thumb_shape, track_h, has_halo, radius, … }`,
   `RadioStyle { shape: Round|Square, … }`). For "I just want to change a
   thing." Cheapest.
2. **Full painter** — a closure/trait that draws the whole widget from its
   `*Visual` descriptor. For "I want it to look completely different." Full
   control, still no interaction/layout/a11y work.

Paint resolution order in the widget: **full painter? → use it. Else default
painter, parameterized by registered style data (or built-in defaults).**

### Where the registry lives
It IS the `ThemeData` extension type-map that already exists
(`with_ext`/`ext::<T>()`) — a `SwitchStyle`/`SwitchPainter` is just a typed entry
keyed by its Rust type. So:
- A **skin = a theme carrying a set of registered overrides.** Swapping the
  theme swaps shapes, not just colors — exactly the "dynamic theme" goal.
- Ergonomic sugar on top: `theme.with_style::<Radio>(RadioStyle::square())` /
  `theme.with_painter::<Switch>(my_painter)`. App-global registration is an
  optional convenience over the same map.
- Fully optional at every level; unregistered widgets always self-paint.

## The hard boundary (what a skin may and may NOT touch)
This split is the whole safety story — a third-party skin can restyle
everything and **cannot break behavior or accessibility**:

| Framework-owned (a skin never sees/changes) | Skin-owned |
|---|---|
| Hit region / interactive-by-identity | The drawn pixels |
| Focus node, focus order, keyboard | Metrics that drive intrinsic size |
| Semantics (role/label/value) | Which visual states map to which look |
| Animation *state* (the eased channels) | How those eased values are *drawn* |
| The value + on_change contract | — |

A skin is a **pure painter + metrics provider**. It receives an immutable
*Visual* descriptor (already-eased animation values, resolved state, rect) and
draws into `PaintCtx`. It cannot register hits, mutate the tree, or change
semantics. If a skin method isn't overridden, the Material default runs.

## The mechanism
One `Skin` trait, **one method per widget kind, each defaulting to Material**:

```rust
pub trait Skin: Send + Sync {
    fn switch_metrics(&self) -> SwitchMetrics { material::switch_metrics() }
    fn paint_switch(&self, ctx: &mut PaintCtx, v: &SwitchVisual) { material::paint_switch(ctx, v) }
    fn paint_checkbox(&self, ctx: &mut PaintCtx, v: &CheckboxVisual) { material::paint_checkbox(ctx, v) }
    fn paint_slider(&self, ctx: &mut PaintCtx, v: &SliderVisual) { material::paint_slider(ctx, v) }
    // … one pair (metrics? + paint) per skinnable widget
}
```

- A third-party skin `impl Skin for CupertinoSkin` overrides only the widgets it
  restyles; everything else falls back to Material for free.
- `ThemeData` carries `skin: Arc<dyn Skin>` (default `Arc<MaterialSkin>`).
- Widgets call `ctx.theme.skin.paint_switch(ctx, &visual)`.

### The Visual descriptor = the plugin API (design carefully, version it)
Per widget, a plain-data struct is THE stable contract skins code against:

```rust
pub struct SwitchVisual {
    pub rect: Rect,
    pub position: f32,  // eased 0(off)→1(on)   ← animation already applied
    pub halo: f32,      // eased state-layer opacity (hover/press/focus)
    pub press: f32,     // eased press amount
    pub disabled: bool,
    pub focused: bool,
    // resolved tokens the skin may use (or it reads ctx.theme.colors itself)
}
```

The **widget** computes this (interaction booleans → `animate_channel` → eased
floats), then hands it to the skin. Animation stays framework-owned so motion is
consistent and a skin can't leak per-frame state.

## Widget authoring shape (after this lands)
```rust
impl Widget for Switch {
    fn layout(&self, ctx) -> Size { ctx.theme.skin.switch_metrics().size /* constrained */ }
    fn paint(&self, ctx) {
        ctx.semantics(...);          // framework-owned
        ctx.register_hit(...);       // framework-owned
        let focused = ctx.focus_node().is_focused();
        let v = SwitchVisual { position: ctx.animate_channel(0, ...), halo: ..., ... };
        ctx.theme.skin.paint_switch(ctx, &v);   // skin-owned pixels
    }
}
```
The Material painter is literally the body I already wrote in `switch.rs`, moved
behind `MaterialSkin::paint_switch`. So the work I've done isn't thrown away —
it *becomes* the default skin.

## Rollout (deferred — do it once, after the control family is matured)
Widgets keep their built-in paint the whole time; this is purely additive.
1. **Mature the control family with built-in paint** (Switch ✓, Checkbox ✓,
   Slider ✓, Radio, Chip, …). NO skin work during this — just keep each
   widget's `paint()` self-contained (it already is).
2. **Then, one pass:** add the registry lookup + define the `*Visual` contracts,
   designed against the 5–6 widgets now in hand (so the contract generalizes).
   For each of those widgets: extract the state it computes into a `*Visual`,
   and top its `paint` with "registered painter? → delegate, else self-paint."
   **Zero visual change** — verify by re-rendering identical PNGs.
3. **Add the two-tier registry sugar** (`with_style::<W>()` / `with_painter::<W>()`)
   over the existing `ThemeData` ext map.
4. **Bake into the Quality Bar** (only from then on): "widget exposes a
   registry hook + documents its `*Visual` contract" becomes a DoD item.
5. **We do NOT ship an alternate skin** (no Cupertino). We ship the *capability*
   + one worked example in the docs; third parties or a future us supply skins.

## Open decisions to lock (when we pick this up)
- **D-SKIN-1**: Registry = the existing `ThemeData` ext type-map, keyed by a
  per-widget style/painter type. Two tiers: style-data (default painter reads
  it) and full painter (replaces paint). Both optional. ✅ chosen.
- **D-SKIN-2**: No alternate skin built by us (dropped Cupertino — one design
  system; glassmorphism is the modern alternate and its primitives already
  exist). We ship the hook + docs only. ✅ chosen.
- **D-SKIN-3**: `*Visual` stability — `#[non_exhaustive]` + constructor so
  adding fields never breaks a registered painter. ✅ lean.
- **D-SKIN-4**: Metrics — a registered *style* may also carry intrinsic-size
  fields the widget's `layout` reads, so a tweak can resize (square radio,
  bigger switch) without a RenderObject. ✅.
- **D-SKIN-5**: A registered painter that panics surfaces in dev (skins are
  first-party-ish trusted); no silent catch. ✅ lean.
