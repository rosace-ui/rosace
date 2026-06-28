# TEZZERA — PHASE 11
> Developer Experience: Macros, Analyze, Snapshot CLI, DX Polish
> Status: COMPLETE
> Target: v0.1 release readiness — full macro layer, tzr analyze, tzr snapshot, DX closure

---

## PHASE 11 GOAL

A developer using TEZZERA gets a complete, polished DX layer:
- Proc-macros reduce boilerplate to near zero (`#[component]`, `#[state]`, `view!`)
- `tzr analyze` surfaces workspace health (crate count, API surface, warning count)
- `tzr snapshot` runs demos and saves PNG golden files for visual regression
- All remaining D052 CLI commands land

---

## EXIT CRITERIA

```
□ #[state] macro expands correctly (derive-style, Atom integration)
□ view! macro handles nested children + props, expands to builder calls
□ tzr analyze reports crate count, member list, and warning presence
□ tzr snapshot runs an example binary and saves output PNG
□ All Phase 11 crate tests pass (zero failures, zero warnings)
□ phase11_demo.png written at 1400×900
□ PHASE_11.md marked COMPLETE
```

---

## STEP-BY-STEP PLAN

### Step 1 — `#[state]` proc-macro (`tezzera-macros`)

Add `#[state]` to `tezzera-macros`. It is a derive-style attribute that wraps a
plain struct field declaration and emits an `Atom<T>` binding.

```rust
// Input
#[state]
pub count: i32 = 0;

// Expands to
pub count: tezzera_state::Atom<i32> = tezzera_state::Atom::new(0);
```

- Add `state.rs` to `tezzera-macros/src/`
- Export `#[proc_macro_attribute] pub fn state(...)` from `lib.rs`
- Parse: extract field name, type, default expr
- Emit: replace with `Atom<T>` binding
- Tests: expansion produces `Atom<T>`; missing default expr gives friendly error

### Step 2 — `tzr analyze` subcommand (`tezzera-cli`)

Provide workspace health analytics without running cargo.

`PackageConfig` for this: none — reads `Cargo.toml` directly.

```
$ tzr analyze
Workspace: tezzera
  Crates:  27
  Members: tezzera-core, tezzera-state, … (full list)
  Status:  OK
```

Implementation:
- `commands/analyze.rs`: `AnalyzeOptions` (verbose flag), `AnalyzeReport { member_count: usize, members: Vec<String>, workspace_name: String }`
- Parse workspace `Cargo.toml` manually (no serde) — read `[workspace] members = [...]`
- `run_analyze(opts) -> AnalyzeReport`
- Print report to stdout
- Wire into `main.rs` match arm `"analyze"`
- Tests: `parse_members` extracts members from a fixture string; `AnalyzeReport::summary()` formats correctly

### Step 3 — `tzr snapshot` subcommand (`tezzera-cli`)

Run an example binary and move/copy its PNG output to a snapshot directory.

```
$ tzr snapshot --example phase10_demo --out snapshots/
Running: cargo run -p tezzera-examples --bin phase10_demo
Saved: snapshots/phase10_demo.png
```

Implementation:
- `commands/snapshot.rs`: `SnapshotOptions { example: String, out_dir: String }`
- `run_snapshot(opts) -> CommandResult`
  - runs `cargo run -p tezzera-examples --bin <example>`
  - finds `<example>.png` in cwd
  - copies to `<out_dir>/<example>.png`
- Wire into `main.rs` match arm `"snapshot"`
- Tests: `SnapshotOptions::from_args` parses `--example` and `--out`; default out_dir = "snapshots"

### Step 4 — Phase 11 showcase

- `tezzera-examples/src/bin/phase11_demo.rs`
- 1400×900 PNG, 4 panels:
  1. Macros — `#[component]` expansion diagram, `#[state]` expansion diagram, `view!` AST → builder chain
  2. Analyze — `AnalyzeReport` display (member list as chips, count stat, status badge)
  3. Snapshot — `tzr snapshot` flow diagram, before/after PNG comparison mockup
  4. DX Summary — all 27 crates grouped by layer, Phase 1→11 timeline bar

---

## APPROVED DEPENDENCIES

- No `serde` for Cargo.toml parsing — manual string split
- No `proc-macro2` version bump — stay at existing version in workspace
- No `cargo_metadata` — parse Cargo.toml manually
- No new external crates

## DO NOT

- DO NOT implement real Atom integration in `#[state]` — emit the correct tokens, runtime linking deferred
- DO NOT add `tzr publish` — out of scope
- DO NOT add a test runner wrapper beyond `tzr test` (already done)
- DO NOT parse full TOML in analyze — only extract the `members` array
