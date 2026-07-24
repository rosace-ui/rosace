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

## Next candidates
- Live-verify `TextInput.scroll_x` in a real windowed app (scaffold via `rsc new`).
- Live-verify net/ws hooks (`use_query`/`use_websocket`) — deferred "check later".
- `rosace-style` integration (explicitly deferred by user).
