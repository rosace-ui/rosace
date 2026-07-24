# Interaction

ROSACE has one consistent vocabulary for "something happened to this widget": `.on_press(...)`, `.on_change(...)`, `.on_long_press(...)`. This chapter covers how that plumbing works, the built-in interactive widgets, and how to make an arbitrary widget clickable.

## The `.on_press` / `.on_change` vocabulary

Interactive widgets never use `.on_click`/`.on_tap` — the project standardizes on `.on_press` for "this was activated" and `.on_change` for "this control's value changed," across every widget that has one. You've already seen the pattern from [Components & State](components-and-state.md):

```rust
Button::new("Save").on_press(|| save())
Checkbox::new(checked).on_change(|new_value| checked_atom.set(new_value))
Slider::new(0.5).on_change(|v| volume.set(v))
```

Each callback is a plain closure — `Fn() + Send + Sync + 'static` for `on_press`, `Fn(T) + Send + Sync + 'static` for `on_change` — so the usual move-a-clone-of-the-atom-into-the-closure pattern from the counter example applies everywhere.

## Making any widget clickable: `PressApi`

Every widget gets `.on_press(f)` and `.on_long_press(f)` for free via the blanket `PressApi` trait — not just `Button`:

```rust
Text::new("tap me").on_press(|| do_thing())
Card::new(content).on_press(|| open_details())
```

This wraps the widget in a `Pressable`/`LongPressable` that registers a hit region over the whole child rect. Widgets that already have their own `.on_press` builder (`Button`, `ListTile`) keep their inherent method — it wins over the blanket one, and does the same thing (register a hit region), just with widget-specific visuals (hover/press feedback) layered on top.

Long-press fires after roughly 500ms of holding without moving.

## Built-in interactive widgets

The common controls all follow the same shape — construct with the current value, attach `.on_change`:

```rust
Checkbox::new(is_checked).label("Notify me").on_change(|v| is_checked_atom.set(v))
Switch::new(is_on).on_change(|v| is_on_atom.set(v))
Radio::new(selected == Choice::A).on_select(|| selected_atom.set(Choice::A))
Slider::new(value).range(0.0, 100.0, value).on_change(|v| value_atom.set(v))
Dropdown::new(options, selected_index, dropdown_open_atom).on_change(|i| selected_atom.set(i))
```

These are all **controlled** components: you own the value in an `Atom`, the widget reports changes, you decide what happens (usually just writing the atom back). Nothing renders itself out of sync with your state. `Radio` uses `.on_select(f)` (no value — it always means "I was picked"); pairing several `Radio`s against one `Atom<T>` for mutual exclusion is the app's job, not built into the widget.

A `Button` (or `Slider`, or anything else with a hit region) that has no callback wired still absorbs its clicks rather than letting them fall through to whatever's behind it — a click on an unwired button never accidentally triggers a drag-to-pan region underneath it.

## Hover and press state

Widgets that want to paint hover/press feedback read it straight from `PaintCtx` during `paint`:

```rust
ctx.hovered()   // true while the pointer is over this widget's region
ctx.pressed()   // true from mouse-down until mouse-up while this is the pressed target
```

Pair `ctx.pressed()`/`ctx.hovered()` with `ctx.animate_to(target, duration_ms)` for eased visual feedback — this is exactly how `Button` gets its lift-on-hover, deeper-lift-on-press fill (see `rosace-widgets/src/tree/button.rs` if you're building a custom pressable and want the same feel).

## Positional interaction: dragging, sliders, canvases

Some widgets need to know *where* on their rect a press landed, not just that one happened — `Slider` is the example: clicking anywhere on the track should jump the value to that position. That's `ctx.on_press_at(f)`, where `f: Fn(f32, f32)` receives the press point in window-space logical pixels:

```rust
let r = ctx.rect;
ctx.on_press_at(move |px, _py| {
    let t = ((px - r.origin.x) / r.size.width).clamp(0.0, 1.0);
    on_change(min + t * (max - min));
});
```

For scroll-wheel/trackpad input on a custom widget, `ctx.on_scroll(f)` (`f: Fn(f32, f32)` for `(delta_x, delta_y)`) does the equivalent for wheel events. Scrollable containers (`ScrollView`, `ListView`) already wire this up for you — reach for these low-level hooks only when building a new interactive primitive (a custom slider, a canvas, a 2D pan/zoom surface like `InteractiveViewer`).

## Non-interactive and pointer-blocking regions

Two widgets adjust how a subtree participates in hit-testing without changing what it looks like:

- `IgnorePointer` — the subtree becomes click-through; taps pass to whatever's behind it.
- `AbsorbPointer` — the subtree still blocks clicks from reaching what's behind it, but doesn't handle them itself (useful for a disabled overlay).

## Gestures: taps, drags, swipes, pinch

`rosace_gesture` (re-exported as `rosace::gesture`) defines a lower-level `GestureEvent` model — `Tap`, `DoubleTap`, `LongPress`, `Swipe { direction, velocity }`, `Drag { dx, dy, phase }`, `Pinch { scale, center }` — and a `GestureRecognizer` trait (`TapRecognizer`, `DragRecognizer`, `SwipeRecognizer`, `PinchRecognizer`) that turns raw platform `InputEvent`s into these. This is the toolkit widgets like `Slider`'s drag handling and `InteractiveViewer`'s pan/zoom are conceptually built from.

For everyday app code, prefer the widget-level API above (`PressApi`, `.on_change`, `ctx.on_press_at`/`ctx.on_scroll`) — it's what the built-in widget set actually uses, and it's already wired into hit-testing, clipping, and z-order via the render tree. Reach for a raw `GestureRecognizer` only if you're building a new low-level input primitive that needs swipe/pinch detection from scratch.

## Forms

Multi-field forms with validation are covered in their own chapter — see [Forms & Text Input](forms-and-text.md) for `Form`, `FormField`, and the built-in `Validator`s (`Required`, `Email`, `MinLength`, etc.).

---

**Under the hood:** how hit regions, hover, and z-order are tracked on the render tree is in [Widget Protocol](../architecture/widget-protocol.md).

Next: [Navigation](navigation.md).
