# TEZZERA — Hot Reload & `tzr` Build Architecture (Plan)

> Status: PLAN (not yet built). Decision recorded as **D102**.
> Feasibility: native module hot-swap = proven pattern (hot-lib-reloader,
> Bevy dynamic_linking, Dioxus desktop). Web = "hot restart with state
> preservation" (wasm has no stable dynamic linking). Hot-restart fallback
> always available.

## Problem

`tzr dev` today only runs `cargo run` (desktop) or `cargo build --target wasm32`
+ serves `dist/` (web). The file watcher (`tezzera-hot-reload`) merely re-runs
`cargo build` and prints a message — **the running process is never updated**.
The whole app (the user's `Component::build()`) is statically linked into one
monolithic runtime binary, so there is no seam to swap. Hot reload does not
work because there is nothing to swap into.

## Goal

Edit a `build()` / style / handler → see it live in < ~1s without losing app
state, on **desktop and web**, with an automatic **hot restart** when a swap
fails or the change exceeds hot-swap limits (D041).

## Two hot-reload mechanisms, layered (READ THIS FIRST)

There are two independent ways to swap live UI. They stack:

- **Tier 1 — Template / data hot-reload (UNIVERSAL, the foundation).** Ships
  *data, not code*, so it works on EVERY target — desktop, Android, iOS device,
  iOS simulator, web — because it never loads new machine code (bypasses the
  iOS `dlopen` ban and wasm's lack of dynamic linking). This is the Dioxus
  template model: a `view!` macro splits UI into a **static template** (which
  widgets, nesting, literal text/attrs) + **dynamic slots** (the `{expr}` holes
  that reference compiled Rust). A dev watcher re-parses the changed `view!`,
  rebuilds the template (pure data), pushes it to the running app, which
  **interprets** it and swaps the subtree — no recompile. Covers structure
  edits, reorder/wrap, literal text/style/attribute changes, moving a dynamic
  value. Does NOT cover handler *bodies*, new hooks, or new dynamic
  expressions (those are new code). This is ~70–80% of everyday edits,
  instant, on all platforms. **Requires a declarative `view!` layer (see D103).**
- **Tier 2 — Native module dylib swap (ACCELERATOR, capable platforms only).**
  Reloads *logic* too by swapping compiled `.so`/`.dylib`. Works on desktop,
  Android (dev), iOS simulator — NOT iOS device, NOT web. The host/module
  architecture below.
- **Tier 0 (floor) — Hot restart with state preservation.** Always available
  everywhere. When a change exceeds Tier 1 (logic) on a platform without
  Tier 2 (iOS device, web), rebuild + relaunch + rehydrate `#[persist]` atoms.

Net guarantee: **hot reload is never worthless on any platform.** Every target
gets instant Tier-1 structure/style reload; logic changes get Tier-2 where the
OS allows loading code, else a fast state-preserving Tier-0 restart.

The rest of this document details Tier 2 (the host/module architecture) and the
platform matrix; Tier 1's design is summarized under "Template hot-reload".

## Core idea: a stable host + reloadable UI modules

Split the process into two ABI domains:

```
┌─────────────────────────────────────────────────────────────┐
│  HOST  (stable runtime binary — rarely changes in a session) │
│  winit event loop · GPU/canvas renderer · reconciler/frame   │
│  loop · render tree · STATE STORE (atoms) · reload supervisor│
└───────────────▲───────────────────────────▲─────────────────┘
                │ links dynamically          │ dlopen / libloading
                │                            │
┌───────────────┴──────────┐   ┌─────────────┴────────────────┐
│ SHARED RUNTIME DYLIB      │   │ UI MODULE DYLIBS (reloadable)│
│ `tezzera` compiled ONCE   │   │ app_home.so, app_settings.so │
│ as crate-type=["dylib"].  │   │ … one per UI crate. Contain  │
│ Guarantees a SINGLE copy  │   │ Component::build(), handlers, │
│ of the state-store statics│   │ styles. Rebuilt individually.│
│ shared by host + modules. │   │ Link the shared runtime dylib│
└───────────────────────────┘   └──────────────────────────────┘
```

The host and every UI module link the **same** `tezzera` shared dylib, so
there is exactly one atom store, one render tree, one theme — owned by the
host, read/written by modules. This is what makes state survive a reload.

## The three make-or-break problems (and their fixes)

### 1. Statics duplication → single shared runtime dylib
Rust `static`s are duplicated per dynamically-linked object. Our reactive
state lives in `tezzera-state` statics/thread-locals. If host and each module
statically link `tezzera-state`, each gets its own store → state neither
survives reload nor flows between modules.
**Fix:** `tezzera` (and its deps) built once as `crate-type = ["dylib"]`; host
and all modules link *it* dynamically. One instance of every static. (Bevy's
`dynamic_linking` feature works exactly this way.)

### 2. Dangling closures on unload → strict swap ordering
`on_press` etc. are `Arc<dyn Fn>` whose vtable/code lives in the module.
Unloading the module dangles them → segfault if later invoked.
**Fix — reload protocol (ordered, never violated):**
1. `libloading::Library::new(new.so)` — load the NEW module (old still loaded).
2. Force a **global rebuild** → new module produces a fresh element tree with
   fresh closures pointing into the NEW code.
3. Swap the live tree/root to the new generation.
4. Drop the OLD element tree + its render-tree regions → all old `Arc`s freed.
5. **Only now** drop the OLD `Library` (unload). Pending input events are
   drained first so no old closure is invoked post-unload.
Each tree generation holds a refcount on the `Library` that produced it; the
`Library` unloads when its generation is fully retired.

### 3. No stable Rust ABI → narrow `extern "C"` boundary, dev-only
**Fix:** modules expose ONE versioned C entrypoint; host + modules are always
the same toolchain and share the `tezzera` dylib in dev. **Release builds do
not use any of this** — they static-link everything into a single binary
(cargo feature `tzr-hot` gates the dynamic path). The dylibs never ship.

## The reload boundary (ABI)

Each UI module exports exactly one symbol (generated by a `#[tzr::module]`
proc-macro so authors never write `unsafe`):

```rust
// generated
#[no_mangle]
pub extern "C" fn __tzr_module_v1(reg: *mut tezzera::ModuleRegistry) {
    // registers this module's root Component factory: fn() -> Box<dyn Component>
}
```

- The host resolves `__tzr_module_vN`, checks `N` against its supported ABI
  version. Mismatch → refuse swap → hot restart with a clear message (D041).
- Trait objects (`Box<dyn Component>`) are created and consumed on the shared
  `tezzera` dylib's vtables, so they are coherent across the boundary. The
  `extern "C"` entrypoint carries only an opaque `*mut ModuleRegistry`.
- `#[tzr::app]` on the host side generates the supervisor wiring + the static
  release entrypoint from the same source.

## State preservation

- **Across a module swap:** automatic. Atoms live in the host-owned shared
  store; the rebuilt module reads the same atoms. Atom identity is keyed by
  `ComponentId` (DFS position) + hook order (existing model). Stable tree
  shape → IDs line up → state preserved.
- **When the tree shape changes** (a hook added/removed): IDs shift for the
  affected subtree → **graceful reset** of just that subtree (D008: "type
  change → graceful reset, never crash"). *Optional hardening:* derive atom
  keys from source `location!()` (`#[track_caller]`) so edits elsewhere don't
  shuffle unrelated state. (Sub-decision, evaluate in Phase 4.)
- **Across a hot restart:** `#[persist(reload|session|permanent)]` atoms
  (D008) are serialized to a state blob and rehydrated by the fresh process.

## Which module rebuilt (incremental)

- UI is split into crates (`app_home`, `app_settings`, …), each
  `crate-type = ["dylib"]` under `tzr-hot`.
- `tzr dev` watches `src/`, maps a changed file → owning crate via
  `cargo metadata` (path→package), runs `cargo build -p <crate>` (cargo does
  the incremental compile), then signals the host to reload **only that**
  dylib. Other modules and the host stay live → sub-second swaps.

## Failure handling → hot restart

- The module entrypoint call and the first post-reload frame run inside
  `catch_unwind`.
- On panic / load failure / symbol-missing / ABI mismatch:
  - Log a clear message (D041: no silent failure).
  - **Revert:** keep the last-good module loaded → "reload failed, kept
    previous version" — the dev session survives a broken edit.
  - If revert is impossible (host state corrupted) → **hot restart**:
    serialize persist/session atoms → `exec` a fresh `tzr` process pointing at
    the blob → rehydrate. User sees a fast restart at ~the same state.
- **D041 "needs restart" changes** (new deps, atom *type* change, new files,
  FFI/macro changes) are detected by the CLI (cargo build-graph / metadata
  diff) → skip hot-swap, do a full rebuild + hot restart automatically.

## Web strategy (no native dylibs)

Wasm has no stable dynamic linking, so native module load/unload (Tier 2) does
not port. Web uses:
- **Tier 1 — template hot-reload (primary).** The universal path above: the
  dev server pushes new template descriptors over a WebSocket; the wasm
  interpreter swaps subtrees with no recompile. Instant for structure/style.
- **Tier 0 — rebuild + rehydrate (floor, for logic changes).** Rebuild wasm →
  WebSocket `reload` → page serializes `#[persist]` atoms to
  `sessionStorage`/`IndexedDB` (D008) → re-instantiate the new wasm → rehydrate.
- **Per-module wasm via dynamic `import()` (stretch).** Split UI into separate
  wasm modules; rebuild + re-instantiate only the changed one. A Tier-2-like
  accelerator for logic on web; depends on wasm dynamic-linking / component-
  model maturity — experimental.
- Shared with native: one state store in the wasm "host" instance; persistence
  maps to D008 levels.

## Template hot-reload (Tier 1) — design

The universal path. Three pieces:

1. **`view!` macro** (`tezzera-macros`). Authors write declarative UI:
   ```rust
   view! {
       Column(spacing: 8) {
           Text(count.to_string())            // dynamic slot 0
           Button("Increment", on_press: inc) // dynamic slot 1
       }
   }
   ```
   The macro emits BOTH: (a) normal Element-building code (release path, zero
   overhead), and (b) in dev (`tzr-hot`), a **template descriptor** — the static
   skeleton with indexed holes — registered under a source-`location!()` key.
   The builder API (`Column::new().child(..)`) stays as the low-level escape
   hatch; `view!` is the hot-reloadable surface.
2. **Runtime interpreter** (`tezzera`/`tezzera-widgets`). Rebuilds an Element
   subtree from a template descriptor using a **widget registry** (element
   name → factory + attribute setters). Dynamic slots are re-bound by INDEX to
   the closures/values the compiled Rust already produced this frame — so the
   interpreter never needs to run new logic. Runs on every platform (it is just
   data → tree), including wasm and iOS device.
3. **Dev watcher → template diff → push.** On save, re-parse the changed
   `view!` block, compute the new template descriptor, diff against the running
   one by location key, and push the delta over the control channel (socket /
   WebSocket / adb). The interpreter swaps the affected subtree; state and
   dynamic slots are untouched.

**Boundary (precise).** A template edit may add/remove/reorder/wrap static
elements, change literal text/attributes/styles, and move an existing dynamic
slot. It may NOT introduce a *new* dynamic slot, change a handler body, or add
a hook — those are new compiled code → escalate to Tier 2 (dylib swap) or
Tier 0 (restart). The watcher detects "template-only" vs "needs code" by
comparing the slot signature (count/positions of `{expr}` holes); unchanged
signature → Tier 1; changed → escalate.

**Why it's universal.** It transports a data tree, not code. iOS device: the
signed binary interprets new data (no `dlopen`). Web: the wasm interpreter
accepts new data (no dynamic linking). This is the ONLY mechanism that reloads
on every target, which is why it is Tier 1, not a stretch goal.

## Mobile (iOS / Android)

Mobile splits by whether the OS permits runtime `dlopen` of a freshly-built lib.

- **Android — full module hot-swap works in dev.** bionic supports `dlopen`;
  Rust cross-compiles to `.so` per ABI. Dev host builds the changed module
  `.so` → `adb push` into the app's **private code-cache dir** → `dlopen` +
  the same ordered swap protocol; control channel over `adb forward` (socket).
  Caveat: W^X / SELinux blocks executing code from arbitrary writable paths —
  load only from `codeCacheDir`; some OEM images are stricter.
- **iOS Simulator — works like desktop.** Simulator `dyld` does not enforce
  device code-signing, so `dlopen` of a fresh `.dylib` succeeds. Primary iOS
  fast-swap target; artifact shared via the Mac filesystem.
- **iOS real device — dylib swap is IMPOSSIBLE.** Code-signing + sandbox
  forbid `dlopen` of any dylib not signed into the bundle; `dyld` refuses it
  (no AOT equivalent of Flutter's debug-JIT). Two-tier fallback:
  1. **Markup/style → data-driven RSX hot-reload** (Tier 3 below): ship a new
     UI *description* the installed binary interprets — no new code, no
     re-sign, instant. iOS-device dev is this tier's key consumer.
  2. **Logic → full rebuild + re-sign + re-deploy + relaunch + rehydrate**
     (Tier 1 hot restart). Slow but the only legal path; `#[persist]` restores
     state.

All mobile targets need a **dev transport + control channel** (adb for
Android; `devicectl`/`ios-deploy` + socket for iOS) that desktop gets for free.
State preservation is unchanged: shared-dylib singleton where swap works;
serialize→rehydrate on the iOS-device restart path.

**Capability matrix:**

| Target        | Tier 1 template (structure/style) | Tier 2 dylib (logic) | Tier 0 floor (logic) |
|---------------|-----------------------------------|----------------------|----------------------|
| Desktop       | ✅ instant                         | ✅ dylib swap         | hot restart          |
| Android (dev) | ✅ instant                         | ✅ dylib via adb      | hot restart          |
| iOS Simulator | ✅ instant                         | ✅ dylib swap         | hot restart          |
| iOS device    | ✅ instant                         | ❌ (code signing)     | rebuild+redeploy+rehydrate |
| Web (wasm)    | ✅ instant                         | ❌ (no dyn-link)*     | rebuild+rehydrate    |

*per-module wasm is a stretch accelerator. Tier 1 (template) reloads on EVERY
target — that is the guarantee that makes hot reload universally worthwhile.

## `tzr` CLI changes

- **`tzr dev`** becomes the orchestrator/supervisor: build the host once (or
  reuse a prebuilt `tzr` runtime), build the shared runtime dylib + UI module
  dylibs, start the watcher, on change map→`cargo build -p`→signal reload over
  an IPC channel (unix socket / named pipe; WebSocket for `--target web`),
  supervise, auto hot-restart on limits/failures.
- **`tzr build` / `tzr package`**: release = static-link everything into one
  binary; no dylibs, no reload machinery (`tzr-hot` feature off). Same source,
  two link modes.
- **`tzr new`**: scaffolds a project already split (host entrypoint + ≥1 UI
  module crate) so hot reload works out of the box.
- **Macros**: `#[tzr::app]` (host) and `#[tzr::module]` (UI module) hide all
  the ABI/supervisor boilerplate; authors write ordinary Components.

## Rollout phases

Ordered so the UNIVERSAL path (Tier 1) and the always-available floor (Tier 0)
land first — hot reload is useful on every platform before any dylib work.

1. **Tier 0 floor — hot restart with state preservation.** `#[persist]`
   serialize/rehydrate (D008); `tzr dev` rebuilds + relaunches + restores on
   change. Works on all platforms immediately; the safety net for everything
   later. (Smallest, highest-certainty win.)
2. **Tier 1 — `view!` macro + template descriptor.** Declarative layer (D103);
   macro emits Element code + dev template descriptor keyed by `location!()`.
   Builder API unchanged.
3. **Tier 1 — runtime interpreter + widget registry.** Rebuild a subtree from a
   template descriptor, re-binding dynamic slots by index. Prove a template
   swap on desktop.
4. **Tier 1 — dev watcher → template diff → push.** Socket control channel;
   detect template-only vs needs-code by slot signature; push deltas. Instant
   structure/style reload on desktop + web + (via transport) mobile.
5. **Tier 2 — runtime split & shared-dylib singleton.** Make `tezzera` a dylib
   under `tzr-hot`; prove ONE state-store instance across host + one module.
   (De-risks problem #1.)
6. **Tier 2 — native single-module hot-swap.** `libloading`; the ordered swap
   protocol (problem #2); `catch_unwind`. Logic reload on desktop. (Problem #2.)
7. **Tier 2 — multi-module incremental + mobile transports.** N dylibs;
   file→crate map; adb / simulator transports.
8. **Stretch.** Per-module wasm; hardened source-location atom keys; RSX-style
   partial-logic patching.

If Tier 2 (problems #1/#2) can't be made robust, Tiers 0+1 already deliver
universal structure/style reload + state-preserving restart — a large win over
today, on every platform. Tier 2 is an accelerator, not a prerequisite.

## Risks / non-goals

- **ABI instability** → same-toolchain dev only, narrow `extern "C"`, dylibs
  never shipped. Release is monolithic.
- **Atom identity drift on structural edits** → graceful reset (D008);
  optional source-location keys.
- **Per-OS dylib quirks** → macOS two-level namespace / `dylib` install-name,
  Windows `.dll` export tables + `dllimport`, Linux `RTLD_GLOBAL`; encode as
  per-target link flags in `tzr`.
- **Non-goals:** hot-reloading the host/runtime itself, new dependencies,
  trait/type-signature changes, macro changes → restart-only (D041).
```
