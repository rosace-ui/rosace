# Observability & DevTools Track (O1–O7) — D123

> Status: O1 foundation landing 2026-07-16. O2–O7 scoped, not started.
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
