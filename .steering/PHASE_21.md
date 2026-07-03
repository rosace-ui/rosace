# Phase 21 — Widget Protocol: Children, Contexts, Semantics

> Status: PLANNED
> Started: —
> Completed: —
> Decisions: D098 (two-concept model, taxonomy by defaults),
> D099 (framework-owned child geometry, declarative semantics),
> D100 (CustomPaint as recorded Leaf)
> Spec: `.steering/WIDGET_PROTOCOL.md`
> Ordering: BEFORE Phase 22 (API consistency) — built-ins migrate once,
> onto the final protocol, not twice.

## Why This Phase

Custom-widget authors today face: two composition systems (Component vs
Widget) with no stated relationship, hand-written delegation for every
wrapper, private Mutex measure caches per container, positions recomputed
in paint (measure/paint drift — the Text overflow bug class), no semantics
hook at all, and no hit-test story beyond register_hit. The protocol
(WIDGET_PROTOCOL.md) collapses this to one trait with defaults keyed off
`children()`, framework-owned child geometry, and declarative semantics.

## Migration Rule

Demos/tests green after every step; one commit per step. The Widget trait
changes are breaking — built-ins migrate in the same step as the change
that breaks them. No compatibility shims.

## Steps

### Step 1 — `Children` accessor + default layout/paint/flex
Add `Children` enum + `children()` to the Widget trait; implement defaults
(leaf: constraint-min + nothing; One: delegate; Many: layout must be
overridden, paint from stored positions). Migrate trivial wrappers
(RepaintBoundary, RectReader, WithFocus, WithOverlay, Expanded) to
delete their hand-written delegation.
Exit: wrappers implement only the methods they change.

### Step 2 — LayoutCx child geometry
`layout_child(i, c)` (memoized on the render-tree node) + `position_child`;
`PaintCx::paint_child(i)`. Migrate Column/Row/Stack/Scaffold: their
measure logic moves into `layout()` once; `paint()` stops re-measuring;
delete every `Mutex<Option<(Constraints, Vec<Size>)>>` cache.
Exit: `grep measure_cache tezzera-widgets` → nothing; layout runs once
per dirty node per frame (verify via LayoutStart traces).

### Step 3 — Default hit protocol
`hit()` default from declared regions + children back-to-front; `on_press`/
`on_scroll` as PaintCx declarations (mechanics already on the render tree
from Phase 20). `hit()` override documented for non-rect targets.
Exit: Slider knob / circular targets expressible without full-rect hacks.

### Step 4 — Semantics hook
`SemanticsCx` writing role/label/value/actions to the render-tree node;
derived semantics tree collector; defaults for Text/Image; declarations on
Button, Checkbox, Switch, Slider, TextInput, Dialog (Role::Dialog), Menu.
Trace: emit node count under TEZZERA_TRACE=perf.
Exit: app_demo produces a non-empty semantics tree with correct roles.

### Step 5 — CustomPaint (D100)
`CustomPaint::new(closure).repaint_when(atom)` + hit_test via standard
protocol. Example bin demonstrating a custom chart.
Exit: a custom painter works with caching (repaints ONLY when its atom
changes — verify via trace).

### Step 6 — Authoring guide
`docs/` or steering: "Writing a Widget" walkthrough — the decision table,
one worked example per taxonomy row, escape-hatch ladder. README section.
Exit: the three WIDGET_PROTOCOL.md examples compile as doc-tests or an
example bin.

## DO NOT

- Do not expose Element/RenderTree in the authoring API.
- Do not keep per-widget measure caches "temporarily".
- Do not implement platform a11y bridges here (only the authoring hook +
  derived tree; bridges are their own phase).
