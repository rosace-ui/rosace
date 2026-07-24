# Hot Reload

`rsc dev` is ROSACE's fast edit-save-see loop. This chapter explains what actually happens when you save a file, what state survives, and where the current limits are — so you're never surprised by a reload behaving differently than you expected.

## Starting it

```bash
rsc dev                  # desktop, hot reload on by default
rsc dev --target web      # web dev server with hot reload
rsc dev --target android  # push edits to a device/emulator you already `rsc run`
rsc dev --target ios
```

On desktop, plain `rsc dev` gives you the fastest path available for your app (explained below). Add `--watch` to switch to a supervised hot-*restart* loop instead — useful as a fallback, or while your app isn't wired for the fastest path yet:

```bash
rsc dev --watch                 # desktop: rebuild + relaunch on every change
rsc dev --debounce 500          # coalesce a burst of saves into one rebuild (default 300ms)
```

## Two mechanisms, layered

ROSACE's hot reload is two different techniques stacked on top of each other, plus a restart floor. They cover different kinds of edits.

### Tier 1 — data reload (works everywhere)

Your UI markup is split into a **static template** (which widgets, how they nest, literal text/attribute values) and **dynamic slots** (the `{expr}` holes wired to real Rust). When you edit a `view!` — reorder children, change a literal string, tweak a spacing number — `rsc dev`'s file watcher re-parses just that call, pushes the new template into the running app's live registry, and forces a repaint. No recompile.

This is universal: it's the same mechanism on desktop (a filesystem watcher thread), web (the dev server pushes the edited source over a WebSocket to the browser), and Android/iOS (the edited source is pushed over a socket to the device/simulator, forwarded via `adb`/`devicectl`). It's also how asset edits work — change an image or font file under `assets/` and it hot-swaps by invalidating the decode cache and repainting, no rebuild at all.

Tier 1 does **not** cover new handler logic, a new hook, or anything that isn't expressible as template data — those need compiled code.

### Tier 2 — full code hot-swap (desktop, opt-in)

On desktop, `rsc dev` (without `--watch`) defaults to swapping *compiled code* live — `build()` bodies, handler logic, new hooks, all of it — with no restart and no lost state. This works by splitting your app into a stable **host** process (the window, renderer, and — critically — the shared state store) and a reloadable UI **module** compiled as a `dylib`. Both link the same `rosace` shared library, so there's exactly one copy of the atom store; when the module is swapped, the host rebuilds the tree against the new code and the atoms it reads are the same ones that were already live.

To opt in, your app's `src/lib.rs` needs to export the Tier-2 entry point:

```rust
// src/lib.rs
use rosace::prelude::*;

struct AppRoot;

impl Component for AppRoot {
    fn build(&self, ctx: &mut Context) -> Element {
        // ...
        # unimplemented!()
    }
}

#[no_mangle]
pub extern "C" fn __rsc_dev_root() -> Box<dyn rosace::Component> {
    Box::new(AppRoot)
}
```

If `rsc dev` doesn't find `__rsc_dev_root` in `src/lib.rs`, it tells you and falls back to Tier 1 (`view!`-only reload via plain `cargo run`) automatically — you always get *some* form of hot reload, never a hard error.

Once wired, saving any source file rebuilds only the small module dylib (not the whole app) and the host swaps it in — usually well under a second. A rebuild that fails leaves your currently-running app untouched (you don't lose a live session to a typo); the terminal prints the error and keeps watching.

### Tier 0 — hot restart (the floor, always available)

`rsc dev --watch` supervises your app as a child process: it builds once up front, launches it, then on every source change rebuilds and — only on a *successful* build — kills and relaunches the process. A failed build leaves the old process running.

This is a full process restart, so in-memory (`ctx.state`) atoms reset to their defaults. `ctx.state_permanent` values do **not** reset — they're already being written to the on-disk store on every change (see [Persistence & Networking](persistence-networking.md)), so the relaunched process reads them straight back.

## What triggers a reload

Any `.rs` file under `src/` (or your project root if there's no `src/`), plus asset files (`png`, `jpg`, `svg`, `ttf`, `json`, etc.) for Tier 1's asset hot-swap. Edits are debounced — a burst of saves within the debounce window (default 300ms, `--debounce` to change it) collapses into one rebuild, and any edits that land *during* a rebuild are queued for exactly one follow-up rebuild afterward. You never get a backlog of stacked rebuilds for a string of quick saves.

## Platform differences, honestly

| Target | Structure/style edits (`view!`) | Logic edits (handlers, new hooks) |
|---|---|---|
| Desktop, Tier-2-ready app | instant, no restart | instant, no restart (full code swap) |
| Desktop, not Tier-2-ready | instant, no restart | requires a manual restart (falls back to `cargo run`) |
| Desktop, `--watch` | instant restart, state resets except `state_permanent` | same |
| Web | instant, pushed over WebSocket, no restart | **manual**: `rsc dev --target web` doesn't push a compiled-logic reload — rebuild and refresh the page yourself. wasm has no dynamic-linking story, so there's no swap target for Tier 2 there yet. |
| Android / iOS (device or simulator) | instant, pushed over a socket | rebuild + redeploy via `rsc run --target android/ios` again |

The honest summary: **Tier 1 (markup/style) hot-swaps live on every platform, including web.** Tier 2 (arbitrary logic) hot-swaps live only on desktop today, and only once your app exports `__rsc_dev_root`. Everywhere else, a logic change is a rebuild-and-relaunch — with persistent state intact, but in-memory state lost.

## Practical tips

- Keep anything you don't want reset by a Tier-0 restart in `ctx.state_permanent`, not `ctx.state` — a login token or an in-progress form draft you don't want to lose to a restart-triggering edit.
- If Tier 2 isn't kicking in on desktop, check that `__rsc_dev_root` is exported from `src/lib.rs` exactly as shown above — `rsc dev` prints the reason it fell back.
- A broken build never kills your running app on either `--watch` or the default desktop path — fix the error and save again; the terminal keeps watching.

---

You've now covered the whole guide — see the [Architecture book](../architecture/README.md) if you want to know how any of this works internally.
