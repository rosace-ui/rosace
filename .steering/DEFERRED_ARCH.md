# Optional & Deferred Architecture

> A log of architecture directions we've explored but deliberately **deferred**,
> kept **optional**, or decided **minimally** — with the reasoning and what would
> trigger revisiting. The point: don't lose the thinking.
>
> This is NOT the decision log (`DECISIONS.md`) — it's the "roads considered"
> record. An item here **graduates to a formal `D-xxx` decision** when it's
> actually built.

## Index
- **[A1] Persistence backend** — ✅ DECIDED (minimalist: platform-native)
- **[A2] User-facing reactive local data layer** — ⏸ DEFERRED (future direction)
- **[A3] Client-consumed UI/data structure (server-driven UI + DevTools/tracing wire)** — ✅ NEEDED (future)
- **[A4] Handler / closure holes (interactive hot reload)** — 🟡 NULLARY DONE + live-verified; arg-handlers deferred
- **[A5] `rsc new` dependency mode (path in dev → crate version at release)** — 🔨 IN PROGRESS (P1)
- **[A6] Asset system (declare → typed codegen → per-platform resolve → load, hot-reloadable)** — 📐 DESIGNED; to build (P2)

---

## A1 — Persistence backend: platform-native, minimalist
**Status: DECIDED (direction) 2026-07-21 · implementation pending.**

**Context.** `ctx.state_permanent` persists through the `PersistBackend` trait
(in `rosace-core` — an already-pluggable seam; `set_persist_backend`). The only
implementation, `rosace-storage`, uses `rusqlite` with **`bundled`** (compiles C
SQLite) on every target, and uses it as **pure KV** (`kv(key TEXT PRIMARY KEY,
value BLOB)`). Bundling C caused real friction: Android NDK `cc`/`cmake` setup,
slow compiles, larger binaries, and no wasm build.

**The realization (user):** SQLite was chosen for **durability**, not SQL — and
every platform *already ships* a storage engine. So don't bundle one C engine
everywhere; **abstract (already done) + install a platform-native backend per
target.** The path is already open (pluggable trait); the gap is the backends.

**Options considered.**
- *Bundled SQLite everywhere (status quo)* — bulletproof durability, but C-dep,
  heavy compiles, no wasm, overkill for KV.
- *`redb` (pure-Rust KV, ACID, crash-safe)* — drops C, KV-native, faster builds;
  but doesn't fix web, less battle-tested than SQLite, loses "free SQL later".
- *Platform-native backends behind the trait (chosen)* — each platform uses its
  own storage.

**Decision (minimalist).** Thin `PersistBackend` trait + **per-platform default
backends**, installed at the entry point (`App::launch` / `rosace-ffi` init),
**overridable** by the app (batteries-included but swappable). Concretely:
1. **Mobile — link SYSTEM SQLite** (drop `bundled`). The OS ships `libsqlite3`;
   linking it removes the NDK C-compile friction, shrinks the binary, and uses
   the OS's hardened/auto-patched SQLite. **Quick win.**
2. **Web — the real gap.** `localStorage` (permanent) / `sessionStorage`
   (session tier) / `IndexedDB` (large/structured). Note `sessionStorage`
   **clears on tab close** → maps to D008's `session` tier, NOT `permanent`.
   IndexedDB is async → awkward from the synchronous persist model (real work).
3. **Desktop — keep bundled SQLite** (revisit `redb` later; lowest stakes).
4. **Consistency — a shared conformance test-suite** every backend must pass
   (one trait, N impls that must agree on value semantics/errors/durability).

**Tier → store mapping:** `reload`/`session` → in-memory / `sessionStorage`;
`permanent` → `localStorage` / IndexedDB / SQLite.

**Consequence.** Because native platforms use *their* SQLite anyway, SQLite
stays the natural native choice — its query power is free for a future user data
layer ([A2]). `redb` becomes an *optional desktop alternative*, not a rip-out.

**Relates:** D114/D121 (persistence tiers + `PersistBackend`), D113 (crate-choice
reasoning), Known Issue #16 (Android parked). Graduate to a `D-xxx` when built.

---

## A2 — User-facing reactive local data layer
**Status: DEFERRED (future direction, named so it's not foreclosed).**

**Question.** Should app developers get to store and **query** their app's
structured data locally (offline caches, lists, records)? KV `get`/`set` by key
can't answer "all todos where `done = false` ordered by date" without loading
everything into memory.

**The ROSACE-right answer** is NOT raw SQL (footgun surface, un-curated). It's a
**typed, reactive local data layer** where *a query behaves like an atom* — when
the underlying data changes, every component that read the query rebuilds
automatically (like SwiftData / Room-with-Flow / Drift). This plays to ROSACE's
single biggest strength (the reactive engine) and fits "invisible on top."

**Key link to [A1]:** this decides the backend. Commit to user queries → SQLite's
SQL/indexes are a *feature* (keep SQLite). Stay KV-only → `redb` is fine.

**Scope.** A real feature: typed records + a query API + reactivity integration +
schema migrations + per-platform backends. A future phase with its own decision.
**Near-term:** an app that needs structured local data today uses its own store
crate (an honest escape hatch).

---

## A4 — Handler / closure holes (interactive hot reload)
**Status: NULLARY handlers (`Fn()` — buttons) DONE + LIVE-VERIFIED 2026-07-21. Arg-taking handlers (`Fn(T)`) still deferred. The keystone for interactive `view!` + nav reload + SDUI.**

**What landed (nullary / button case).** `view!` handler props (`on_press`,
`on_click`, `on_tap`, `on_long_press`) now hot-reload. Macro wraps the closure as
`Arc<dyn Fn() + Send + Sync>` in the dev hole array (`is_handler_prop` in
`rosace-macros/src/view.rs`); release path unchanged. `Handler` type +
`FromProp<Handler>` + `build_button` (registers Button: positional label +
`on_press` handler hole, applied as `move || (*h)()`) in
`rosace-widgets/src/template/inflate.rs`. VERIFIED: unit (button inflates with a
handler hole; non-handler value in a handler slot → PropType), both-mode
integration (`view!` button compiles in release AND inflates in dev), and LIVE —
`hot_reload_button_demo` runs a working counter-button under `rsc dev` (before A4
it panicked on the closure) and hot-swaps on edit (`⚡ swapped`).

**Still deferred: arg-taking handlers** (`on_change(f32)` slider, `on_change(String)`
input) — each `Fn(T)` is a distinct type needing its own wrap/`FromProp`; the
macro would need per-signature handling. So sliders/inputs inside `view!` still
need the hybrid until this lands.

**What "not supported" means.** A `view!` **hole** is a dynamic `{expr}` slot — the
non-literal bits. `Text(title)` where `title` is a runtime `String` is a hole.
**Value holes** (String/f32/bool/i64) DO hot-reload: the runtime carries them as
data (`Box<dyn Any>`), downcasts them, and hands them to the widget. A
**function/closure hole** — a `Button`'s `on_press: || …` — does NOT hot-reload
today (the *dev/inflate* path). In RELEASE it's fine (compiled calls); only the
inflate interpreter can't bind it.

**Why.** A `String` has a concrete, nameable type → `Box<dyn Any>` downcasts to
it. A closure has an **anonymous, unnameable type** → nothing to downcast to; and
widget setters take `impl Fn()` (a compile-time generic), which can't be produced
from type-erased runtime data. So a closure can't ride the value-hole machinery.

**Currently noted (scattered — this entry consolidates it):** D125 ("Value holes
only; handler/closure holes deferred — need a typed handler-registry"), D103
boundary ("may NOT change a handler body → escalate"), `rosace-widgets/src/
template/inflate.rs` module doc.

**Future design (how we'd support it).** Wrap each handler as a **callable,
nameable trait object**: `Arc<dyn Fn(Sig) + Send + Sync>`. Unlike an anonymous
closure, `Arc<dyn Fn(String)>` is a concrete type — it can be stored in a
`Box<dyn Any>`, downcast back, AND called (`Arc<F: Fn>` itself impls `Fn`, so
`button.on_press(arc)` works). Two binding modes:
- **positional** (hot reload) — the macro wraps the handler as `Arc<dyn Fn(Sig)>`
  and puts it in the hole array by index; a per-signature `FromProp` impl
  downcasts it. Complication: the macro must know the slot's handler signature
  (the widget setter knows it; the macro doesn't) → likely a convention
  (`on_*` props are handlers) or an `inflatable!` handler-slot declaration.
- **name-based** (SDUI) — handlers register by NAME (`"onSave"` → the compiled
  `Arc<dyn Fn>`); the descriptor carries the name; the runtime looks it up. This
  is the "name-based binding" of D125, and what server-driven UI needs.

**Unblocks (why it's a keystone):** interactive widgets *inside* `view!` (buttons,
sliders, inputs); **hot-reloadable navigation** ([A2] and the nav vision — nav
actions are closures); **server-driven UI**. One piece, three payoffs.

## A5 — `rsc new` dependency mode (P1)
**Status: IN PROGRESS 2026-07-22.**

`rsc new` today generates `rosace = { path = "{framework}/rosace" }` where
`framework = env!("CARGO_MANIFEST_DIR").parent()` — an ABSOLUTE path to the
checkout the `rsc` binary was built from. Great for dogfooding (builds against
local crates, no publish), but machine-specific: a distributed `rsc` would emit
a path pointing at the author's disk → users' scaffolds won't build.

**Fix:** auto-detect. If `{framework}/rosace/Cargo.toml` exists on the machine
running `rsc` (dev checkout) → path dep. Else (installed/published `rsc`) →
`rosace = "<rsc's own version>"` (crate dep from crates.io; versioned together
via `env!("CARGO_PKG_VERSION")`). Same binary does the right thing in dev and
for real users. Relates: the examples repo's same path→crate-version flip.

## A6 — Asset system (P2 — building)
**Status: layers 3+4 (load API + dev resolve/hot-reload) LANDING 2026-07-22.
Layer 2 (typed codegen) + release bundling still to do. Completeness bar: ASSET
HOT-RELOAD must work — verified live.**

**Built so far (2026-07-22):**
- `rosace_core::asset` — the one resolver: `resolve(name)`, `bytes(name)`,
  `set_root(path)` (mobile/desktop-release override), default root `assets/`.
- Typed loaders on it: `ImageWidget::asset("logo.png")`,
  `FontCache::from_asset("fonts/Brand.ttf")` → all platforms, one API.
- Hot-reload: `ImageCache::clear/invalidate`; the desktop dev watcher now
  ALSO watches `assets/` (widened `is_watched_ext`) and on an asset change calls
  `dev_reload::apply_asset_change` → clear image cache + repaint. String API
  works today; typed handles are the codegen layer still to come.

Goal: a multi-platform asset pipeline that beats Flutter on type-safety,
hot-reload, and wasm. Four layers:
1. **Declare** — `rsc.toml [assets] dirs = ["assets"]` + an `assets/` dir.
2. **Index (build-time codegen)** — scan the dirs → generate a TYPED `assets`
   module (one const `Asset` handle per file, nested by folder, carrying a
   content hash). Typos → compile error (vs Flutter's stringly-typed
   `Image.asset("path")`). Fits P5 (compile-time checks).
3. **Load (runtime API)** — `Image::asset(assets::LOGO)`, `Font::asset(...)`,
   `asset_bytes(assets::data::CONFIG)`. Typo-proof, autocomplete.
4. **Resolve (per-platform, mode-aware — REUSES the rsc-hot dev/release split)** —
   - **dev** (`rsc dev`): load from DISK at source path → edit an asset →
     HOT-RELOADS live (the completeness bar).
   - release desktop/mobile: copy into the platform bundle (macOS Resources/,
     iOS bundle, Android assets/), load by resolved path.
   - release web (wasm): embed (`include_bytes!`) or content-hashed fetch (no fs).

**Why better than Flutter:** typed handles (compile-time typo safety);
hot-reloadable assets (rides the hot-reload engine); wasm-native embed; one
API, invisible per-platform resolution (P1); content hashes → free web
cache-busting. Architecturally consistent — the dev-disk/release-embed switch
is the SAME pattern as `rsc-hot`; the typed codegen is the same family as the
`view!` macro / widget-schema.

**First-build decision (open):** embed-everywhere-in-release (simplest, bigger
binaries, wasm-safe) vs bundle-native/embed-web (leaner, more moving parts).
Leaning embed-everywhere for the first cut. Hot-reload verification (edit an
asset, see it live in `rsc dev`) is the definition of done — "otherwise it is
not complete" (user).

**Named future extensions:** build-time asset transforms (image
resize/optimize, font subsetting, @2x/@3x density variants) hook into layer 2.

### A6.1 — Theme-from-assets (user idea 2026-07-22, "plan later if worth it")
Load a whole THEME from an asset file (e.g. `theme.toml`/`.json` describing
colors, typography, radii, and referencing image/font assets), not from code —
so people can publish & share themes and an app loads one at runtime:
`Themes::from_asset("themes/midnight.toml")`. Naturally rides A6: a theme is
just another typed asset that resolves + hot-reloads (edit `theme.toml` under
`rsc dev` → live restyle — a compelling demo). **Worth it?** Likely yes as a
differentiator, but only AFTER the core asset load + typed codegen land; needs a
declarative `ThemeData` (de)serialization format decided first (serde on
`ThemeData`, asset-ref fields for fonts/images). Not blocking; logged so it's not
lost. Relates: `rosace-theme` `ThemeData`/`Themes`, A6 loaders, hot-reload.

**Relates:** rsc-hot (dev/release split reused), the file watcher (extend to
`assets/`), rosace-widgets image/font widgets, `rsc build`/`package`.

## A3 — Client-consumed UI/data structure (server-driven UI + DevTools/tracing wire)
**Status: NEEDED (future) — rides the inflater; do NOT foreclose it.**

**The point (user).** Can the client **consume a STRUCTURE supplied from
outside** — a UI/data description delivered as *data* — and use it, rather than
running *our compiled logic*? Yes, and we need it. This is the data-driven /
server-driven-UI direction, and it's the **same machinery as hot reload's
inflater**: receive a UI DESCRIPTION → inflate via the widget registry → paint.
Only the SOURCE differs (hot reload = local dev socket; SDUI = remote server;
DevTools = the inspector tool).

**The boundary (what CAN travel).** Structure + literals/styles + wiring to
ALREADY-COMPILED handlers — but NOT new logic/closures. That holes boundary is
also the app-store no-remote-code rule (how Airbnb SDUI / MS Adaptive Cards /
Shopify work: layout + data + action-IDs, never code). Name-bound actions need
[A4] (the handler-registry).

**Why "as tracing part."** The observability/DevTools track (D123) wants to send
the running app's structure to an external inspector — and that's the SAME wire
and SAME descriptor as SDUI. Design once, three consumers: hot reload, SDUI,
DevTools. Two cheap up-front choices keep it open (already flagged in
HOT_RELOAD.md): the descriptor has a JSON/wire form, and actions bind by NAME
(not only positional index).

**NOT this item:** *remote DB access from the client* — holding a DB connection
to a server — is a separate thing and IS a non-goal (D113: that's server-side
behind an HTTP API). (Originally mis-logged here as the user's point; it was not.)

**Relates:** the inflater (rosace-widgets template), [A4] handler-registry,
[A2] data layer, D123 (observability/DevTools), D107 (web semantic-tree shadow).
