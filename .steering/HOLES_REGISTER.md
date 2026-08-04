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
| `rosace-net` hooks (`use_query`, `use_network_status`) | P30 | ✅ CLOSED 2026-08-04 — live-verified via `examples/src/bin/http_demo.rs` (its own pre-existing exit-bar demo, just needed a broken `examples/Cargo.toml` build config fixed to run — see below). Real HTTPS request to `httpbin.org/json` through `use_query` over rustls/ureq: went Loading → Loaded, rendered the real response (`HTTP 503` — httpbin's own transient error, not ours; proves the round-trip + state transitions + render all work regardless of the upstream status code) |
| `rosace-ws` (`use_websocket`) | P30 | ✅ CLOSED 2026-08-04 — live-verified via `examples/src/bin/ws_demo.rs`. Real `wss://ws.postman-echo.com/raw` connection over tungstenite: reached `Open`, sent messages once/second, received real echoes back, UI updated live (32 echoes rendered and counting) |
| `rosace-style` | pre-P23 | ❌ 37-line crate, only re-exported from umbrella, no consumer. Explicitly deferred by user |

## Named feature deferrals (deliberate, still open)
- **Mobile native-host IME** (P28 Step 6) — desktop IME works; mobile cannot type via OS IME.
- **Magnifier loupe** for text selection (P28 Step 7) — needs P27 offscreen/shader.
- ✅ **`TextInput.scroll_x`** (P28) — CLOSED 2026-07-24, LIVE-VERIFIED
  2026-08-04. Was declared but never assigned (stuck at 0.0). Now computed
  each paint from the caret, written back via new `PaintCtx::set_scroll_x`,
  `-scroll_x` baked into `boundary_x` (so caret/selection/spans/IME/hit-test
  all shift as one), content clipped to the field via PushClip/PopClip.
  Headless-tested (overflow→scroll_x>0+clip; short value→0). Live-verified
  in the real running showcase app: typed a string long enough to overflow
  the field — content correctly scrolled to keep the caret visible (text
  clipped from the left edge, caret visible at the right), full value still
  correctly captured in app state via `on_change`.
- ✅ **Mouse-drag text selection** (P28) — Already CLOSED. `engine.rs` has
  `text_drag` (D116 Step 3) + `handle_drag` (Step 7); the register's earlier
  "not implemented" note was based on stale mid-phase P28 text.
- ✅ **P32 widgets** — CONFIRMED 2026-07-31 (user). InteractiveViewer,
  DatePicker/TimePicker, RichText are built and done — the earlier "not
  started" note was stale. Emoji and italic axis are separate, still-open
  font-level items (emoji: bundling Noto for web, see
  `project_web_fonts_emoji` memory; italic axis: not tracked further here).
- **P27 GPU migration** — CORRECTED 2026-07-31 (user): this is not an
  unfinished CPU-fallback path. The intended architecture is GPU-shapes (SDF
  quads) for shapes plus `tiny-skia` scoped specifically to rasterization
  (text glyphs, images) that feeds the GPU compositor as textures — that's
  the design, not leftover CPU-only work (confirmed during the 2026-07-30
  workspace cleanup, when tiny-skia removal was considered and rejected for
  exactly this reason).
- **Web/wasm backends** — net (P30), storage `permanent` tier via IndexedDB (P31); WebGPU presenter now landed (D126) for the main frame path.
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

### ✅ RESOLVED 2026-07-31 — the 13 pre-existing engine test failures
Bisected with a `git worktree` (not the main checkout) to `a34e7e8`
(2026-07-24) — already known and named in `project_dev_release_state.md` as
"test-isolation/global-state leakage in the DevTools WIP." Root cause:
`devtools_fab_enabled()` checked only `cfg!(debug_assertions)`, true for
`cargo test` too — every headless test engine got a real DevTools FAB
overlay injected, and the process-global `DEVTOOLS_OPEN`/`DEVTOOLS_TAB`
atoms it reads/writes leaked across tests sharing one test-binary process.
Fixed: `devtools_fab_enabled()` also checks `!cfg!(test)`, scoped to
`rosace`'s own test binary only — a downstream app's real debug build is a
different compilation unit and is completely unaffected. `rosace` lib tests
now 64/64 passing, confirmed stable across 4 repeated runs. Full workspace
`cargo test --workspace --no-fail-fast` clean.

### 🟡 MEDIUM — flagged, deliberately not touched
- ✅ **`rosace-hot-reload/src/watcher.rs:21`** — RESOLVED 2026-08-04.
  Traced it: the background thread `move`s its OWN clone of `tx` and uses
  that to send every event; the struct's separate `sender` field was never
  read anywhere (confirmed by grep — no `self.sender` call site exists),
  and the "keeps the channel open" theory doesn't actually hold either,
  since (a) the thread's own clone already keeps the channel alive for the
  thread's whole lifetime regardless, and (b) no consumer anywhere
  distinguishes a disconnected channel from an empty one (`while let
  Ok(e) = rx.try_recv()` treats both identically). Genuinely dead —
  removed the field.
- ✅ **`rosace-render/src/canvas.rs:519`** — RESOLVED 2026-08-04. Verified
  against the current code, not just the comment's claim: every
  `DrawCommand` variant in `play_picture`'s GPU branch now pushes its own
  typed `CanvasFrameItem` (Shader/Backdrop/Glyphs/Image) instead of falling
  through to the generic CPU-segment bbox tracker — confirmed by reading
  all 13 match arms. No variant left needs `grow_segment`, so the "no
  caller" claim is accurate today, not stale. Genuinely a deliberate
  extension seam for a future GPU-less command, not a latent gap — left
  as `#[allow(dead_code)]` with its existing comment, no code change
  needed.
- ✅ **`rosace-page/`** — RESOLVED 2026-07-31. Asked the user directly: the
  project has been moved to its own outer repo (its remote,
  `rosace-ui/rosace-page.git`, has one real commit — this local clone had
  just never fetched it). Confirmed fine to remove; deleted the local
  directory and its now-stale `.gitignore` entry.
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

## Closed this session (2026-08-04)
- `TextInput.scroll_x` — live-verified in the real showcase app (see above).
- `rosace-net`/`rosace-ws` hooks — live-verified via their own pre-existing
  exit-bar demos (see above), which required fixing a broken build first:
  `examples/Cargo.toml` used `version.workspace = true` etc. with no
  `[workspace]` table of its own and no membership in the root workspace
  (it's supposed to be its own separate repo per 2026-07-29's nesting
  decision) — `cargo build` failed outright. Fixed by giving it literal
  `[package]` metadata values and an empty `[workspace]` table instead of
  inheriting. Pure build-config fix, no app code touched.
- `rosace-hot-reload/src/watcher.rs`'s `sender` field and
  `rosace-render/src/canvas.rs`'s `grow_segment` — both resolved, see the
  MEDIUM section above (one removed as genuinely dead, one confirmed
  intentional and left as-is).

## Next candidates
- `rosace-style` integration (explicitly deferred by user).
- Nothing else currently open in this register.
