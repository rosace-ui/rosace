# Getting Started

By the end of this chapter you'll have a ROSACE app running in a window.

## Prerequisites

- **Rust** (stable) — install via [rustup](https://rustup.rs).
- For web/mobile later: the `wasm32-unknown-unknown` target, Xcode (iOS), or the Android SDK/NDK. Not needed for desktop.

## The `rsc` CLI

ROSACE ships a developer CLI, `rsc`, that wraps `cargo` and adds the app-framework workflow: scaffolding, a dev loop with hot reload, and multi-platform build/run. Install it from the repo:

```bash
# from the ROSACE repo root
cargo install --path rosace-cli
rsc --help
```

`rsc` uses `cargo` under the hood for desktop; it adds the toolchains for web (`wasm`), iOS (`xcodebuild`), and Android (`gradle`) on top. Run `rsc doctor` to check your toolchain, and `rsc devices` to list simulators/devices.

## Your first app

Create a new app:

```bash
rsc new my-app
cd my-app
```

Then run it:

```bash
rsc dev          # dev mode: runs the app with hot reload on
# or
rsc run          # build + run once (release-style, no hot reload)
```

`rsc dev` builds and launches the app, watches your `src/`, and hot-reloads `view!` changes live (see the [Hot Reload](hot-reload.md) chapter). `rsc run` just builds and runs.

## The anatomy of an app

Every ROSACE app is a **Component** you hand to `App::launch`:

```rust
use rosace::prelude::*;

struct App0;

impl Component for App0 {
    fn build(&self, _ctx: &mut Context) -> Element {
        Scaffold::new(
            Column::new()
                .padding(EdgeInsets::all(24.0))
                .child(Text::new("It works!")),
        )
        .into_element()
    }
}

fn main() {
    App::new().title("My App").size(480, 320).launch(App0);
}
```

- `impl Component` with a `build` method is the whole contract — you return the UI as an `Element` tree.
- `Scaffold` gives you the standard screen frame (optional app bar, body, etc.).
- `Column` stacks children vertically; `Text` renders text; `.into_element()` turns a widget into the `Element` the framework expects.
- `App::new()...launch(...)` opens the window and starts the frame loop.

## Scaffolding a real app

The fastest way to a running app is the `rsc` CLI, which generates a
ready-to-run project and launches it:

```bash
rsc new my-app        # scaffold a new ROSACE app
cd my-app
rsc run               # build and run it on the desktop
# or: rsc dev         # run with hot reload (edit + see changes live)
```

`rsc run`/`rsc dev` also take a target (e.g. `--mac`, `--ios`, `--android`)
— see [The rsc CLI](../architecture/cli.md) for the full command surface.

Next: [Components & State](components-and-state.md) — the two ideas everything else is built on.
