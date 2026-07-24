# The ROSACE Guide

> A textbook for building apps with ROSACE. Concepts, paired with real code you can run.

ROSACE is a Rust framework for building native-feeling apps — desktop, web, iOS, and Android — from **one** codebase. You describe your UI declaratively, wire it to reactive state, and the framework handles rendering, layout, input, and platform differences.

This guide teaches you to *build with* ROSACE. If you want to understand how the framework works *inside* (to contribute to it), read the [Architecture](../architecture/README.md) book instead.

## What you'll learn

- Write your first app and run it
- **Components and state** — the two ideas the whole framework is built on
- Lay out UI with the widget set
- Handle interaction (buttons, gestures, forms, text input)
- Navigate between screens
- Theme your app (and adapt per platform automatically)
- Animate, persist data, talk to the network
- Ship to desktop, web, and mobile with the `rsc` CLI
- Use hot reload for a fast dev loop

## How to read this

Every chapter pairs a concept with runnable code. Start at the top — [Getting Started](getting-started.md) — and go in order the first time; later, jump around. Each chapter ends with a pointer into the [Architecture](../architecture/README.md) book if you want to know *why* it works that way.

See [SUMMARY.md](SUMMARY.md) for the full table of contents.

## The 30-second version

```rust
use rosace::prelude::*;

struct Hello;

impl Component for Hello {
    fn build(&self, _ctx: &mut Context) -> Element {
        Scaffold::new(
            Column::new()
                .padding(EdgeInsets::all(24.0))
                .child(Text::new("Hello, ROSACE")),
        )
        .into_element()
    }
}

fn main() {
    App::new().title("Hello").size(400, 300).launch(Hello);
}
```

That's a complete app. The next chapters unpack every line.
