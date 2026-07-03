# TEZZERA — WIDGET API DESIGN
> The constitution for every public widget API. New widgets MUST follow this.
> Existing violations are migrated in Phase 21. Decisions: D093–D097.

---

## 1. The Constructor Law (D093)

**`new()` takes exactly the widget's required content — nothing else.**
Everything optional is a builder method.

| Widget kind | Constructor | Rule |
|---|---|---|
| Content leaf | `Text::new("hi")`, `Button::new("Save")`, `Icon::new(kind)`, `Image::file(path)` | The one thing you would always pass |
| Single-child wrapper (child required) | `Card::new(child)`, `ScrollView::new(child, atom)`, `Sheet::new(child)`, `Scaffold::new(body)` | Child in constructor — the widget is meaningless without it |
| Single-child wrapper (child optional) | `Container::new().child(x)` | No-arg constructor, `.child()` builder |
| Multi-child | `Column::new().child(a).child(b)` / `.children(vec)` | Always `.child()` + `.children()`, never constructor args |
| State-carrying input | `Checkbox::new(checked)`, `Slider::new(value)`, `TextInput::new()` | Current value in constructor; callbacks via `.on_*()` builders |

Corollaries:
- Never two required positional args of the same type (`Padding::new(insets, child)` is illegal — ambiguity and argument-order memorization).
- `Default` implemented iff `new()` takes no args.
- Named convenience constructors are encouraged (`Text::title(..)`, `Divider::horizontal()`), but the plain `new()` MUST also exist whenever a named variant exists.

## 2. Styling: builder chain, not style structs (D096)

Evaluated for `Text`:

```rust
// A — builder chain (CHOSEN)
Text::new("hello").size(20.0).weight(FontWeight::Bold).color(c)

// B — style struct argument (REJECTED as primary)
Text::new("hello", TextStyle { size: 20.0, ..Default::default() })
```

Why A: Rust has no named/optional arguments, so B forces
`..Default::default()` noise on every call site; A is IDE-discoverable
(autocomplete on the value), needs no extra imports, and defaults are free.
Every UI toolkit in Rust (iced, egui) converged here.

B's one advantage — reusable styles — is recovered additively later:
`.style(TextStyle)` as ONE builder method that applies a shareable struct,
which also becomes the bridge to `tezzera-style`'s ComputedStyle. Deferred
until the style-system integration phase; not required for consistency.

## 3. The Property Vocabulary (D094)

One name per concept, everywhere. A widget either uses the canonical name or
doesn't expose the property.

| Concept | Canonical builder | Notes |
|---|---|---|
| Surface/fill color | `.background(Color)` | NEVER `.color()` or `.bg()` for surfaces |
| Content/foreground color | `.color(Color)` | Text, Icon, Divider — the thing itself |
| Border | `.border(Color, width)` | One call; radius is separate |
| Corner radius | `.radius(f32)` | Applies to fill, border, AND shadow (vocabulary guarantees this since 5f184ef) |
| Shadow | `.elevation(f32)` | Material-style single knob; `.shadow(Color, blur)` only for full control |
| Inner padding | `.padding(EdgeInsets)` | `EdgeInsets::all/symmetric/only` |
| Fixed size | `.width(f32)` / `.height(f32)` / `.size(w, h)` | |
| Gap between children | `.spacing(f32)` | Flex containers only |
| Child alignment | `.align(Alignment)` | Single-child containers |
| Press callback | `.on_press(fn)` | NEVER `.on_click`/`.on_tap` |
| Change callback | `.on_change(fn)` | Inputs |
| Disabled state | `.disabled()` | Flag-style: no bool arg |

## 4. Parent-Widget Behavior Contract

Every single-child container lays out identically:
1. Shrink incoming constraints by its padding
2. Measure the child with the shrunk constraints
3. Own size = child size + padding, clamped by explicit `.width/.height`, clamped by constraints
4. Paint: background → border → child at padded, aligned rect
5. A child may NOT paint outside its allotted rect (Text enforces this;
   per-node clip lands with Phase 20 Step 5)

`layout()` and `paint()` must agree — what is measured is what is painted.

## 5. Widget Consolidation (D095)

**Too many widgets doing one widget's job. One box: `Container`.**

| Dies | Replaced by |
|---|---|
| `ColoredBox` | `Container::new().background(c).size(w, h)` |
| `SizedBox` | `Container::new().size(w, h)` (`.child()` optional) |
| `Padding` | `Container::new().padding(insets).child(x)` |
| `Center` | `Container::new().align(Alignment::Center).child(x)` |
| `Expanded::empty()` | `Spacer::flex()` (or plain `Spacer`) |
| `tezzera-layout` widget structs (element-based `Column`, `Row`, `Stack`, `SizedBox`, `Spacer`, `Flex`, `Expanded`) | tree widgets are canonical; the layout crate keeps ONLY the math (`layout_column/row`, `Constraints`, alignments). `Grid`, `Wrap`, `AspectRatio` math is ported to tree widgets when first needed. |
| `ListView` (if it is just a Column) | audit in Phase 21; keep only if it virtualizes |

**Survivors and their jobs (no overlap):**
- `Container` — the box: size, padding, background, border, radius, shadow, alignment, optional child
- `Column` / `Row` / `Stack` — multi-child arrangement
- `Spacer` — gap inside flex (fixed or flex)
- `Expanded` — flex-fill wrapper around a child
- `Card` — kept as a themed shorthand for `Container` (surface color + radius + elevation from theme). It must stay a thin preset; if it grows its own properties beyond theming, fold it in.
- `ScrollView` — viewport + clip (API per D097)

Rule of thumb going forward: **a new widget must either draw something new
or lay out something new. "Existing widget with different defaults" is a
named constructor or a theme preset, not a widget.**

## 6. Scroll API (D097)

- `ScrollView::new(child, scroll_y: Atom<f32>)` — live scrolling is the default;
  the current silent-static `new` is the trap that shipped a broken demo.
- `ScrollView::fixed(child, offset)` — honest name for snapshot/golden-test mode.
- `.live_x(atom)`, `.axis()` unchanged.
- `Column::scrollable(atom)` / `Row::scrollable(atom)` — planned sugar that
  wraps self in a ScrollView; `Expanded` is ignored on an unbounded scroll axis.

**Unbounded-axis doctrine** (kills Flutter's ScrollView/Expanded/Center pain):
1. Flex on an unbounded axis is DEFINED: ignored, child sizes to content.
   Never a crash. Debug builds emit a trace naming the widget and the fix.
2. ScrollView content constraints on the scroll axis are
   `min = viewport, max = Unbounded` — short content can center or
   space itself against the full viewport with zero boilerplate
   (Flutter's LayoutBuilder + ConstrainedBox(minHeight) recipe, as the
   default), long content scrolls.
3. `AxisBound::Unbounded`/`Shrink` are named states, not `f32::INFINITY` —
   framework messages and author code pattern-match them explicitly.

## 7. Navigation (also D097)

- **`ScreenNav<R>` is the canonical routing API.** Enum of screens +
  `push/pop/replace/current/can_pop`.
- `tezzera-nav`'s `Navigator`/`Route`/`history` and `tezzera-nav-anim`'s
  `Navigator` are internal machinery or future transition layers — not
  exported from the prelude. One way to navigate.
- `AppBar::back_button(&nav)` — the `can_pop → leading("← Back")` boilerplate
  becomes one call.

## 8. What consistency buys

Learning curve = number of rules, not number of widgets. Target: a user who
has used TWO widgets can guess the API of every other widget correctly.
