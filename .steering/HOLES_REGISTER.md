# Holes Register — gaps found per phase (swept 2026-07-24)

Living list of integration gaps ("holes") discovered by sweeping the phase
files and verifying real call sites. Distinguishes genuine unclosed work from
deliberate, named deferrals. Update as holes are closed or new ones surface.

## Method
- Swept `PHASE_*.md` + `CRATE_CONTRACTS.md` for deferred/TODO/not-wired markers.
- Verified each suspect crate's *external* call sites (used by others vs.
  only re-exported from the umbrella `rosace` crate).
- Recurring project pattern confirmed: **crates built but never wired.** Check
  real call sites, not crate existence.

## Built-but-never-wired (structural)
| Item | Phase | State (2026-07-24) |
|------|-------|--------------------|
| `rosace-forms` | P28 | ✅ CLOSED — wired into text_edit/input/area + engine |
| `state_permanent` | P31 | ✅ CLOSED — wired through core/persist/context, used by cli/dev |
| `rosace-net` hooks (`use_query`, `use_network_status`) | P30 | ⚠️ Reachable via `rosace::net::*`; unit-tested against real TCP; **never live-verified in a running app** (no on-screen Loading→Loaded proof). DEFERRED "check later" (user, 2026-07-24) |
| `rosace-ws` (`use_websocket`) | P30 | ⚠️ Same as above — exists + reachable, not live-verified |
| `rosace-style` | pre-P23 | ❌ 37-line crate, only re-exported from umbrella, no consumer. Explicitly deferred by user |

## Named feature deferrals (deliberate, still open)
- **Mobile native-host IME** (P28 Step 6) — desktop IME works; mobile cannot type via OS IME.
- **Magnifier loupe** for text selection (P28 Step 7) — needs P27 offscreen/shader.
- ✅ **`TextInput.scroll_x`** (P28) — CLOSED 2026-07-24. Was declared but never
  assigned (stuck at 0.0). Now computed each paint from the caret, written
  back via new `PaintCtx::set_scroll_x`, `-scroll_x` baked into `boundary_x`
  (so caret/selection/spans/IME/hit-test all shift as one), content clipped
  to the field via PushClip/PopClip. Headless-tested (overflow→scroll_x>0+clip;
  short value→0). NOT yet live-verified in a windowed app.
- ✅ **Mouse-drag text selection** (P28) — Already CLOSED. `engine.rs` has
  `text_drag` (D116 Step 3) + `handle_drag` (Step 7); the register's earlier
  "not implemented" note was based on stale mid-phase P28 text.
- **P32 widgets not started**: InteractiveViewer, DatePicker/TimePicker, RichText, emoji, italic axis.
- **P27 GPU migration** — scoped; CPU `tiny-skia` DrawText/BlitRgba commands still present.
- **Web/wasm backends** — net (P30), storage `permanent` tier via IndexedDB (P31), WebGPU presenter (P27).
- **Push notifications** — real APNs/FCM blocked on account access (P29).

## Deferred to v1.0 (do not touch now)
- CJK/complex-script shaping (D014, `FallbackShaper` one-glyph-per-char limit).
- GraphQL / gRPC clients (P30 — REST + WebSocket cover the common case).

## Closed this session (2026-07-24)
- `TextInput.scroll_x` horizontal scroll-into-view — wired + headless-tested.
- Confirmed mouse-drag selection was already done (no work needed).

## Workspace-wide dead-code/debug-plug sweep (2026-07-30)

A full-workspace sweep (compiler warnings, `#[allow(dead_code/unused_*)]`
sites, orphaned files, stray `println!`/`eprintln!`/`dbg!`, ad-hoc debug
env-vars, commented-out code, `TODO`/`FIXME`/`XXX`/`HACK`) across all ~41
crates. Overall the codebase came back clean — no TODO/FIXME/HACK anywhere,
no commented-out code, no ad-hoc debug env-vars beyond the known real ones
(`ROSACE_LOG`, `ROSACE_TRACE`, `ROSACE_CPU_SHAPES`, etc.). What follows is
everything that wasn't already clean.

### 🔴 HIGH PRIORITY — needs investigation, not fixed here
- **13 pre-existing failures in `rosace`'s engine test suite**, reproducing
  identically on the commit *before* this session's changes (confirmed via
  `git stash`) and in isolation single-threaded (so not pure test-order
  interference, though the failure count does shift with `--test-threads`,
  suggesting some interference on top of a real underlying issue). Repro:
  `cargo test -p rosace --lib -- --test-threads=1`. Failing names cluster
  around typing/arrow-key editing, form validity, and scroll-into-view —
  e.g. `arrow_left_then_insert_lands_in_the_middle_not_appended_at_the_end`,
  `typing_in_a_bound_field_updates_the_forms_live_validity`,
  `wheel_scrolling_away_from_the_caret_is_not_snapped_back_by_scroll_into_view`.
  This reads like a real, current regression in the text-editing/forms
  pipeline (or a broken shared test fixture) — worth a dedicated session,
  not folded into a cleanup pass. **Not caused by, and not fixed by, any
  commit in this session.**

### 🟡 MEDIUM — flagged, deliberately not touched
- **`rosace-hot-reload/src/watcher.rs:21`** — `FileWatcher`'s `sender` field
  is `#[allow(dead_code)]` with no comment explaining why it's kept-but-
  unused. Plausible reason (holds a channel sender open so the receiver
  doesn't see a spurious disconnect) but unconfirmed — worth a second look.
- **`rosace-render/src/canvas.rs:519`** — `grow_segment`'s doc comment says
  code that skips a GPU pipeline "must call this before rasterizing... or
  `cut_segment` will silently drop its pixels," implying a call site that
  should exist but currently doesn't. Reads like a latent gap rather than
  confirmed-intentional dead code — needs someone who knows this path to
  confirm whether a call site is actually missing.
- **`rosace-page/`** — an empty, no-commits nested `.git` clone sitting in
  the workspace root (remote `rosace-ui/rosace-page.git`, no `Cargo.toml`,
  no `src/`). It's explicitly listed in `.gitignore` (not accidentally
  untracked), which reads as intentional local scratch space rather than
  junk — left alone. **Ask the user directly** what this is for and whether
  it should stay, be finished, or be deleted; don't guess.
- **Library/dev-tooling code bypassing the `rosace-trace` logging
  framework** — `rosace-hot-reload/src/rebuild.rs`, `rosace/src/dev_host.rs`,
  `rosace/src/dev_reload.rs`, `rosace-ffi/src/engine.rs:73`,
  `rosace/src/lib.rs:171-173`, and `rosace-widgets/src/tree/{column,row}.rs`
  all use raw `println!`/`eprintln!` for legitimate developer-facing
  messages (hot-reload status, a debug-only unbounded-`Expanded` warning)
  instead of `info!`/`warn!`. Not leftover scaffolding — these read as
  intentional — but architecturally inconsistent with the rest of the
  codebase. A consistency pass (migrate to the trace macros) would be
  low-risk but touches several files; not done as part of this cleanup
  since it wasn't asked for and isn't itself a defect.

### ✅ Clarified, not actually a hole
- **"hooks"** — an earlier project-memory note mentioned "forms, RichText,
  hooks" as a recurring built-but-never-wired pattern, but no `use_hook`/
  `hooks::`-named artifact exists anywhere in the workspace today. Traced
  it: this almost certainly refers to `rosace-animate`'s `use_animation`/
  `use_spring` (the hook-style animation API — see its crate contract in
  CRATE_CONTRACTS.md), which is confirmed actively wired into
  `rosace-widgets`/`rosace-platform`/`rosace-nav-anim`. Not a gap.
- **`rosace-forms` / `RichText`** — both confirmed fully wired (engine.rs
  Form/FormField dispatch, widget builder methods, umbrella re-exports).
  The "built ahead of integration" phase for these has already closed.

### Removed this session (confirmed dead, not "built ahead of schedule")
- `rosace-anim` — a whole duplicate animation crate (744 lines), zero
  consumers anywhere except its own dead re-export. This was flagged as an
  **open, unresolved decision** in the 2026-07-08 `CRATE_CONTRACTS.md` audit
  ("remove, or find/state its purpose") — resolved now: removed, with the
  user's explicit go-ahead given it's a whole-crate deletion. See D126 /
  CRATE_CONTRACTS.md.
- `rosace-platform/src/web.rs` — the original pre-winit single-frame web
  MVP (`putImageData`, no event loop), already documented as dead in
  `docs/architecture/platform-and-app-loop.md`, now triply superseded.
- `rosace-widgets/src/tree/mod.rs`'s `clamp(Constraints, Size)` — zero
  call sites workspace-wide.
- `rosace-widgets/src/tree/dropdown.rs`'s `Rect` import — unused; had been
  silenced with a decoy `use Rect as _RectUsed` alias instead of removed.
- `rosace-cli/src/commands/tier2.rs`'s trailing `child.wait()` — provably
  unreachable (every loop exit is an explicit `return`; `try_wait()` inside
  `app_closed()` already reaps on every path that observes the exit).
- `rosace/src/engine.rs`'s `probe_offsets_frame_by_frame_after_typing_
  from_scrolled_top` test — a self-labeled "TEMP diagnostic probe" with
  zero assertions, superseded by the real regression test right above it.

## Next candidates
- Live-verify `TextInput.scroll_x` in a real windowed app (scaffold via `rsc new`).
- Live-verify net/ws hooks (`use_query`/`use_websocket`) — deferred "check later".
- `rosace-style` integration (explicitly deferred by user).
- **Investigate the 13 failing engine tests** (see HIGH PRIORITY above) — likely the single most important open item in this file right now.
- Confirm/fix the `watcher.rs` unused `sender` field and `canvas.rs`'s `grow_segment` possible-missing-call-site (both MEDIUM above).
- Ask the user what `rosace-page/` is for.
