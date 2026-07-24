# Hot Reload

> Covers `rosace-hot-reload` (file watching), `rosace-widgets::template` (the Tier 1 data-swap engine, in `rosace-widgets/src/template/`), and `rosace`'s `dev_reload.rs`/`dev_host.rs` (the Tier 1 driver and Tier 2 dylib host). Distilled from [`.steering/HOT_RELOAD.md`](../../.steering/HOT_RELOAD.md) — the plan document — against what actually landed. See [D102 in DECISIONS.md](../DECISIONS.md).

## In one sentence

Hot reload is three independent fallback tiers stacked from fastest to slowest — swap just the changed *data* (Tier 1, works everywhere including iOS device and web), swap the changed *code* by reloading a dylib (Tier 2, desktop/simulator only), or restart the process and rehydrate persisted state (Tier 0, the universal floor) — chosen automatically based on what actually changed and what the target platform allows.

## Mental model

Picture three doors out of "I edited something, make it show up live," tried in order of how cheap they are:

```mermaid
graph TD
    EDIT["source file changed"] --> T1{Tier 1: is this a\nview! shape edit?\n(literal/structure only)}
    T1 -->|yes| SWAP["swap the Template in the\nlive registry — reinterpret\nnext frame, no recompile"]
    T1 -->|no: new hole / new logic| T2{Tier 2: desktop or\niOS-sim/Android-dev?}
    T2 -->|yes| DYLIB["rebuild the UI dylib,\ndlopen it, swap the root\nComponent — state survives"]
    T2 -->|no: iOS device / web| T0["Tier 0: rebuild + restart\nthe process, rehydrate\npersisted atoms from disk"]
```

Only Tier 1 requires no new machine code to load, which is why it's the only tier that works absolutely everywhere (iOS device forbids `dlopen`ing new code; wasm has no dynamic linking at all).

## How it works

### Tier 1 — template (data) hot-reload

**1. A `view!` tree has a data half and a code half.** The macro splits every `view! { ... }` invocation into a [`Template`](../../rosace-widgets/src/template/descriptor.rs) (the widget kinds by *name*, their nesting, and literal prop values — pure data) plus a compiled array of **holes**: the `{expr}` bits (a computed string, an `on_press` closure) that stay real Rust code and are filled in positionally at runtime. This split is the entire reason Tier 1 can hot-swap without recompiling — only the data half ever needs to change.

**2. In dev builds, every `view!` site registers its `Template` in a process-global registry.** [`template::register`](../../rosace-widgets/src/template/registry.rs) keys the map by [`TemplateKey`](../../rosace-widgets/src/template/descriptor.rs) (`file` + `line` + `column`, from `file!()`/`line!()`/`column!()`). This registry is compiled out entirely in release builds — the `view!` macro emits pure builder calls with zero template/interpreter machinery under release, gated by the `rsc-hot` cargo feature (only `rsc dev` turns it on).

**3. An edited file's source is re-parsed and diffed, not blindly swapped.** [`apply_reload(file, src)`](../../rosace-widgets/src/template/reload.rs) re-parses every `view!` in the edited source ([`parse_file_templates`](../../rosace-widgets/src/template/parse.rs)), finds the matching **running** site by `(line, path-suffix)` — tolerating that the file-watcher's path form and the macro's `file!()` form disagree — and hands the pair to [`diff`](../../rosace-widgets/src/template/diff.rs). The diff answers exactly one question: is this a safe data edit, or did it touch compiled logic?
   - **`TemplateDiff::Unchanged`** — identical shape, nothing to do.
   - **`TemplateDiff::Swappable`** — same widget kinds, same hole positions, only literal values (or structure/reordering of literals) differ → safe.
   - **`TemplateDiff::Escalate(EscalationReason)`** — the number of holes changed (a new `{expr}` = new compiled code), or a hole now targets a different `(widget, prop)` slot than the running binary compiled it for. Either means the edit needs a recompile, and the diff refuses to swap.

**4. A safe swap replaces the registry entry; nothing is mutated in place.** [`apply_swap`](../../rosace-widgets/src/template/swap.rs) — the one place the diff's safety gate meets the live registry — calls `registry::register(new)` on `Swappable`, and leaves the running descriptor completely untouched on `Escalate`/`UnknownSite`. The actual re-inflation into widgets happens naturally on the *next* frame: `apply_source_edit` (below) marks the app globally dirty, the normal `Component::build()` cycle runs again, and the `view!` interpreter reads the now-updated registry entry and reconstructs the subtree by calling the same widget constructors the release builder would call — an *inflater*, not a second renderer. There is no tree-surgery step; the reactive rebuild does the work for free.

**5. `apply_source_edit` is the single platform-agnostic entry point every transport calls.** [`dev_reload::apply_source_edit(file, src)`](../../rosace/src/dev_reload.rs) runs `apply_reload`, and then:
   - if anything swapped, forces a **global** dirty flag (`rosace_state::reset_to_global_dirty()`) and `request_frame()` — a per-component dirty mark isn't enough because the affected subtree's shape changed, not just its state;
   - if anything needs a restart, logs that the edit needs a recompile;
   - if the file didn't parse at all (mid-keystroke), does nothing and waits for the next save.

   Only the **transport** — how edited source *reaches* this function — differs per platform: a filesystem watcher thread on desktop ([`spawn_dev_watcher`](../../rosace/src/dev_reload.rs), reusing [`rosace-hot-reload::FileWatcher`](../../rosace-hot-reload/src/watcher.rs)), a WebSocket the dev server pushes to on web, and a length-framed TCP socket ([`serve_hot_reload_socket`](../../rosace/src/dev_reload.rs)) that `rosace-ffi` opens on an Android/iOS device for `rsc dev --target android/ios` to push into over `adb forward`/the simulator's shared localhost. Every transport ends at the same `apply_source_edit` call — this is deliberately the one seam a new platform transport has to plug into.

**6. Assets hot-swap through the same watcher by a simpler path.** A non-`.rs` file under `assets/` (image/font/data) triggers [`apply_asset_change`](../../rosace/src/dev_reload.rs): the decoded-image cache is cleared and a global-dirty repaint is requested, so the next paint re-reads the file from disk. No template diffing involved — swap the cache, repaint.

### Tier 2 — native dylib swap (desktop / simulator, an accelerator)

**7. `rsc dev` (desktop, default, no flags) reaches for Tier 2 before falling back to Tier 1.** [`rosace-cli`'s `tier2::run`](../../rosace-cli/src/commands/tier2.rs) — see [cli.md](cli.md) for the CLI-side orchestration — builds the app's own crate as a `dylib` (`-C prefer-dynamic`), builds a small **host** binary that links the *same* shared `rosace` dylib, launches the host, and watches `src/`: every edit rebuilds only the app's dylib, and the running host notices and hot-swaps it — meaning `build()` bodies, event handlers, and any other compiled logic reload live, not just literal data.

**8. The host/module split only works because host and module share one copy of every static.** If `rosace-state`'s statics (the atom store, dirty set) were statically linked into both the host and the reloadable module separately, each would get its own copy and state would neither survive a reload nor flow between them. The fix is architectural: `rosace` is built once as a shared dylib, and both the host and every module dynamically link *that one instance* — so there is exactly one atom store for the whole process, owned by the host, read and written by whichever module is currently loaded.

**9. The module's one export is `__rsc_dev_root`, a plain `extern "C" fn() -> Box<dyn Component>`.** [`rosace::dev_host`](../../rosace/src/dev_host.rs) — the actual Tier-2 host, compiled only under `rsc-hot` on native desktop — `dlopen`s the module dylib via `libloading` and looks up that one symbol. This is **simpler than what `.steering/HOT_RELOAD.md`'s plan describes**: the plan calls for a versioned `__tzr_module_vN(reg: *mut ModuleRegistry)` entry point generated by an `#[rsc::module]` proc-macro, with explicit ABI-version negotiation. What's actually implemented is a single unversioned symbol returning a boxed `Component` directly — no `ModuleRegistry`, no proc-macro, no version check. It's the same core mechanism (one shared dylib, `dlopen` the module, swap the root) with less ceremony; if you're implementing multi-module (per-screen) hot-reload or the version-negotiation the plan describes, that layer does not exist yet.

**10. The reload is strictly ordered: load-new → swap → drop-old.** [`dev_host::run`](../../rosace/src/dev_host.rs)'s paint closure, on noticing the module's mtime changed: (a) copies the dylib to a fresh temp path and `dlopen`s *that* — a fresh filename every generation, because `dlopen` caches by path and re-opening the same path would silently return the stale code; (b) calls `engine.set_root(new_root)`, which drops the OLD element tree and every closure (`Arc<dyn Fn>`) it held *before* the old library is dropped; (c) only then drops the old `libloading::Library`, unloading its code. This ordering is what prevents a segfault from an outgoing module's closure being invoked after its code is gone — a background poller thread wakes the loop on mtime change, but the swap itself always happens on the loop's own paint call, never concurrently with event dispatch.

**11. Component state survives a Tier-2 swap the same way it survives an ordinary rebuild.** Atoms live in the shared dylib's state store, keyed by `ComponentId` (DFS position — see [core.md](core.md), D001) — not by which module produced them. As long as the new module's `build()` produces the same tree shape, the same `ComponentId`s land on the same atoms and state is preserved with no special-casing. A tree-shape change (a hook added/removed partway through the swap) shifts IDs for the affected subtree exactly like an ordinary rebuild would — there is no separate "reload identity" concept.

### Tier 0 — hot restart (the universal floor)

**12. When neither tier applies, `rsc dev --watch` supervises a rebuild-and-relaunch loop instead of swapping anything live.** [`run_desktop_watch`](../../rosace-cli/src/commands/dev.rs) owns the app as a child process, rebuilds via [`RebuildRunner`](../../rosace-hot-reload/src/rebuild.rs) on every watched change, and kills+relaunches the child only on a *successful* build — a broken edit leaves the previous working session running. State survives across this restart only for `ctx.state_permanent(...)` atoms (D114/D121), because those are the ones actually persisted to disk (`rosace-storage`, SQLite); ordinary `ctx.state(...)` atoms are in-memory and reset. This is also the implicit fallback for platforms Tier 1/2 can't fully cover — the plan's `reload`/`session` persistence tiers (in-memory-but-survive-a-restart) remain documented no-ops, since nothing currently snapshots and rehydrates non-permanent atom values across a real process restart.

## Key types

- [`Template` / `TemplateNode` / `TemplateKey` / `PropValue` / `StaticValue`](../../rosace-widgets/src/template/descriptor.rs) — the data form of a `view!` tree: shape + literals as data, `{expr}`s as positional holes.
- [`template::{register, get, find_running, snapshot}`](../../rosace-widgets/src/template/registry.rs) — the process-global, dev-only map from site key to the currently-running `Template`.
- [`diff` / `TemplateDiff` / `EscalationReason`](../../rosace-widgets/src/template/diff.rs) — the safety gate: `Unchanged` / `Swappable` / `Escalate(reason)`.
- [`apply_swap` / `SwapOutcome`](../../rosace-widgets/src/template/swap.rs) — diffs against the registry and installs a safe swap.
- [`apply_reload` / `ReloadReport`](../../rosace-widgets/src/template/reload.rs) — re-parses an edited file and applies every safe swap in it; reports `applied`/`unchanged`/`unknown`/`escalations`.
- [`dev_reload::apply_source_edit`](../../rosace/src/dev_reload.rs) — the one platform-agnostic Tier-1 entry point every transport (filesystem watcher, WebSocket, mobile socket) calls.
- [`dev_host::run`](../../rosace/src/dev_host.rs) — the Tier-2 host: `dlopen`s the module dylib, holds the window/`FrameEngine`, swaps the root `Component` on change.
- [`rosace_hot_reload::{FileWatcher, Debouncer, RebuildRunner}`](../../rosace-hot-reload/src/lib.rs) — the shared polling watcher (200ms interval, 100ms debounce) and `cargo build` runner used by every tier's transport.
- [`rosace-cli`'s `tier2::run`](../../rosace-cli/src/commands/tier2.rs) — builds the module dylib + host binary and drives the Tier-2 dev loop; see [cli.md](cli.md).

## Why it's like this

- **D102 — stable host + reloadable UI modules, layered with D103's template model.** The three-tier structure exists because no single mechanism covers every platform: iOS device forbids loading new code at all, and wasm has no stable dynamic linking, so *only* a data-only reload (Tier 1) can be universal; Tier 2 is explicitly an accelerator for platforms that allow `dlopen`, not a prerequisite. See [D102 in DECISIONS.md](../DECISIONS.md).
- **D103 — the declarative `view!` layer this all depends on.** Tier 1 is only possible because `view!` produces a real data description of structure separately from compiled `{expr}` holes; without that split there would be nothing to diff or swap that isn't just "the whole compiled program." See [D103 in DECISIONS.md](../DECISIONS.md).
- **D125 — the template descriptor's concrete shape (widget/prop names as strings, positional holes only).** Chosen so the descriptor is inert data with a trivial future JSON form (a serde impl is a named, not-yet-taken deferral) and so hole order only needs to be stable within one dev recompile of the same source — it does not need cross-build stability. See [D125 in DECISIONS.md](../DECISIONS.md).
- **D091 — the same per-node retained-state model that undergirds the widget tree (see [widget-protocol.md](widget-protocol.md)) is what lets a Tier-2 swap preserve state for free** — atoms and render-tree node state are keyed by position, not by which compiled module produced them, so a fresh module's rebuild naturally lands on the same identities.
- **Strict load→swap→drop ordering (not swap→drop→load) is a direct, deliberate answer to a real crash class**: a dangling `Arc<dyn Fn>` whose vtable lives in an unloaded dylib is a segfault waiting to happen, not a theoretical concern — see the ordering rationale in [`.steering/HOT_RELOAD.md`](../../.steering/HOT_RELOAD.md).

## Gotchas & invariants

- **The plan document and the landed code diverge on the Tier-2 ABI — trust the code.** `.steering/HOT_RELOAD.md` describes a versioned `__tzr_module_vN(reg: *mut ModuleRegistry)` boundary generated by an `#[rsc::module]` proc-macro. What's actually built is a single unversioned `__rsc_dev_root() -> Box<dyn Component>` export, hand-written into the scaffold (no macro, no registry, no version negotiation). If you're extending Tier 2, the registry/versioning/multi-module layer is a real gap, not just unread documentation.
- **A hot-reload push that isn't a safe data edit does not apply anything — it reports and waits.** A new hole, a retargeted hole, or a brand-new `view!` site (unknown to the registry) all leave the running app exactly as it was; nothing partially applies. Don't expect a "half swapped" state — `EscalationReason`/`unknown` sites are all-or-nothing per site.
- **Unparseable source (mid-keystroke) is silently ignored, not treated as an error.** `ReloadReport::ignored()` exists specifically so a save-on-every-keystroke editor doesn't spam "reload failed" for a momentarily invalid file; it just waits for the next parseable save.
- **Only `ctx.state_permanent(...)` atoms survive a Tier-0 restart.** Ordinary `ctx.state(...)` atoms are in-memory only and reset on relaunch — if a hot-restart loses state you expected to keep, check whether that atom should be `state_permanent` (see [core.md](core.md) and D114/D121).
- **Release builds carry none of this.** The `rsc-hot` cargo feature gates the entire template registry, the interpreter, and the dylib host out of release builds by construction (`cfg!(feature = "rsc-hot")` in the macro) — there is no runtime flag that could accidentally leave hot-reload machinery in a shipped binary.
- **Tier 2 requires host and module to share identical `RUSTFLAGS`/`CARGO_TARGET_DIR`.** A mismatch causes cargo to re-fingerprint and rebuild everything on every single edit — see the `hot_env` note in [cli.md](cli.md).

---

This closes out the architecture book's planned chapters — see [Architecture Overview](README.md) for the full map, or the [Guide](../guide/README.md) if you're building an app rather than contributing to the framework (its own [Hot Reload](../guide/hot-reload.md) chapter covers the same ground from an app author's point of view).
