# Layout & Widgets

`Component::build` returns an `Element` tree, but you almost never build `Element`s by hand — you compose **widgets**, and call `.into_element()` at the root. This chapter covers the widget set you'll reach for on every screen: how flex layout works, how boxes/padding/alignment work, and which widget to pick for a given shape of UI.

## Widgets vs. the Element tree

A **widget** is any type implementing the `Widget` trait — it knows how to measure itself (`layout`) and how to draw itself (`paint`). Widgets compose by holding other widgets as children (`Column`, `Container`, etc. all just hold `Box<dyn Widget>`). `Element` — the tree `Component::build` actually returns — is the thin wrapper the framework needs to mount a widget subtree under a component; `Widget::into_element()` produces it. You'll see `Element` in signatures, but you build with widgets. (More on why the split exists: [Core: Component, Element, Context](../architecture/core.md).)

## Column and Row: flex layout

`Column` stacks children top-to-bottom; `Row` stacks them left-to-right. Both are flex containers with the same shape:

```rust
Column::new()
    .spacing(12.0)                                    // gap between children
    .padding(EdgeInsets::all(16.0))
    .main_axis_alignment(MainAxisAlignment::Center)    // along the stacking axis
    .cross_axis_alignment(CrossAxisAlignment::Stretch) // across it
    .child(Text::new("Title"))
    .child(Text::new("Subtitle"))
```

`MainAxisAlignment` has `Start` (default), `Center`, `End`, `SpaceBetween`, `SpaceAround`, `SpaceEvenly`. `CrossAxisAlignment` has `Start`, `Center`, `End`, `Stretch`, and `Baseline`. `Row`'s cross-axis default is `Center` (so a row of text and an icon line up naturally); `Column`'s is `Start`.

Give a child `Expanded` (or `Expanded::empty()` for a bare spacer) to make it eat the leftover space on the main axis:

```rust
Row::new()
    .child(Icon::new(IconKind::Star))
    .child(Expanded::new(Text::new("Fills the rest of the row")))
    .child(Button::new("Go"))
```

`Expanded` takes a `.with_factor(2.0)` if you want it to take a larger share relative to sibling `Expanded`s (flex weights). Inside an unbounded axis — e.g. a `Column` nested in a vertical `ScrollView` — there's no finite space to divide, so `Expanded` children fall back to sizing to their own content instead of panicking; you'll see a debug-only warning if that happens.

Both containers offer `.scrollable()` — `Column::new()...child(...).scrollable()` wraps the whole thing in a vertical `ScrollView` (see below) with zero extra wiring.

## Container: the one box

Everything box-shaped — a colored panel, a bordered card, a circle avatar backdrop, a pill-shaped tag — is a `Container`. There's deliberately no `ColoredBox`/`SizedBox`/`Circle` type; **`Container` is the single box widget**, and shape is a property, not a type:

```rust
Container::new()
    .size(64.0, 64.0)
    .shape(BoxShape::Circle)          // Rect (default), Circle, or Stadium (pill)
    .background(Color::rgb(110, 75, 210))
    .border(Color::rgb(60, 65, 95), 1.0)
    .radius(8.0)                      // corner radius, ignored for Circle/Stadium
    .shadow(Color::rgba(0, 0, 0, 90), 12.0)
    .padding(EdgeInsets::all(12.0))
    .align(Alignment::Center)         // fills available space, then aligns the child within it
    .clip()                           // clip the child to the box shape
    .child(Text::new("Hi"))
```

`.elevation(e)` is shorthand for a black drop shadow scaled by `e`. `.gradient(from, to)` / `.gradient_h(from, to)` paint a two-stop gradient instead of a solid `.background()`. A `Container` with no explicit `.width()`/`.height()` shrink-wraps its child; setting `.align(...)` switches it to fill the available space instead (there'd be nothing to align within otherwise).

`Card::new(...)` exists as a themed `Container` preset (elevation + rounded corners from the theme) — reach for it before hand-rolling the same `Container` chain repeatedly.

## Sizing model: constraints flow down, sizes flow up

Every widget's `layout(ctx)` receives `Constraints` — a min/max width and height — and must return a `Size` within them. A parent decides its children's constraints (tight, for "be exactly this size"; loose, for "up to this size"); a child measures itself and reports back; the parent then positions children using those sizes. This is the same top-down-constraints/bottom-up-sizes model as Flutter — there's no cross-talk, no relayout-on-paint. Containers like `Column`/`Row` cache their measure pass and reuse it at paint time (measure and paint must always agree on the same sizes).

You mostly stay above this layer — `.width(200.0)`, `.padding(...)`, `Expanded` — but it explains why, e.g., a `Column`'s `Expanded` needs a *bounded* height to divide: constraints, not content, are what flex math runs against.

## Padding and alignment

`EdgeInsets` is the padding/margin type everywhere: `EdgeInsets::all(v)`, `::symmetric(h, v)`, `::horizontal(h)`, `::vertical(v)`, `::only(top, right, bottom, left)`.

`Alignment` (used by `Container::align`, and anywhere else a single child is placed inside extra space) is one of the nine compass points: `TopLeft`, `TopCenter`, `TopRight`, `CenterLeft`, `Center` (default), `CenterRight`, `BottomLeft`, `BottomCenter`, `BottomRight`.

## Stack and Positioned: overlays

`Stack` draws all its children on top of each other, back-to-front, at the same rect:

```rust
Stack::new()
    .fit(StackFit::Expand)   // or StackFit::Loose (default) — size to the largest child
    .child(Image::asset("banner.png"))
    .child(Positioned::new(Text::new("Caption")).bottom(8.0).left(8.0))
```

`Positioned` anchors a `Stack` child by edges (`top`/`left`/`right`/`bottom`, plus optional explicit `width`/`height`) instead of letting it fill the stack — pass opposite anchors (e.g. both `left` and `right`) and the child is stretched to fit between them.

## Grid: multi-column layout

`Grid::new(columns)` flows children left-to-right, top-to-bottom into equal-width cells (each row's height is its tallest child):

```rust
Grid::new(3)
    .spacing(8.0)       // horizontal gap between columns
    .run_spacing(8.0)   // vertical gap between rows
    .child(Card::new(...))
    .child(Card::new(...))
    .child(Card::new(...))
```

Two more placement modes ride the same builder: `.staggered()` packs each child into the currently-shortest column at its own measured height (Pinterest-style masonry); `.bento()` (or `.child_span(w, col_span, row_span)`, which switches to bento mode implicitly) places children on a fixed lattice where a tile can span multiple columns/rows — useful for dashboard-style layouts. `.row_height(h)` sets the lattice row height for bento mode.

## Scrolling: ScrollView and ListView

`ScrollView` wraps one child that may exceed the viewport; it clips to the viewport and paints the child at a scroll offset:

```rust
ScrollView::new(
    Column::new().spacing(8.0).children(items),
)
```

Scroll position lives on the widget's render-tree node automatically — no atom or controller to wire up. Pass `.controller(ctrl)` (or build with `ScrollView::controlled(child, ctrl)`) only if you need to read/drive the offset programmatically. `ScrollView::horizontal(child)` scrolls sideways; `ScrollAxis::Both` is available via the `axis` field for panning content in two dimensions.

For a **long list**, prefer `ListView::builder` over `ScrollView` — it's virtualized: only the rows actually intersecting the viewport are built, laid out, and painted, so a 10,000-row list costs the same as a 10-row one:

```rust
ListView::builder(1_000, 48.0, |i| {
    Box::new(ListTile::new(format!("Row {i}")))
})
```

The second argument is a fixed per-row height (`item_extent`) — scroll math is pure arithmetic, no off-screen measuring.

## Scaffold: the screen frame

`Scaffold` is the root widget for a screen — optional app bar, optional nav rail (sidebar), body, optional bottom bar, optional floating action button:

```rust
Scaffold::new(body)
    .app_bar(AppBar::new("Home"))
    .fab(FloatingActionButton::new().icon(Icon::new(IconKind::Add)).on_press(|| { }))
    .into_element()
```

It fills the available space, paints the theme background behind everything (including under a phone's notch/status bar), and insets the *interactive* content — app bar, body, bottom bar, FAB — by the platform's safe area automatically.

## Picking the right widget

A quick map from "I want X" to the widget that does it:

| You want | Reach for |
|---|---|
| Vertical/horizontal stack | `Column` / `Row` |
| A colored/bordered/rounded box | `Container` |
| Overlay children on top of each other | `Stack` + `Positioned` |
| Multi-column layout or masonry/dashboard tiles | `Grid` |
| A long, cheap-to-scroll list | `ListView::builder` |
| One big scrollable region | `ScrollView` |
| A full screen (bar + body + FAB) | `Scaffold` |
| Fixed gap between children | `Spacer` |
| Fill leftover flex space | `Expanded` |

This is a small slice of the widget set — `rosace_widgets::prelude` exports the full list (buttons, form controls, dialogs, menus, tables, and more), most of which are covered as they come up in later chapters.

---

**Under the hood:** the Widget protocol — how `children()`/`layout()`/`paint()` default and compose — is in [Widget Protocol](../architecture/widget-protocol.md).

Next: [Interaction](interaction.md).
