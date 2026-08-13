<div align="center">
  <img src="https://raw.githubusercontent.com/rosace-ui/rosace/main/assets/rosace/rosace-logo-aurora.svg" alt="Rosace" width="420"/>

  <p><em>Fast by nature. Beautiful by design.</em></p>

  <p><strong>The UI framework Rust deserved from day one.</strong></p>

  ![Status](https://img.shields.io/badge/status-active-brightgreen)
  ![Version](https://img.shields.io/badge/crates.io-0.1.0-orange)
  ![Rendering](https://img.shields.io/badge/rendering-GPU%2FCPU%20hybrid%20(wgpu)-8A2BE2)
  ![Performance](https://img.shields.io/badge/target-120fps-ff69b4)
  ![Platforms](https://img.shields.io/badge/platforms-desktop%20·%20web%20·%20iOS%20·%20Android-blue)
  ![Language](https://img.shields.io/badge/language-Rust-orange?logo=rust)
  ![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)

</div>

---

**Rosace is a declarative, cross-platform GUI framework for Rust.** Write your
interface once and ship it as a native app on **macOS, Windows, Linux, the web
(WebAssembly), iOS and Android** — with GPU rendering through
[wgpu](https://wgpu.rs), fine-grained reactive state, a real text stack (input,
selection, OS IME), platform accessibility (VoiceOver, TalkBack, AccessKit),
and hot reload. Pure Rust, clean-room, no Electron and no web view.

---

> **0.1.0 — first release, published on crates.io.** Rosace is still under active development: APIs are unstable and large parts are still being built. Not production-ready yet — but what's here already runs fast, and it's now installable, not just clonable.

---

## ✨ Highlights

One Rust codebase → native-feeling apps on **desktop, web, iOS, and Android**. Everything below is built clean-room, in pure Rust, designed to compose:

| | |
|---|---|
| 🎨 **Declarative GPU shaders** | Shapes, glassmorphism, backdrop blur and custom effects are **SDF shader pipelines on the GPU** — declared like any other widget. No manual draw calls, no `unsafe`. Shape/effect rendering moved off CPU rasterization onto wgpu; general canvas drawing (text, paths) is still a CPU (tiny-skia) rasterizer composited onto the GPU layer — a hybrid pipeline, not fully GPU-native yet. |
| 🌗 **Dynamic theming** | One deliberate design system (Material 3), an exhaustive compile-checked token system, and **runtime theme switching** — change one atom and every subscribed widget repaints. Platform-adaptive where it matters, and third-party skins plug into the same bundle. |
| 🔥 **Hot reload + in-app DevTools** | A three-tier hot-reload engine (live data-swap → dylib code-swap → hot-restart) picks the fastest path per platform. A built-in **flight recorder** and trace bus record every frame, state change, and event for live debugging. |
| ⚡ **Fine-grained reactivity** | `Atom<T>` state with **subscriber-precise rebuilds** — no virtual-DOM diff, no re-render-the-world. Change state, and only the components that read it repaint. |
| ⌨️ **Real text stack** | `TextInput`/`TextArea` with true keyboard editing, selection, **OS IME** (CJK composition), context menus, and `rosace-forms` validation — not a painted-on illusion. |
| ♿ **Accessible & discoverable** | One semantic tree feeds **platform accessibility** (VoiceOver/TalkBack/ARIA) *and* a **server-side HTML shadow for web SEO** — build it once, get both. |
| 🤖 **Built for the AI-assisted era** | `rsc new` doesn't just scaffold code — it scaffolds *context*. Every new project ships an `AGENTS.md` (the framework's core patterns, widget catalog, and honest capability notes) and a `CLI.md` (the `rsc` commands that project actually uses), so the AI coding assistant sitting in your editor already knows Rosace instead of guessing at it from training data. Not a chatbot bolted on top — the docs your tools read are first-class scaffold output, generated fresh with every app. |

---

## What is Rosace?

Rosace is a declarative, reactive UI framework built in **pure Rust — from the ground up, without compromise.**

The name comes from *tessera* — the individual tiles of a mosaic. Every component is a tile: self-contained, composable, pixel-precise. Assembled, they form the complete picture of your app.

**The problem it solves.** Building a genuinely native, high-performance UI for every platform today means either shipping a browser (Electron — heavy, slow to start, memory-hungry), maintaining a separate native codebase per OS, or wrestling a Rust ecosystem that's mostly ports and wrappers of ideas from other languages. Rosace's answer: **one Rust codebase, real native windows and native mobile hosts, GPU-native rendering at 120fps, and memory-safety with no garbage collector** — the reach of "write once" without the bloat of a web runtime or the danger of `unsafe`.

Rust's type system isn't a restriction here — it's a design partner. Null-pointer exceptions don't exist. Layout panics don't exist. **If it compiles, it runs.**

---

## Quick look

A complete counter app — a component, reactive state, and a window:

```rust
use rosace::prelude::*;

struct Counter;

impl Component for Counter {
    fn build(&self, ctx: &mut Context) -> Element {
        // `ctx.state` gives you a reactive Atom; reading it subscribes this
        // component, so `set`/`update` repaint exactly this widget — nothing else.
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

That's the whole model: **components read state and describe UI; the framework repaints only what changed.** Everything else — layout, theming, input, platform differences — composes on top.

---

## 📚 Documentation

Full docs live on the **[Rosace Wiki](https://github.com/rosace-ui/rosace/wiki)** — two cross-linked books plus a glossary:

- **[Guide](https://github.com/rosace-ui/rosace/wiki/Guide-Home)** — for app developers. Start here to *build with* Rosace: components, state, layout, theming, navigation, animation, hot reload.
- **[Architecture](https://github.com/rosace-ui/rosace/wiki/Architecture-Home)** — for contributors. How it works *inside*: the frame loop, reactive substrate, render pipeline, widget protocol, platform layer — every claim linked to real source.
- **[Glossary](https://github.com/rosace-ui/rosace/wiki/Glossary)** — every Rosace term plus a from-scratch graphics/GPU primer (UV mapping, SDF, LRU, gamma…), each cross-linked and with authority links to code and Wikipedia.

The editable source lives in [`docs/`](docs/); the wiki is generated from it via [`scripts/docs_to_wiki.py`](scripts/docs_to_wiki.py).

---

## Why Rosace?

Most UI frameworks in the Rust ecosystem are ports, wrappers, or direct translations of ideas from other languages. Rosace is none of those things. It was designed to answer a single question: *what would a UI framework look like if it were built by someone who already knew all the mistakes?*

The answer is a framework that:

- **Never sacrifices performance for convenience** — dirty-region GPU compositing at 120fps by default
- **Never hides cost** — every allocation, draw call, and state update is explicit and traceable
- **Never lies about safety** — lifecycle correctness is enforced at compile time
- **Never forgets developer experience** — the `rsc` CLI, hot reload, and the built-in `RosaceTrace` event bus exist because debugging UI should not be miserable
- **Composes all the way down** — from the layout engine to state atoms to the render layer, every abstraction is composable, not opaque

This isn't a prototype. It's a foundation — and it's being built to last.

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                      your app                        │
├─────────────────────────────────────────────────────┤
│   rosace-widgets   │   rosace-cli (rsc)              │  Widgets + CLI
├─────────────────────────────────────────────────────┤
│   rosace-platform  │   rosace-layout                 │  Windowing + Flexure
├─────────────────────────────────────────────────────┤
│   rosace-render    │   rosace-state                  │  Pipeline + Atoms
├─────────────────────────────────────────────────────┤
│   rosace-core      │   rosace-trace                  │  Components + Bus
├─────────────────────────────────────────────────────┤
│              rosace-macros · rosace-compositor        │  Macros + GPU
└─────────────────────────────────────────────────────┘
        wgpu · tiny-skia · fontdue · winit
```

Data flows downward (props). State changes propagate through reactive atoms. The render layer only repaints what changed. The trace bus records everything. See the **[Architecture book](https://github.com/rosace-ui/rosace/wiki/Architecture-Home)** for the full picture.

---

## Getting Started

> Early development — these steps work today but will evolve as the framework stabilises.

**Prerequisites:** Rust 1.78+ (stable), `cargo` in your PATH — install both via [rustup.rs](https://rustup.rs) if you don't have them yet.

```bash
git clone https://github.com/rosace-ui/rosace.git
cd rosace
cargo build
```

### rsc CLI

```bash
cargo install rosace-cli            # install the developer CLI from crates.io

rsc new my-app                      # scaffold a new Rosace project
rsc dev                             # dev loop with hot reload
rsc run                             # build + run once
rsc run --ios                       # build + run on an iOS simulator
rsc analyze                         # static analysis of your component tree
rsc snapshot --package <pkg> --example <name>   # golden snapshot test
```

`rsc new` drops an `AGENTS.md` and `CLI.md` straight into the new project —
your editor's AI assistant reads the framework's own primer instead of
hallucinating an API. Every app, every time, no flag required.

Building from a local checkout instead of crates.io (e.g. to track `main`)
still works — `cargo install --path rosace-cli` in place of the line above.

### Writing custom widgets

Most app code only composes built-in widgets (`Column`, `Button`, `ScrollView`, …) inside a `Component` — no need to read further. If you're building a genuinely new visual primitive, see [`.steering/WIDGET_AUTHORING_GUIDE.md`](.steering/WIDGET_AUTHORING_GUIDE.md) for the `Widget` trait and the leaf / single-child / multi-child decision table, and the **[Widget Protocol](https://github.com/rosace-ui/rosace/wiki/Architecture-Widget-Protocol)** chapter for how it all fits together.

---

## Gallery

The framework supports a rich set of UI components and effects:

### Glassmorphism & modern effects
![Liquid Glass Example](https://raw.githubusercontent.com/rosace-ui/rosace/main/assets/examples/liquid_glass.png)

### Widget gallery
A showcase of built-in widgets in action:

![Widget Gallery 1](https://raw.githubusercontent.com/rosace-ui/rosace/main/assets/examples/widget_gallery_1.png)
![Widget Gallery 2](https://raw.githubusercontent.com/rosace-ui/rosace/main/assets/examples/widget_gallery_2.png)
![Widget Gallery 3](https://raw.githubusercontent.com/rosace-ui/rosace/main/assets/examples/widget_gallery_3.png)

---

## Crate Overview

| Crate | Description |
|---|---|
| **Core** | |
| `rosace` | Main entry point, app launcher — the single dependency most apps need |
| `rosace-macros` | Proc-macros: `#[component]`, `view!{}` |
| `rosace-view-syntax` | The `view!` grammar, shared by the macro and the hot-reload runtime |
| `rosace-core` | Component model, element tree, lifecycle hooks, **a11y**, **i18n** |
| **State & Reactivity** | |
| `rosace-state` | `Atom<T>`, `use_atom`, `GlobalAtom`, subscriber-precise dirty tracking |
| **Layout & Rendering** | |
| `rosace-layout` | Flex layout: Column, Row, Stack, Grid, Wrap |
| `rosace-render` | GPU/CPU hybrid renderer, glyph atlas, dirty-region tracking |
| `rosace-compositor` | wgpu compositor — the only crate that touches wgpu types |
| `rosace-shader` | Shader pipeline types + registration queue (zero wgpu dependency) |
| **Text & Input** | |
| `rosace-text` | TextInput, TextArea, clipboard, OS IME, **shaping**, **bidi** |
| **Widgets** | |
| `rosace-widgets` | 75+ widgets: Button, Card, Dialog, ListView, … plus **forms**, **scroll** |
| **Animation & Navigation** | |
| `rosace-animate` | Tween, Timeline, easing, springs |
| `rosace-nav` | Navigator, Router, route stack, guards, **route transitions** |
| **Styling & Theme** | |
| `rosace-theme` | Material 3, design tokens, the platform `Themes` bundle |
| `rosace-style` | Style primitives |
| **Platform & FFI** | |
| `rosace-platform` | Windowing (winit), platform events, **gestures**, **web SEO**, AccessKit |
| `rosace-ffi` | Native mobile-host FFI bridge (real iOS/Android hosts) |
| **DevTools & Debugging** | |
| `rosace-trace` | Event bus, ring buffer, flight recorder, logging |
| `rosace-devtools` | In-app DevTools overlay |
| `rosace-hot-reload` | File-watching + rebuild primitive |
| **Persistence & Networking** | |
| `rosace-storage` | SQLite-backed persistence (`state_permanent`), **file access** |
| `rosace-net` | HTTP + **WebSocket** client, `use_query`, `use_websocket` |
| `rosace-media` | Image/video, camera access |
| **CLI & Tooling** | |
| `rosace-cli` | `rsc`: new, dev, run, build, package, analyze, snapshot |
| `rosace-asset-codegen` | Build-time typed asset codegen |
| `rosace-test-utils` | Headless widget/render test harness |

> **26 crates, down from 39** (D131). Thirteen small crates were folded into
> the ones they were always used with — the **bold** entries above are those
> merged modules, still importable at the same paths through `rosace`. If you
> remember `rosace-forms`, `rosace-a11y`, `rosace-ws` or `rosace-web-seo` as
> separate dependencies: `forms → widgets::forms`, `a11y → core::a11y`,
> `ws → net::ws`, `web-seo → platform::web_seo`. The public API did not change.

---

## Development Phases

**Landed:** reactive state · Flexure layout · GPU-native render (wgpu SDF shapes + glyph atlas) · animation system · 75+ widgets · Material 3 theming · desktop platforms · web (WASM + SEO) · real iOS/Android native hosts · platform accessibility (VoiceOver/TalkBack/AccessKit) · text stack (TextInput/IME/forms) · app lifecycle · three-tier hot reload · event tracing & flight recorder.

**In progress / next:** widget quality sweep (a systematic audit against a
published [quality bar](.steering/WIDGET_QUALITY_BAR.md) — theme tokens, OS
text scaling, 44px touch targets, semantics) · accessibility **actions**
(roles and labels ship today; activating a control from a screen reader does
not yet) · networking hooks live-verification · persistence tiers (encrypted
Keychain/Keystore) · declarative shader materials · mobile UI polish · web GPU
presenter.

### Multi-platform status
- ✅ Desktop (macOS, Windows, Linux)
- ✅ Web (WASM + semantic SEO tree)
- 🧪 iOS (real native host + Xcode build working; UI polish ongoing)
- 🧪 Android (real native host + Gradle/APK working; UI polish ongoing)

---

## A Note on How This Was Built

> *Coded with AI. Architected by Human.*

Rosace is built with the assistance of AI — and I say that openly, without apology.

Every line was generated with AI assistance, and every single line was read, understood, validated, and approved by me before it landed. The architecture decisions, the crate boundaries, the API shapes, the performance constraints, the trade-offs — those are mine. The AI is a tool. A fast one. But the judgement behind this codebase is human.

This is not a framework vomited out of a prompt. It is designed with intent, built with discipline, and reviewed with care.

---

## Author

<table>
  <tr>
    <td align="center" width="140">
      <a href="https://github.com/godwinjk">
        <img src="https://github.com/godwinjk.png" width="110" height="110" style="border-radius:50%" alt="Godwin Joseph"/>
      </a>
    </td>
    <td>
      <h3>Godwin Joseph</h3>
      <p>Creator &amp; architect of Rosace. Building the UI framework Rust deserved.</p>
      <p>
        <a href="https://github.com/godwinjk">GitHub</a> ·
        <a href="https://rosace.godwinj.com/">rosace.godwinj.com</a>
      </p>
    </td>
  </tr>
</table>

---

## Contributing

Rosace is not yet open for general contributions while the foundation is being laid. That said:

- **Bug reports** — open an issue with steps to reproduce
- **Feature requests & ideas** — open a discussion issue before building anything
- **Pull requests** — please open an issue first so we can align on scope; keep PRs small and focused

Architectural decisions that govern the project are recorded in [`.steering/DECISIONS.md`](.steering/DECISIONS.md). Read it before opening a PR — decisions marked `LOCKED` are not open for debate unless a new decision supersedes them.

---

## License

Copyright (c) 2026 Godwin Joseph.

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Rosace by you shall be dual licensed as above, without any additional terms or conditions.

---

<div align="center">
  <sub>Built in Rust 🦀 Designed with intent 🦀 Reviewed by hand</sub>
</div>
