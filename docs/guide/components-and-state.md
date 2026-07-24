# Components & State

These two ideas — **components** and **state** — are the whole framework. Everything else is widgets and platform glue on top. Get these and you can build anything.

## Components

A component is a struct that implements one method:

```rust
impl Component for MyScreen {
    fn build(&self, ctx: &mut Context) -> Element {
        // describe the UI here
    }
}
```

`build` returns an `Element` — a description of your UI as a tree of widgets. The framework calls `build` for you; you never call it yourself. Think of it as: *"given the current state, here's what the screen should look like."*

Components compose: a component's `build` can include child components and widgets freely.

## State: the `Atom`

A `build` that only ever returns the same thing is a static screen. To make it *react*, you read **state** through `ctx`:

```rust
let count = ctx.state(0i32);   // an Atom<i32>, starting at 0
```

`ctx.state(default)` returns an **`Atom`** — a reactive value. Atoms have three moves:

```rust
count.get();                    // read the current value
count.set(5);                   // replace it
count.update(|n| n + 1);        // derive the next from the current
```

The magic: **when you `set` or `update` an atom, every component that read it is scheduled to rebuild** — and *only* those components. You never manually refresh the UI; you change state, and the affected parts redraw.

## A counter

Putting it together — a button that increments a number on screen:

```rust
use rosace::prelude::*;

struct Counter;

impl Component for Counter {
    fn build(&self, ctx: &mut Context) -> Element {
        let count = ctx.state(0i32);

        Scaffold::new(
            Column::new()
                .padding(EdgeInsets::all(24.0))
                .spacing(12.0)
                .child(Text::new(format!("Count: {}", count.get())))
                .child(Button::new("Increment").on_press({
                    let count = count.clone();
                    move || count.update(|n| n + 1)
                })),
        )
        .into_element()
    }
}

fn main() {
    App::new().title("Counter").size(400, 300).launch(Counter);
}
```

What happens when you click:
1. `on_press` runs `count.update(|n| n + 1)`.
2. That marks `Counter` dirty and requests a frame.
3. Next frame, the framework re-runs `Counter::build` — `count.get()` is now `1` — and repaints just this component.

Note the `let count = count.clone()` before the closure: the button's handler outlives this `build` call, so it needs its own handle to the atom. Atoms are cheap to clone (they share the underlying value).

## The one rule about `ctx.state`

State is matched by **call order** within `build` — the first `ctx.state` is slot 0, the second is slot 1, and so on (if you've used React hooks, this is exactly `useState`). So:

- ✅ Call `ctx.state(...)` unconditionally, in a stable order, at the top of `build`.
- ❌ Never put `ctx.state(...)` inside an `if` or a loop — it shifts every later slot and scrambles your state.

## State that outlives the app

`ctx.state` lives for the life of the running app. For values that must survive a full quit-and-relaunch (a login token, a preference, a draft), use `ctx.state_permanent`:

```rust
let launches = ctx.state_permanent("launch_count", 0i64);
```

It behaves like a normal atom, but its value is stored on disk (keyed by the string you pass) and restored on the next launch. More in [Persistence & Networking](persistence-networking.md).

---

**Under the hood:** how "changing an atom rebuilds only its subscribers" actually works is in the architecture book — [Core: Component, Element, Context](../architecture/core.md).

Next: [Layout & Widgets](layout-and-widgets.md).
