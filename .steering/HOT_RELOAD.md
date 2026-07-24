# ROSACE — Hot Reload & `rsc` Build Architecture (Plan)

> Status: PLAN (not yet built). Decision recorded as **D102**.
> Feasibility: native module hot-swap = proven pattern (hot-lib-reloader,
> Bevy dynamic_linking, Dioxus desktop). Web = "hot restart with state
> preservation" (wasm has no stable dynamic linking). Hot-restart fallback
> always available.

## Problem

`rsc dev` today only runs `cargo run` (desktop) or `cargo build --target wasm32`
+ serves `dist/` (web). The file watcher (`rosace-hot-reload`) merely re-runs
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
│ `rosace` compiled ONCE   │   │ app_home.so, app_settings.so │
│ as crate-type=["dylib"].  │   │ … one per UI crate. Contain  │
│ Guarantees a SINGLE copy  │   │ Component::build(), handlers, │
│ of the state-store statics│   │ styles. Rebuilt individually.│
│ shared by host + modules. │   │ Link the shared runtime dylib│
└───────────────────────────┘   └──────────────────────────────┘
```

The host and every UI module link the **same** `rosace` shared dylib, so
there is exactly one atom store, one render tree, one theme — owned by the
host, read/written by modules. This is what makes state survive a reload.

## The three make-or-break problems (and their fixes)

### 1. Statics duplication → single shared runtime dylib
Rust `static`s are duplicated per dynamically-linked object. Our reactive
state lives in `rosace-state` statics/thread-locals. If host and each module
statically link `rosace-state`, each gets its own store → state neither
survives reload nor flows between modules.
**Fix:** `rosace` (and its deps) built once as `crate-type = ["dylib"]`; host
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
the same toolchain and share the `rosace` dylib in dev. **Release builds do
not use any of this** — they static-link everything into a single binary
(cargo feature `rsc-hot` gates the dynamic path). The dylibs never ship.

## The reload boundary (ABI)

Each UI module exports exactly one symbol (generated by a `#[rsc::module]`
proc-macro so authors never write `unsafe`):

```rust
// generated
#[no_mangle]
pub extern "C" fn __tzr_module_v1(reg: *mut rosace::ModuleRegistry) {
    // registers this module's root Component factory: fn() -> Box<dyn Component>
}
```

- The host resolves `__tzr_module_vN`, checks `N` against its supported ABI
  version. Mismatch → refuse swap → hot restart with a clear message (D041).
- Trait objects (`Box<dyn Component>`) are created and consumed on the shared
  `rosace` dylib's vtables, so they are coherent across the boundary. The
  `extern "C"` entrypoint carries only an opaque `*mut ModuleRegistry`.
- `#[rsc::app]` on the host side generates the supervisor wiring + the static
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
  `crate-type = ["dylib"]` under `rsc-hot`.
- `rsc dev` watches `src/`, maps a changed file → owning crate via
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
    serialize persist/session atoms → `exec` a fresh `rsc` process pointing at
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

1. **`view!` macro** (`rosace-macros`). Authors write declarative UI:
   ```rust
   view! {
       Column(spacing: 8) {
           Text(count.to_string())            // dynamic slot 0
           Button("Increment", on_press: inc) // dynamic slot 1
       }
   }
   ```
   The macro emits BOTH: (a) normal Element-building code (release path, zero
   overhead), and (b) in dev (`rsc-hot`), a **template descriptor** — the static
   skeleton with indexed holes — registered under a source-`location!()` key.
   The builder API (`Column::new().child(..)`) stays as the low-level escape
   hatch; `view!` is the hot-reloadable surface.
2. **Runtime interpreter** (`rosace`/`rosace-widgets`). Rebuilds an Element
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

## Tier 1 mechanics clarified (2026-07-19) — the "inflater, not a renderer" model

The single most important thing to internalize, because it's the recurring
confusion: **the interpreter does NOT paint, and there is NO second set of
painting/widget logic.** The interpreter is a data→widget **inflater** —
exactly like Android's XML `LayoutInflater` (`setContentView(R.layout.x)`
calls `new ScrollView()`/`new LinearLayout()` — the same View classes you'd
write in code) or a browser turning HTML into DOM objects. It reads template
data and *calls the same widget constructors* (`Column::new()`,
`Button::new(..)`, `ScrollView::new(..)`). The resulting `Box<dyn Widget>`
tree is byte-for-byte what the release builder produces, and the engine's
normal `layout()`/`paint()` run on it unchanged. **Painting is the widgets we
already have; hot reload only changes how the widget tree is *constructed*.**

### You do NOT turn code into data — only the SHAPE
Every `view!` splits into two halves:
- **Shape** (which widgets, nesting, literal props) → **data** (the template).
  Hot-swappable.
- **Logic** (the `{expr}` bits — `count.to_string()`, the `on_press` closure)
  → stays **compiled machine code on the device**, never travels. It fills
  numbered **holes**.

### Worked example (Button + ScrollView)
Source:
```rust
view! {
    ScrollView {
        Column(spacing: 12) {
            Button("Save",   on_press: on_save)
            Button("Cancel", on_press: on_cancel)
        }
    }
}
```
Template (dev data): `ScrollView { Column spacing=12 { Button label="Save"
on_press=HOLE#0 ; Button label="Cancel" on_press=HOLE#1 } }`. Holes filled
each frame by compiled code: `[on_save, on_cancel]`. The interpreter:
```
build(node, holes):
  w = registry.construct(node.name)      // "ScrollView" -> ScrollView::new()
  for (key,val) in node.props:
     v = Static(v) | holes[i] for Hole(i)
     registry.set_prop(w, key, v)        // "spacing" -> .spacing(12); on_press -> .on_press(cb)
  for child: registry.add_child(w, build(child, holes))
  return w  // Box<dyn Widget> — same type/tree as release
```
Edit spacing/wrap/literal-text → re-parse → new template (same holes) → push
over wire → registry swaps → next frame same compiled holes drop into new
shape → engine paints. Add a NEW `{expr}` → new HOLE#2 the compiled code
doesn't fill → escalate (Tier 2/0). State (atoms) untouched throughout.

### The two-mode macro (dev vs release)
Gated on the `rsc-hot` cargo feature (on for `rsc dev`, off for `rsc build`).
- **Release**: `view!` emits ONLY direct builder calls
  (`ScrollView::new(Column::new().spacing(12).child(Button::new("Save")
  .on_press(on_save))…)`) — identical to hand-written, inlined, zero template
  machinery in the binary. This is today's architecture unchanged.
- **Dev**: `view!` emits (a) a registered template descriptor keyed by
  `location!()`, and (b) the hole-filler array from the compiled `{expr}`s,
  built through the interpreter so the shape can be hot-swapped.

### The ONE genuinely new runtime piece: the widget registry
Today a widget can only be built by compiled Rust calling its constructor
directly. The interpreter needs to reach the same constructors *from a string
name in data* → a **registry**: `"Column"` → factory, `"spacing"` → setter,
per widget/prop. This is construction plumbing, NOT a rendering rewrite. It's
the sleeper cost of Tier 1 (every built-in widget must register its
name/factory/setters), and third-party widgets register the same way (mirrors
D115 icon-registry / D124 material-registry extensibility).

### Release-path design decision (OPEN — architect's call)
- **Option A — pure builders in release** (recommended): release has zero
  template/interpreter code; fastest/smallest. Risk: dev (interpreter) and
  release (builders) are two paths → possible behavioral divergence → guard
  with a solid interpreter test suite.
- **Option B — always-template (Dioxus's choice)**: release also walks a
  `const` template + fills holes; dev only adds the swappable registry +
  watcher. One code path (dev==release behavior), at the cost of a tiny
  always-present indirection vs raw builder calls.
Leaning A given our optimized builder/GPU paint path; decide before the macro
commits to a codegen shape.

### The inflater IS a server-driven-UI (JSON→UI) engine — design for both (2026-07-19)
Realization (user): the data→widget inflater built for Tier 1 hot reload is
the SAME machinery as server-driven UI / JSON→UI. Both are "receive a UI
DESCRIPTION as data → inflate via the registry → paint"; only the data SOURCE
differs (hot reload = local dev socket; SDUI = remote server). Same boundary
for both: they can ship STRUCTURE + literals/styles + wiring to
ALREADY-COMPILED handlers, but NOT new logic/closures (the holes boundary =
also the app-store no-remote-code rule; this is how Airbnb SDUI / MS Adaptive
Cards / Shopify all work — layout + data + action IDs, never code).
**Two cheap up-front design choices make the descriptor serve hot reload AND
SDUI AND the external-DevTools wire from ONE build:**
1. The template descriptor MUST have a JSON (wire) form, not only an in-memory
   Rust struct — SDUI needs it to travel over the network.
2. Actions/handlers/data bind by NAME/string-key to a **handler registry**
   (`"onSave"` → compiled closure), not only by positional hole index —
   positional works for hot reload (same recompiled source), name-based is
   what SDUI needs. Same registry pattern as the widget registry / D115 icons
   / D124 materials.
Not building SDUI now, but design the descriptor JSON-serializable + name-bound
so it's not foreclosed.

### Current macro state (2026-07-19) — what's built
`rosace-macros` has `#[component]`, `#[state]`, `#[derive(ShaderUniforms)]`,
and `view!`. But `view!` is **syntax-sugar only**: it parses
`Column { Text { content: "Hi" } }` and emits `Column::new().child(Text::new()
.content("Hi"))`, then discards the structure — NO template descriptor, NO
`location!()` key, NO hole indexing, NO dev/release split. No
`#[rsc::app]`/`#[rsc::module]` macros exist. So for hot-reload purposes the
macro layer is effectively at zero: the syntax exists, the reload payload does
not. **Tier 1 is fundamentally macro-first** (the builder API can't be
hot-reloaded — no stable shape to diff/swap); the true first artifact is the
**template-descriptor data model** (the contract the macro writes and the
interpreter reads), then macro codegen, then the widget registry, then the
interpreter, then the watcher/transport (same wire as the external DevTools
tool — design once). Tier 0 (state-preserving restart) needs NONE of this and
can proceed independently as the floor.

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

## `rsc` CLI changes

- **`rsc dev`** becomes the orchestrator/supervisor: build the host once (or
  reuse a prebuilt `rsc` runtime), build the shared runtime dylib + UI module
  dylibs, start the watcher, on change map→`cargo build -p`→signal reload over
  an IPC channel (unix socket / named pipe; WebSocket for `--target web`),
  supervise, auto hot-restart on limits/failures.
- **`rsc build` / `rsc package`**: release = static-link everything into one
  binary; no dylibs, no reload machinery (`rsc-hot` feature off). Same source,
  two link modes.
- **`rsc new`**: scaffolds a project already split (host entrypoint + ≥1 UI
  module crate) so hot reload works out of the box.
- **Macros**: `#[rsc::app]` (host) and `#[rsc::module]` (UI module) hide all
  the ABI/supervisor boilerplate; authors write ordinary Components.

## Rollout phases

Ordered so the UNIVERSAL path (Tier 1) and the always-available floor (Tier 0)
land first — hot reload is useful on every platform before any dylib work.

1. **Tier 0 floor — hot restart with state preservation.** ✅ LANDED
   2026-07-21 (desktop). `rsc dev --watch` is a supervised build→run→relaunch
   loop (`run_desktop_watch` in `rosace-cli`): it owns the app child process,
   and on a successful rebuild kills + respawns it; a FAILED build leaves the
   running app untouched (no lost session to a typo). State preservation rides
   the already-on-disk `permanent` tier — `state_permanent` values are restored
   automatically across the restart (live-verified via `persist_demo`: touch a
   file → launch_count 5→6, restored note intact; broken build → app kept
   alive, counter unchanged). The in-memory `reload`/`session` tiers (transient
   atoms) still reset on restart — a named deferral pending D008's serde
   snapshot impl (D121). Web/mobile relaunch transports: not yet.
2. **Tier 1 — `view!` macro + template descriptor.** Declarative layer (D103).
   - ✅ Descriptor **data model** LANDED 2026-07-21 (D125) —
     `rosace-widgets::template`: `Template`/`TemplateNode`/`PropValue`/
     `StaticValue`/`TemplateKey`, positional holes, `hole_count` slot gate,
     plain-POD (serde-ready, serde dep deferred to the transport step). 8 unit
     tests. This is the contract the next two pieces read/write.
   - ✅ Macro two-mode codegen LANDED 2026-07-21 (**Option A** chosen — pure
     builders in release, zero template machinery in the binary). `view!` now:
     release → unchanged builder calls; dev (`rsc-hot` feature on `rosace-macros`
     + `rosace`) → also registers a `Template` keyed by `file!/line!/column!`,
     classifying literal props as `StaticValue` and non-literals as positional
     `Hole`s (props-before-children order). `TemplateRegistry` (register/get/
     snapshot) added in `rosace-widgets::template`; `view!` re-exported through
     `rosace::prelude`. Verified: macro token tests both modes + an integration
     test that COMPILES the emitted dev code against real `Column` widgets and
     asserts the runtime-registered descriptor (static 12.0 + hole #0, count 1,
     keyed by file). Note: the old `view!` emitted `Text::new().content(..)`
     which never compiled against real widgets — it had never been used.
   - ⧗ TODO (still step 2/adjacent): the **hole-filler array** (compiled
     `{expr}` values handed to the interpreter) — deferred to step 3 where the
     registry/interpreter can receive typed holes; wiring `rsc dev` to build
     with `--features rosace/rsc-hot` — deferred to step 4 where the watcher
     actually consumes the registry (the registry is proven to populate via the
     integration test today).

   **Slot-signature safety (REQUIRED, don't lose this).** `hole_count` equality
   is only the FIRST gate. Before the interpreter plugs a pushed template's
   holes, it must also verify each slot's **type/arity signature** matches the
   running binary's hole at that index — a stale or reordered template could
   otherwise drop a closure into a `bool` slot (or vice-versa) and the
   interpreter would build a corrupt widget instead of escalating. The diff
   step (step 4) must carry a per-slot signature (widget+prop+value-kind), not
   only the count, and a mismatch at ANY slot → escalate (Tier 2/0), never
   inflate. `hole_count` catches add/remove; per-slot signature catches
   type-swap/reorder. Both are the boundary; shipping only the count is unsafe.
3. **Tier 1 — runtime interpreter + widget registry.** ✅ Core LANDED
   2026-07-21 (`rosace-widgets::template::inflate`). `inflate(&Template,
   &[Box<dyn Any>]) -> Result<Box<dyn Widget>, InflateError>` rebuilds the
   widget tree by calling the SAME constructors the builder would; verified by
   layout-equivalence tests (inflated tree measures identically to the
   hand-written builder tree, for both fully-static trees AND value-hole
   binding by index). **Widget registry** = a global `OnceLock<RwLock<HashMap<
   String, BuildFn>>>` (mirrors the D115 icon registry): each widget registers
   ONE build closure `(props, children) -> Box<dyn Widget>` encapsulating its
   typed constructor — no widget downcasting. Built-ins seeded (Column/Row/Text
   as the proof set); third parties `register_widget(name, closure)` (D115
   extensibility bar — tested). Errors ESCALATE, never paint garbage:
   `UnknownWidget/UnknownProp/PropType/UnexpectedChildren/HoleOutOfRange`
   (`PropType` is the per-slot type guard in miniature — a String in an f32
   hole errors, doesn't build a bad widget).
   - ✅ **Macro↔interpreter loop CLOSED 2026-07-21.** Dev-mode `view!` now
     builds its widget by INFLATING (build descriptor → materialise hole-filler
     array `Vec<Box<dyn Any>>` from the compiled `{expr}`s → `inflate`), so dev
     has ONE path and it IS the hot-swap path. THE load-bearing invariant is
     proven: `rosace/tests/view_template.rs` (`cargo test -p rosace --features
     rsc-hot`) asserts an inflated real `view!` lays out IDENTICALLY to the
     hand-written builder tree with the same values (Option A's dev==release
     guard). Constraint: dev `view!` widgets must be registered (built-ins are;
     else a clear `view! inflate failed: unknown widget` panic) — the derive
     macro will remove this.
   - ✅ **Ergonomic registration LANDED 2026-07-21.** `FromProp` trait (public;
     impls for f32/f64/i64/bool/String — third parties impl it for their own
     prop types) is the typed edge between untyped props/holes and a widget's
     builder. The `inflatable!` macro generates a whole build closure from a
     `"prop" => setter: Type` table (container + leaf forms), so registering a
     widget is a few lines, not a hand-written closure — the boilerplate a
     future `#[derive(Widget)]` would emit, and the single place a widget's
     prop schema is declared (the same data a tooling/IDE schema would read).
     Tested: a macro-registered widget inflates (props + children + hole
     binding) identically to its builder, and still escalates on unknown prop /
     unexpected children.
   - ⧗ TODO: register the remaining built-in widgets via `inflatable!` (proof
     set is 3); a true `#[derive(Widget)]` (reads builder methods → emits the
     `inflatable!` table — needs an impl-level macro since a derive can't see
     methods); **handler/closure holes** (`on_press`) need a typed
     handler-registry — value holes only today; the live desktop **swap** demo
     (needs step 4's watcher) — the inflate mechanism it swaps through is
     proven end-to-end.
4. **Tier 1 — dev watcher → template diff → push.** Socket control channel;
   detect template-only vs needs-code by slot signature; push deltas. Instant
   structure/style reload on desktop + web + (via transport) mobile.
   - ✅ **Diff engine + per-slot safety check LANDED 2026-07-21**
     (`rosace-widgets::template::diff`). `diff(old, new) -> TemplateDiff`
     {`Unchanged` | `Swappable` | `Escalate(reason)`}. Implements the LOCKED
     safety rule: a hole's "type site" = `(widget kind, prop name)`; a swap is
     safe only when EVERY hole index keeps its site (so the frozen compiled
     value's type still fits). Escalates on `KeyMismatch` /
     `HoleCountChanged` / `HoleSlotRetargeted`. 8 tests (retext/wrap/static-add
     = swappable; add/remove/retarget hole = escalate). Documented positional
     caveat: guarantees TYPE safety, not value identity across a reorder of two
     same-typed holes (name-binding, deferred, removes it).
   - ✅ **Single-grammar refactor + runtime parser LANDED 2026-07-21.** The
     `view!` grammar now lives in ONE place — a new `rosace-view-syntax` crate
     (syn/proc-macro2, no widget deps) — consumed by BOTH the proc-macro
     (parse→codegen) and the runtime parser (parse→`Template`), so they can't
     diverge. `rosace-macros` refactored onto it (all 19+23 macro tests still
     green → identical codegen). `rosace-widgets::template::parse_template(body,
     key) -> Result<Template, ParseError>` turns edited `view!` source TEXT into
     a `Template` at runtime, same hole-indexing as the macro. Proven: the
     no-divergence equivalence test (`view_template.rs`) asserts the runtime
     parser and the compile-time macro produce the SAME template for the same
     `view!`.
   - ✅ **File scanning LANDED 2026-07-21.** `rosace_view_syntax::scan_file(src)
     -> Vec<ViewSite{line,column,element}>` token-walks a whole `.rs` (lexed,
     not fully parsed → survives mid-edit) and finds every `view!` with its
     line (proc-macro2 `span-locations`); does NOT descend into a matched body
     (widget grammar can't nest a `view!`). `rosace-widgets::template::
     parse_file_templates(src, file) -> Vec<Template>` keys each by
     `(file, line, col)`. Tested (multi-site, nested-in-fn, ignores non-view
     macros, hole classification). Match note: scanner col is the `view` token's
     0-based column vs the macro's `column!()` — match on `(file, line)`, col as
     tiebreaker.
   - ✅ **Swap application LANDED 2026-07-21.** `apply_swap(new: Template) ->
     SwapOutcome {Applied|Unchanged|Escalate(reason)|UnknownSite}` diffs an
     edited template against the running one and, if safe, REPLACES the site's
     registry entry. No in-place tree surgery: dev-mode `view!` now inflates the
     CURRENT registry descriptor each frame (`get(key)`, baseline registered
     once), so the next reactive rebuild shows the swapped shape — the frame
     loop does the work. Proven end-to-end: `a_hot_swap_changes_what_the_same_
     view_site_renders` — the same `view!` site (source unchanged) renders the
     edited spacing after `apply_swap`, verified by layout.
   - ✅ **Reload runtime + tolerant key matching LANDED 2026-07-21.**
     `apply_reload(file, src) -> ReloadReport{applied, unchanged, unknown,
     escalations, parse_error}` is the watcher's whole in-process decision:
     re-parse every `view!` in the edited file → match each to its running site
     → diff → apply safe swaps. Crucially it solves the **key-matching**
     problem via `registry::find_running(file, line)` — matches by line +
     path-suffix (segment-wise), so the watcher's ABSOLUTE path + the scanner's
     column still match the macro's `file!()`/`column!()` key (proven:
     `safe_edit_swaps_despite_different_path_form_and_column`). `ReloadReport`
     tells the watcher what to do: `hot_swapped()` → repaint; `needs_restart()`
     (a new hole/logic change or a brand-new `view!`) → Tier 0; `ignored()`
     (unparseable mid-edit) → do nothing. Pure over `(file, src)`, 4 tests.
   - ✅ **DESKTOP LIVE HOT RELOAD LANDED + VERIFIED LIVE 2026-07-21.**
     `App::launch` (under `rsc-hot`, `rosace/src/dev_reload.rs`) spawns a
     dev-only watcher thread: watches `src/`, on a `.rs` edit calls
     `apply_reload`, and on a safe swap calls `rosace_state::reset_to_global_
     dirty()` + `request_frame()` → the next reactive rebuild re-inflates the
     new shape (no in-place surgery; `paint()` rebuilds only when dirty, so the
     forced global-dirty is what makes the swap show). `rsc dev` builds with
     `--features rosace/rsc-hot`. **Verified in a running window**: edit
     `spacing: 20.0 → 80.0` → `⚡ swapped 1 view! site(s)` ~4s later, no
     recompile; edit `spacing → variable` (adds a hole) → `↻ needs a recompile`
     (correct escalation). Both branches live.
   - ✅ **App-structure decision**: `view!`/hot-reload apps are STANDALONE
     crates (own `[workspace]`, `rosace` path dep — the `my_app`/`demo_app`
     pattern), NOT workspace members. Reason: a workspace member is forced
     through the library's release regression (`cargo build --workspace`),
     which only compiles the release `view!` path — so a `view!` using `Text`
     or a custom widget (inflate-only) would break the lib build. Reference app:
     `rosace-examples/hot_reload_demo/` (excluded from the parent workspace).
   - ✅ **Platform portability CONFIRMED + web device-side built 2026-07-21.**
     The whole reload runtime compiles to **wasm32** (`rosace-view-syntax` and
     `rosace-widgets` incl. parse/diff/inflate/swap/reload) — so parse-on-device
     works on web (and, by extension, the mechanism is genuinely
     platform-portable, unlike the Tier 2 dylib path). Reload handling is now
     platform-agnostic: `dev_reload::apply_source_edit(file, src)` (swap +
     repaint) is the ONE entry point every transport calls. Transports:
     desktop = filesystem watcher (`spawn_dev_watcher`, native-only); web =
     `connect_hot_reload_socket()` — a wasm `WebSocket` to `ws://host/
     __rosace_hot` that parses `"<file>\n<source>"` and calls `apply_source_edit`
     (compiles for wasm; wire protocol `parse_reload_message` unit-tested).
     `web-sys`/`wasm-bindgen` added to rosace's wasm deps.
   - ✅ **Mobile socket transport BUILT + verified headless 2026-07-21.**
     `serve_hot_reload_socket(port)` (native, device side) listens on
     `127.0.0.1:port` for **length-framed** `"<file>\n<source>"` messages
     (4-byte BE length prefix + payload — raw TCP is a byte stream and source
     has newlines) and funnels each into `apply_source_edit`. Reusable for BOTH
     Android and iOS (only the host→device forwarding differs). Verified on
     localhost: `socket_transport_applies_a_pushed_edit` pushes a framed edit
     and the registered site hot-swaps. `App::launch` now picks the transport by
     target: desktop → file watcher, android/ios → socket, wasm → WebSocket.
   - ✅ **Mobile sender + FFI wiring BUILT 2026-07-21.** `rsc dev --target
     android|ios` (rosace-cli): watches `src/` and pushes framed `"<file>\n
     <source>"` to the device app; Android runs `adb forward tcp:9765 tcp:9765`
     first, iOS-sim shares host localhost (no forward). Sender framing
     (`push_source_edit`) verified on host to match the receiver's `read_frame`
     (`push_source_edit_writes_a_length_framed_message`); target parsing tested.
     Device side: `rosace-ffi::Engine::init` starts `serve_hot_reload_socket` on
     android/ios under `rsc-hot` (mobile enters here, not `App::launch`);
     `rosace-ffi` got an `rsc-hot` feature; `rosace::dev_reload` is now
     `pub mod`. The FFI line is a call to host-tested public fns (transport +
     framing both proven on host) — full mobile-target compile deferred (wgpu
     is slow; low-risk one-liner).
   - ✅ **Mobile deploy plumbing COMPLETE 2026-07-21.** Hot-reload build path:
     `RSC_HOT=1 rsc run --target android|ios` builds the app WITH the feature
     (Android Gradle cargo task adds `--features rosace-ffi/rsc-hot` + drops
     `--release` when `RSC_HOT=1`; iOS-sim legacy build adds `rosace/rsc-hot`),
     then `rsc dev --target android|ios` pushes edits. Android `INTERNET`
     permission added (scaffold template + demo_app manifest) for the localhost
     bind. Full mobile MECHANISM built + host-verified; only the on-device/
     emulator LIVE run remains (needs a device + the slow wgpu mobile build —
     inherently not headless-verifiable here).
   - ✅ **WEB server half BUILT 2026-07-21 (live Chrome verify pending).**
     `rsc dev --target web` now: builds wasm WITH `rosace/rsc-hot` (so the
     client is present), serves `dist/`, and runs a **WebSocket server** at
     `/__rosace_hot` + watches `src/`, pushing `"<file>\n<source>"` frames on
     edit. The WS handshake (SHA-1 + base64 accept key) and server→client text
     framing are hand-rolled in `rosace-cli/src/commands/hot_ws.rs` (no new dep;
     rsc-cli kept light) and unit-tested against known vectors incl. RFC 6455's
     handshake example. Each connection served on its own thread (a WS stays
     open). Client side (`connect_hot_reload_socket`) built earlier + wasm-
     compiled. 102 CLI tests pass. LIVE browser round-trip (load page → edit →
     console `⚡ swapped`) deferred — user away from the Mac; verify via Chrome
     mcp tools later.
   - **ALL THREE platform transports now wired**: desktop (file watcher —
     LIVE-verified in a window), mobile (socket — host-verified + deploy
     plumbing), web (WebSocket — built + component-verified, live pending).
5. **Tier 2 — runtime split & shared-dylib singleton.** Make `rosace` a dylib
   under `rsc-hot`; prove ONE state-store instance across host + one module.
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
  per-target link flags in `rsc`.
- **Non-goals:** hot-reloading the host/runtime itself, new dependencies,
  trait/type-signature changes, macro changes → restart-only (D041).
```

## Future direction — hot-reloadable navigation (named 2026-07-21)

Goal: extend the data-driven reload model from widgets to **navigation**, so
editing screens, the nav graph, and transitions reloads live too — "everything
hot-reloadable, with a better arch." This is deliberately a FUTURE phase, but
it is why the current pieces are shaped the way they are — it rides the SAME
machinery, not a parallel system:

- **Screen content** already reloads via `view!` (the descriptor/inflater/diff
  built in steps 2–4). No new mechanism needed.
- **Navigation actions** ("on tap → go to Settings") are closures = **handler
  holes**, which today escalate. Unblocking them needs the deferred **typed
  handler-registry + name-based hole binding** (D125's named extension, also the
  SDUI convergence): a `"navigate(Settings)"`-style named action bound to a
  compiled handler, so the action can travel as data while the code stays on
  device (the app-store no-remote-code boundary).
- **The nav graph itself** (routes, transitions) would get its OWN
  **data-driven descriptor** — the same shape as the widget `Template`
  (name-keyed nodes + typed slots) inflated through the same registry/`inflate`
  path and gated by the same `diff` safety check. Today `ScreenNav<R>` is a
  typed Rust enum + closures; the "better arch" is a descriptor form that the
  inflater can rebuild, so route/transition edits diff-and-swap like widget
  edits. `ScreenNav` stays the low-level truth (the builder-equivalent).

The convergence to preserve: **one descriptor model, one registry, one
inflater, one diff** — widgets, navigation, and (later) SDUI are all just
different data feeding the same load-bearing pipeline. Don't build a separate
navigation-reload stack.
