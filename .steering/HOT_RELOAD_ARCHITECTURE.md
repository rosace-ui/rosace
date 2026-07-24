# ROSACE Hot Reload — Architecture (for review)

> Status: **DRAFT for your confirmation.** This reflects the *current, proven*
> design as of 2026-07-23 (after the Tier-2 dylib work). Read, then tell me
> "go" or what to change. Nothing here is built beyond what "Status" notes say.

---

## 1. Goal

Edit any part of your app → see it live in ~1–2s, **without losing app state**,
without restarting. Robust under real workflows: single edits, rapid saves, and
multi-file refactors. Never worse than a fast restart on platforms that can't
hot-swap code.

---

## 2. Three layered tiers (they stack; each is a safety net for the one above)

| Tier | What it swaps | How | Platforms | Speed |
|---|---|---|---|---|
| **Tier 1** | `view!` shape/text/props (DATA, not code) | re-parse the `view!` block → push a new template descriptor → interpreter rebuilds that subtree | **every** platform (incl. web, iOS device) | instant (no compile) |
| **Tier 2** | **any code** — `build()`, handlers, structure | recompile the UI crate → `dlopen` the new `.dylib` → swap the root Component | desktop, Android-dev, iOS **simulator** | ~1–2s (incremental compile) |
| **Tier 0** | (floor) full app | rebuild + relaunch + rehydrate persistent state | everywhere | seconds |

- **Tier 1** is a *data* mechanism → works where code can't be loaded (web/iOS
  device). Limited to `view!` blocks + registered widgets. **Already works.**
- **Tier 2** is the *code* mechanism → the "edit anything" experience. Needs
  `dlopen`, so not web / not iOS-device. **Proven; being productized.**
- **Tier 0** is the guaranteed fallback when an edit exceeds the active tier.

This doc is mostly about **Tier 2** (the new work) + how state survives.

---

## 3. The multi-dylib architecture

The running process is split into **three ABI domains**:

```
┌────────────────────────────────────────────────────────────────┐
│  SHELL  (host binary — stable, never reloaded in a session)     │
│  • main() • winit event loop • window/surface • FFI (mobile)    │
│  • the reload supervisor (watch → rebuild → tell host to swap)  │
└───────────────▲───────────────────────────────▲────────────────┘
                │ links dynamically              │ dlopen / libloading
                │                                │
┌───────────────┴──────────────┐   ┌────────────┴───────────────────┐
│ SHARED RUNTIME DYLIB          │   │ UI MODULE DYLIB(S) (reloadable) │
│ librosace.dylib               │   │ libui.dylib  (later: many)      │
│ • ALL of rosace: widgets,     │   │ • YOUR screens / components     │
│   renderer, layout, theme     │   │ • build() logic + handlers      │
│ • ⭐ THE ATOM STORE ⭐         │   │ • __rsc_dev_root() entrypoint   │
│   (rosace-state statics)      │   │ • depends on rosace ONLY        │
│ compiled ONCE, shared by all  │   │ rebuilt on each edit, swapped   │
└───────────────────────────────┘   └─────────────────────────────────┘
```

Three key rules make this work:
1. **One shared `librosace.dylib`.** The shell and every UI module link the
   *same* rosace dylib (via `-C prefer-dynamic`). So there is exactly **one**
   copy of every rosace `static` — including the atom store.
2. **The UI module depends on rosace ONLY** — never winit, never the FFI/shell
   crate. (This is *required*, not stylistic — see §7.)
3. **Ordered swap:** load the new dylib → rebuild the tree (fresh handlers) →
   drop the old tree → *then* unload the old dylib. So a handler from the old
   code is never called after its dylib is gone.

---

## 4. rosace vs. ui — what goes where, and why they're different

| | **rosace** (shared dylib) | **ui** (reloadable module) |
|---|---|---|
| **What it is** | the framework (published, versioned) | *your* app's screens |
| **Who writes it** | us | you |
| **Changes during a session?** | ❌ no | ✅ constantly (that's the point) |
| **Reloaded?** | ❌ never (it's the stable base) | ✅ on every edit |
| **Contains** | widgets, renderer, layout, theme, **atom store** | components, `build()`, event handlers |
| **Depends on** | its own sub-crates | **rosace only** |

**Why the split is mandatory:** to *swap* your UI while the app runs, your UI
must be in a **separately-loadable file** (the dylib). You can't swap code baked
into the running binary. So the swappable part (screens) goes in `ui`; the
un-swappable part (window, event loop, `main`) stays in `shell`. rosace sits
underneath both as the shared, stable base — the "kernel" both link against.

---

## 5. ⭐ Atoms & state — your questions, answered precisely

You asked: *are atoms persistent, or only in memory? How does reload preserve
the cache?* Here is exactly how it works.

### 5a. There are two completely different kinds of state

| | **In-memory atoms** | **Persistent state** |
|---|---|---|
| **API** | `GlobalAtom`, `ctx.state(default)` | `ctx.state_permanent(key, default)` |
| **Stored where** | RAM — inside `librosace.dylib` (rosace-state statics / thread-local `STORE`) | **on disk** — SQLite (rosace-storage / `PersistBackend`) |
| **Survives hot-swap (Tier 2)?** | ✅ yes | ✅ yes |
| **Survives full restart (Tier 0)?** | ❌ no (resets to default) | ✅ yes (read back from disk) |

So: **regular atoms are memory-only.** They are **not** written to disk. Only
`state_permanent` (SQLite) is persistent across a process restart.

### 5b. Why in-memory atoms SURVIVE a Tier-2 reload (the key insight)

The atom store does **not** live in your `ui` module. It lives in
**`librosace.dylib`** — the shared dylib that is **never reloaded**.

```
  ┌─ librosace.dylib (NOT reloaded) ──────────┐
  │   atom store:  Count = 7   Theme = dark    │  ← state lives HERE, untouched
  └───────────────▲───────────────────────────┘
                  │ reads/writes the same store
  ┌───────────────┴─── libui.dylib (RELOADED) ─┐
  │   build() { let c = ctx.state(0); ... }     │  ← only THIS code is swapped
  └─────────────────────────────────────────────┘
```

When you edit a screen:
1. Only `libui.dylib` recompiles and reloads.
2. `librosace.dylib` — and the atom store inside it — **stays exactly as is.**
3. The new `build()` calls `ctx.state(0)` again, but the store already has a
   value for that slot, so it returns the **existing** value (the `0` default is
   ignored). Your `Count = 7` is still 7.

**How the store matches state to the right component across a reload:** atoms
are keyed by **`ComponentId` (position in the tree) + hook order** — not by a
pointer into the old code. As long as the tree *shape* is unchanged, the keys
line up and every atom keeps its value. If an edit changes the shape (adds/
removes a hook), only the *affected subtree's* state gracefully resets (never a
crash) — the rest is preserved.

Think of it as: **the atom store is a little server; your UI is the client.**
Reloading swaps the client; the server keeps the data.

### 5c. What resets, and when
- **Tier-2 swap** (edit a screen): in-memory atoms **preserved** (shared dylib
  untouched).
- **Tier-0 restart** (change to the shell, new dependency, or a platform with no
  dylib): the process restarts → in-memory atoms **reset to defaults**; only
  `state_permanent` (disk) values come back. *(A future enhancement can
  serialize in-memory atoms to a blob across a restart — noted, not built.)*

---

## 6. The build recipe (the four problems we solved)

Making an app crate loadable as a dylib surfaced four real linker issues.
Fixes, all now codified in `rsc dev`:

1. **Native libraries** (e.g. SQLite): the module re-instantiates a
   dependency's inline code that calls native C → must re-link it.
   → pass `-L/-l` (harvested from the build graph) to the module only.
2. **Shared generics**: debug builds emit some generics as "provided by an
   upstream dylib" → not found at load. → `-Z share-generics=off` on the module
   → it instantiates its own generics locally.
3. **Dead-stripped statics**: the shared dylib drops statics rosace itself
   doesn't reference but the module needs. → `-C link-dead-code` keeps them.
4. **Duplicate/mismatched deps (winit)**: an app that pulls winit (via its
   shell/`launch()`) builds a *different* winit than the shared rosace → symbol
   mismatch. → **the ui module must depend on rosace ONLY** (no winit/shell) →
   it references no winit symbols and unifies with rosace cleanly. *(This is
   why §3 rule #2 exists.)*

Consistency rule: **every** cargo call in a session uses the *same* build env
(same `RUSTFLAGS`, same target dir) so nothing needlessly rebuilds.

---

## 7. ⭐ The rebuild pipeline — debounce + queue (the piece to build now)

Your concern: rapid edits / refactors must not trigger a pile-up of rebuilds.
Correct. The design is a **trailing debounce + single-slot coalescing
rebuilder** (the pattern Vite / cargo-watch use):

```
        edit(s)                    build finishes, nothing pending
  Idle ─────────► Debouncing ─────────────────────────────────► Idle
   ▲                  │  quiet for ~300ms (resets on each edit)
   │                  ▼
   │              Building ◄──────┐
   │                  │           │ edit arrived while building
   │  finished,       │           │ → set DIRTY (do NOT start a 2nd build)
   └── not dirty ◄────┘           │
                      was DIRTY? ─┘  → rebuild exactly ONCE more → Idle
```

Guarantees (this is the "no hassle" part):
- **A burst of N edits → at most 1 in-flight build + 1 trailing build.** Never a
  backlog of N sequential builds.
- **Multi-file refactor**: the ~300ms trailing window batches all the saves into
  **one** rebuild.
- **Edits during a build are never lost**: they set the dirty flag → exactly one
  follow-up build on the latest source. *(This also fixes the current
  "second edit sometimes doesn't rebuild" bug — by construction.)*
- The final build always reflects the **latest bytes on disk**.

Status: **NOT built yet — this is what I'll implement on your "go".** It
replaces the current naive `for event in rx { build }` loop.

---

## 8. Scaling to many dylibs (future — your "ceiling + working set" idea)

Start with **one** `ui` dylib (simplest; incremental compile keeps it fast).
When an app grows large enough that relinking one big ui dylib gets slow, split
`ui` into **N feature/screen dylibs** with a **hot/cold working set**:
- recently-edited screens → a small "hot" dylib (fast relink);
- stable screens → "cold" dylibs (cached, not relinked);
- screens migrate hot↔cold by recency.

Crucially: **all** ui dylibs link the *same* `librosace.dylib`, so they **all
share the one atom store** — state flows across modules exactly as within one.
The reload *protocol* is identical; only the *partitioning* changes. So starting
with one dylib does not foreclose this — it's an additive upgrade. (Not built;
do only when relink time proves it's needed.)

---

## 9. Release mode — no dylibs at all

In `rsc build` (shipping), there is **no** split and **no** dylib:
`ui` + `shell` + `rosace` are **statically linked into one ordinary
executable**, exactly like any normal Rust binary. The whole hot-reload
apparatus exists **only** under `rsc dev`. Users never see two files, and there
is zero runtime cost from any of this.

---

## 10. What's built vs. pending (honest status)

| Piece | Status |
|---|---|
| Tier 1 (`view!` data reload), all platforms | ✅ works |
| Tier 2 mechanism (dylib swap + state survival) | ✅ proven live |
| The 4-problem build recipe, baked into `rsc dev` | ✅ done |
| `rsc dev` runs Tier 2 on a real stateful app | ✅ proven (single edit) |
| **Coalescing rebuild pipeline (§7)** | ✅ done (`--debounce`, default 300ms) |
| Crash-hardening the swap (keep old dylibs loaded + clear handler caches) | ✅ done — verified crash-free over text/structural/logic edits |
| `rsc new` scaffolds shell+ui automatically | ⬜ **build next** |
| Multi-dylib working set (§8) | ⬜ future (when needed) |
| Serialize in-memory atoms across a Tier-0 restart | ⬜ future |

---

## 11. Open questions for you
1. **Debounce window**: 300ms default — good, or do you want it configurable
   (`rsc dev --debounce <ms>`)?
2. **Scaffold**: OK to make `rsc new` generate the **shell + ui** two-crate
   layout by default (required for Tier 2 to "just work")?
3. **Scope now**: build §7 (rebuild pipeline) first, then crash-hardening, then
   the scaffold? Or a different order?
