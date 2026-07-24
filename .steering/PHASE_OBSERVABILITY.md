# Observability & DevTools Track (O1–O7) — D123

> Status: O1 COMPLETE 2026-07-18 (flight recorder landed 2026-07-16;
> Perfetto/Chrome trace JSON export — the missing exit-bar piece — added
> 2026-07-18, `rosace_trace::to_chrome_trace_json` /
> `RingBufferSubscriber::export_perfetto_json`, unit-tested).
> O2 COMPLETE 2026-07-19: the full in-app element inspector.
>   - Read-only seam: `RenderTree::inspect()` (plain-data `InspectNode` per
>     node) + `RenderTree::pick(x,y)` (deepest node under a point).
>   - `PaintCtx::child` now records every painted widget node's world-space
>     `cached_rect` (previously only element/cache-boundary nodes had one,
>     so the picker could only select the coarse Scaffold) — the fix that
>     made fine-grained per-widget selection work.
>   - Pure logic in `rosace-devtools::element_inspector`: `ElementInspector`
>     (enabled/hover/selected + toggle/set_hover/select/on_escape),
>     `node_rect`, `panel_lines` (Flutter-style W×H / rect / constraints /
>     role readout; name falls back to semantic role, size to the rect when
>     tag/cached_size absent — the widget-node case). 40 unit tests.
>   - Engine wiring (`rosace/src/engine.rs`): `Key::F12` toggles; while on,
>     the inspector OWNS the pointer (hover→pick→highlight, click→pick→
>     select, Esc→deselect-then-close), app input frozen; chrome (hover
>     outline, selected outline+tint, corner panel) drawn on the overlay
>     layer above everything via `draw_dev_inspector`. `ROSACE_DEVTOOLS=1`
>     boots it already open (WM-eats-F12 fallback / first-frame inspect).
>   - Live-verified: highlight box hugs the picked `TextInput` exactly,
>     panel reads `TextInput "Your name" / size 300 × 40 / rect / editable`.
> O3–O7 scoped, not started.
> A SEPARATE track from the feature roadmap (Phases 1–32). Its own
> numbering (O1…O7) because it's cross-cutting infrastructure, not a
> linear feature phase.
> Decision: **D123** — an in-app observability + DevTools experience that
> also matures `rosace-core` into a "build-on-able" platform via read-only
> introspection seams. Read `.steering/DECISIONS.md` D123 first — it holds
> the governing constraints.

## The one rule (from a real past disaster)

**Tracing is EVENT-DRIVEN, never frame-driven.** An earlier attempt hung
the app and printed every frame because nothing classified event
frequency. No visible sink (console/overlay) and no default recorder ever
receives high-frequency events (`AtomRead`, `FrameStart`/`FrameEnd`,
`PaintRegion`, `LayoutStart`). Enforced by `RosaceTrace::is_high_frequency()`,
structurally — not by remembering to be careful.

## Deferred, on purpose (named, not dropped)

- **Live property editing** of a selected widget — gated on hot reload
  (D102). Widgets are immutable value structs rebuilt every frame; there
  is nothing persistent to edit. Atom-backed props are already editable
  via the state inspector. Arbitrary static props (`.padding(8)`) need a
  per-node override seam that only makes sense once `build()` can re-run
  with patched values. Ships WITH hot reload, not in this track.
- **Unified capability/extension API** + **embedding entry point** — the
  other two core "give-out" gaps. Not required for read-only tooling;
  scoped as follow-on once the tooling shows which seams matter.

## Architecture direction: an EXTERNAL DevTools tool (decided 2026-07-19, build later)

**Decision (user, 2026-07-19):** the mature observability experience is an
**external visualization tool**, NOT one big in-app overlay. Reasoning: the
scope is broad — live tracing, networking, and data inspection *and
modification* — and cramming all of that into an on-device (especially
mobile) canvas overlay is the wrong call. The in-app F12 element inspector
(O2) stays as a lightweight on-device convenience; the heavy multi-panel
experience targets an external tool. This mirrors how the mature frameworks
do it (Flutter DevTools over the VM-service protocol, React DevTools over a
bridge): the app exposes its observability data over a transport, an
external app/web page visualizes it and sends mutations back.

**Key insight (user): "all are there, just to wire and output them."** The
DATA already exists as tested structures — the remaining work is a wire +
serialization + external viewer, not new instrumentation:
- **Tracing** — `rosace-trace` flight recorder (O1) + `to_chrome_trace_json`
  already produce the event stream and a Perfetto-loadable export.
- **Element/layout** — `RenderTree::inspect()` returns plain-data
  `InspectNode`s (JSON-ready) already.
- **State (atoms)** — `AtomWrite` events carry `by: ComponentId` causality;
  `rosace-devtools::AtomInspector` already snapshots atom values (unwired).
- **Networking** — `RosaceTrace::{RequestStart, RequestEnd}` already record
  method/url/status/duration/cache/size.

**What the external tool actually needs (the wiring, later):**
1. A **serialization layer** — one JSON (or compact binary) schema unifying
   trace events + inspect snapshots + atom state + network log.
2. A **transport** — a local socket / WebSocket the app opens in debug
   builds (`rosace-ws` already exists), streaming the above out and
   accepting **mutation commands** back (set-atom, later set-prop).
3. An **external viewer** — a separate app or web page (Perfetto covers the
   trace flamegraph for free; the rest is a custom panel set) that connects,
   visualizes, and issues mutations.
4. A **mutation channel** on the app side — atom writes are already the
   reactive control, so "modify data from the tool" is a set-atom-by-id
   command; static-prop editing waits on hot reload (D102), same gate as O6.

**Not now.** Recorded so O3–O5 build toward emitting-over-the-wire rather
than only painting in-canvas, and so the transport/schema is designed once.

**Re: reusing Flutter DevTools (evaluated 2026-07-19, rejected as a whole).**
Flutter DevTools is NOT a generic viewer that "picks up a file" — it attaches
to a live app over the **Dart VM Service Protocol** (JSON-RPC/WebSocket:
`getVM`/`getIsolate`/`getVMTimeline`/…) and reasons about Dart isolates/heap.
Repurposing its widget-inspector/state/network panels would mean impersonating
a Dart VM — large, brittle, semantically wrong. So the external tool is OUR
own. HOWEVER the tracing half IS a real standard: Flutter's timeline uses the
**Chrome Trace Event Format**, the same one Perfetto/`chrome://tracing` read —
exactly what O1's `to_chrome_trace_json` already emits. So: trace timeline →
Chrome-format file → **Perfetto** (drag-drop, framework-agnostic, the right
target); widget/state/network → our own wire+viewer.

## Phases

### O1 — Trace foundation done right (the flight recorder)
The disaster-proof substrate everything else reads. `RosaceTrace` gains a
`category()` (State / Lifecycle / Layout / Frame / Route / Network / Ffi /
Gesture / Render) and `is_high_frequency()` classification. A bounded
ring-buffer "flight recorder" installs by default in DEBUG builds
(near-zero cost, no printing) that EXCLUDES high-frequency events, so the
last N meaningful events are always available after a bug. The console
subscriber excludes high-frequency events unless explicitly asked. Add a
structured export to Chrome/Perfetto trace JSON so an external viewer
gives a pro flamegraph for free. `App::launch` installs the flight
recorder in debug instead of the print-everything path.

Exit: an app runs with the recorder always on and NO measurable per-frame
overhead / NO log spam; after triggering a state change + a network call,
the recorder's snapshot contains exactly those meaningful events (and zero
`AtomRead`/`FrameStart`); the same buffer exports to a Perfetto-loadable
JSON file. Unit-tested classification + a "high-frequency never reaches a
filtered sink" test.

### O2 — In-app DevTools overlay + element picker (the killer feature)
A hotkey-toggled overlay panel built WITH ROSACE's own widgets (ultimate
dogfood + widget stress test). New READ-ONLY `RenderTree::inspect()`
snapshot API (plain data: per-node id, parent, rect, size, kind/label,
semantics role, hit/overlay flags — additive, non-invasive). Element
picker: hover → highlight the node in-app (draw an outline overlay),
click → select it; the selected-node panel shows rect, **size
(Flutter-style W×H readout)**, constraints, semantics, attached hits/
overlays. Pointer-driven tree selection = this picker.

Exit: in a running app, the overlay toggles on a hotkey, hovering
highlights the widget under the cursor, clicking selects it, and the panel
shows that widget's real size and rect — verified live.

### O3 — State inspector + "why did this rebuild?"
Live atom list with values + subscriber sets (the `rosace-devtools`
`atom_inspector` finally gets a UI). **Time-travel scrubbing** — the
inspector already has `travel_to`/`step_back`/`step_forward` in data;
wire it to the overlay. Causality: `AtomWrite` already carries
`by: ComponentId`, so surface the chain — *"Component X rebuilt because
atom Y was set by Z's on_press."* Reads O1's flight recorder for the
event stream, filtered to State/Lifecycle.

Exit: change a value in a running app; the inspector shows the write, WHO
wrote it, and which components rebuilt as a result; scrubbing back N steps
shows the prior state — verified live.

### O4 — Visual layout debugging
Overflow stripes (Flutter's yellow/black bars) drawn ON any widget whose
child exceeds its constraints; a "show all sizes" toggle labelling every
widget's W×H; unbounded-constraint / layout-error surfacing drawn in place
rather than only logged. Reads layout data already computed each frame.

Exit: a deliberately-overflowing layout shows the overflow marker on the
offending widget in a running app; the size-overlay toggle labels widgets
live — verified visually.

### O5 — Frame profiler
Per-frame build / layout / paint / present timing from the `FrameStart`/
`FrameEnd`/`LayoutStart`/`LayoutEnd` events (the `frame_profiler` data
structure exists, gets a UI). FPS + jank detection; which components
rebuilt each frame and why (feeds from O3's causality). Feeds O1's
Perfetto export for offline flamegraphs.

Exit: the profiler overlay shows live per-frame timing broken into
build/layout/paint/present with a rebuilt-components list; a janky frame is
visibly flagged — verified live, and the same data exports to a Perfetto
trace.

### O6 — Live property editing (GATED on hot reload / D102)
The deferred one. When D102 (hot reload/restart) exists, add a per-node
debug-override seam so a selected widget's props can be patched and
`build()` re-run to see the result. Atom-backed props edit the atom
(already possible); static props hit the override map. NOT started until
D102 lands — placeholder here so the track is complete.

Exit (future): select a widget, change its padding/color in the overlay,
see the app update without a manual rebuild.

### O7 — Golden / visual-regression testing (standalone crate)
A new `rosace-golden` crate (third-party-shaped, its own crate so apps opt
in): render any widget/component headless → PNG, diff against a committed
golden, fail on pixel drift beyond a threshold. CI-friendly. Prerequisite:
fix Known Issue #13 (`WidgetApp::render_png` integer overflow). Standalone
because visual regression is a test-time concern, not a runtime one, and
belongs outside the shipped SDK.

Exit: a golden test renders a widget to PNG, stores it, and a subsequent
run diffs against it and fails on an intentional visual change — in CI,
headless.

## Sequencing

O1 is the substrate everything reads — first. O2 (overlay + picker + the
`inspect()` seam) is the killer feature and the biggest single step. O3–O5
are independent panels on top of O1+O2 (any order). O6 waits on D102. O7
is standalone (can be done any time after Known Issue #13's fix).

## Migration Rule

Purely additive. No existing widget, component, or trace API changes
behavior. DevTools is debug-only (the flight recorder compiles out of
release with the rest of the trace system); the golden crate is opt-in.

## UX REDESIGN 2026-07-23 (user) — FAB + tabbed panel, not hotkeys
O2's "hotkey-toggled overlay" is superseded: mobile has no keyboard, so use a
small **FAB** (Next.js/Turbopack-style corner button) → tap opens ONE
alpha/translucent panel with **TABS** (Elements · Network · Logs · Traces),
built with ROSACE's own widgets (matures the widget system). No separate
F11/F12 keys — everything lives in tabs. Same model for a future web inspect.
Wire to external (Chrome) via the trace bus later.

Progress:
- Phase 1 — Tab widgets DONE (tree/tab.rs TabBar made interactive + theme-defaulted
  + animated sliding underline; tree/tabs.rs TabView + Tabs). Meets widget recipe.
- Phase 2 — DevTools FAB: IN PROGRESS (engine-drawn overlay chrome, touch+click).
- Phase 3 — Tabbed DevTools panel using the tab widgets + trace_panel (O5) content.
- Phase 4 — External/Chrome sink off the trace bus.

### Phases 2-3 LANDED 2026-07-23 (compiles + tests green; live-verify pending)
- Phase 2 — DevTools FAB: engine draws a bottom-right accent circle (`</>` glyph,
  `×` when open) as dev-only chrome (`devtools_fab_enabled()`=cfg!(debug_assertions),
  never in release). Tap (MouseDown, touch+mouse) hit-tested in the engine input
  loop BEFORE app input → toggles `devtools_open`. `draw_devtools_fab` + `devtools_fab_rect`.
- Phase 3 — Tabbed panel: `draw_devtools_panel` (translucent bg + tab bar with accent
  underline + content rows, newest-on-top) top-right; tabs = `DEVTOOLS_TABS` [All, Logs,
  Network] from rosace-devtools::trace_panel. `TracePanel::rows_for(snapshot, tab, max)`
  filters by category per tab. Tab taps hit-tested in engine input loop → `devtools_tab`.
  Overlay pass runs in debug builds so the FAB is always visible (cost: overlay redraw
  each debug frame — optimize later via retained-overlay).
- HONEST SCOPE: the panel is drawn with raw DrawCommands (structured like TabBar), NOT
  yet through the full widget pipeline (real `Tabs` widget in the render tree). Rendering
  DevTools via actual widgets + overlay hit-testing = the "mature widget system" follow-up.
  F11/F12 still exist as extra shortcuts (fold into tabs later). LIVE-VERIFIED 2026-07-23 (screenshot): FAB (× when open) bottom-right, translucent panel top-right with All·Logs·Network tabs (All underlined accent), rows show NET →/← + LOG ERROR/WARN/INFO newest-on-top from the bus. ROSACE_DEVTOOLS=1 boots panel open. Element inspector (F12) also visible = future Elements tab.

### WIDGET-BASED DevTools LANDED 2026-07-24 (the mature rewrite)
Per user ("it's an overlay → it's a widget → already tracked; don't over-engineer"): DevTools rebuilt as REAL widgets injected as a normal OverlayEntry — NOT raw engine chrome. `rosace-devtools/src/devtools_ui.rs`: `devtools_overlay(rows) -> OverlayEntry(LayerPosition::Fill, Stack{ Positioned FAB bottom-right, if open Positioned panel top-right })`. FAB = `FloatingActionButton` widget (own elevation shadow + press anim). Panel = `Container` + `Column`{ `TabBar`(All/Logs/Network, interactive+animated) + `ScrollView`(Text rows) }. State = GlobalAtoms `DEVTOOLS_OPEN`/`DEVTOOLS_TAB` (AtomId 9101/9102); FAB/tab on_press set atom + reset_to_global_dirty+request_frame. Engine injects it each paint (debug) by chaining `devtools_entry` into the overlay iteration (same path as build_overlays/context-menu) → laid out, painted, HIT-TESTED, damage-tracked like any overlay. REMOVED all raw chrome: draw_devtools_fab/panel, devtools_fab_rect/panel_rect, manual hit-testing, engine fields devtools_open/tab/pressed_at. Kept devtools_fab_enabled()=cfg!(debug_assertions). ROSACE_DEVTOOLS=1 sets DEVTOOLS_OPEN atom. Compiles; LIVE-VERIFIED render (screenshot): FAB w/ real shadow bottom-right, panel top-right (All tab underlined, NET/LOG rows). Fixes prior complaints (shadow/centering/lag/selection) BY CONSTRUCTION via the widget system. rosace-devtools gained rosace-render dep. POSSIBLE NIT to verify: tab bar may show only "All" prominent — check TabBar lays all 3 evenly. NEXT: wire into rsc new scaffold (app root wraps with it in debug), fold F12 inspector in as "Elements" tab, delete devtools_demo throwaway.

### EXTERNAL WEB DEVTOOLS + IDE integration — PLAN 2026-07-24 (user request)
Goal: an external DevTools web tool (for browser + VS Code/RustRover webview) that shows network/logs/traces/elements for a running app (desktop OR mobile). Reuse, don't impersonate Chrome CDP (rejected D123). Reuse Perfetto for the trace timeline (to_chrome_trace_json EXISTS). Build a lightweight custom web tool for the rest.
ARCHITECTURE (rides the trace bus — same interceptor as the in-app overlay + logging):
  1. Serialize `RosaceTrace` to JSON via serde — THE prerequisite (serde currently deferred on the event enum). Also makes SDUI/descriptor-wire real (see hot-reload memory).
  2. `WebSocketSink` = a `TraceSubscriber` that streams JSON events. WS server already hand-rolled in rsc-cli/src/commands/hot_ws.rs (SHA1/base64/RFC6455) — reuse it. New `rsc devtools` command serves it + the web page.
  3. Web page = ONE self-contained HTML/JS file, connects to the WS, renders Network·Logs·Traces·Elements panels (same panels as the in-app overlay). Perfetto link for the flamegraph.
  4. IDE = thin VS Code + RustRover extensions = a webview pointing at the page + WS URL. Or just open in a browser.
MOBILE: stream the device's events over the EXISTING hot-reload socket transport (adb forward / devicectl) to the desktop web tool — the real mobile-debugging story (Flipper/React-Native-DevTools/Chrome-remote-debug model). The in-app overlay is the on-device quick glance.
IN-APP DEVTOOLS ON MOBILE (current state): works because it's a widget overlay (renders+hit-tests via the FFI engine pipeline on device; FAB is a tap target; trace bus + flight recorder run on mobile). NEEDS: responsive panel (full-screen/bottom-sheet on narrow screens vs the 440px desktop panel), FAB must respect safe-area insets (notch/home-bar), small-screen ergonomics. NOT yet done/verified on device.
SEQUENCING: serde-on-events → WebSocketSink → web page → `rsc devtools` → IDE webviews. Mobile responsive-overlay is a parallel small task.
