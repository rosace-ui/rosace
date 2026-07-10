# Phase 21 — Widget Protocol: Children, Contexts, Semantics

> Status: COMPLETE
> Started: 2026-07-03
> Completed: 2026-07-08
>
> Progress notes:
> - Step 1 ✅ Children accessor + defaults (d070844)
> - Step 2 ✅ scoped: unbounded-axis doctrine live (min=viewport ScrollView,
>   defined flex + warning, min-preservation, paint reuses layout's sizes —
>   fd2de5f). The FULL framework child-geometry (layout_child memoized on
>   the tree + position_child) is DEFERRED: cross-frame memo is unsafe
>   until widget reconciliation exists (Phase 20 Step 1) — content can
>   change under identical constraints. Do them together.
> - Step 3 ✅ scoped: on_press/on_scroll declaration sugar (a3a997e).
>   Custom-shape hit() override deferred to the two-pass walker.
> - Step 4 ✅ semantics hook + derived a11y tree + 9 widget declarations
> - Step 5 ✅ CustomPaint (71b4914)
> - Step 6 ✅ LANDED 2026-07-08: `.steering/WIDGET_AUTHORING_GUIDE.md` — the
>   practical companion to WIDGET_PROTOCOL.md, written against the REAL
>   shipped API (not the aspirational planning spec — no framework-owned
>   layout_child/position_child, no hit()/semantics() trait methods; both
>   descoped per Steps 2/3 above). Three worked examples, one per taxonomy
>   row (Dot=leaf, Highlight=single-child wrapper, EvenColumn=multi-child
>   container), proven via a real compiling+running bin
>   (rosace-examples/src/bin/widget_authoring_demo.rs) rather than
>   untested prose — visually verified all three render correctly. README
>   gained a "Writing Custom Widgets" section linking the guide.
> Decisions: D098 (two-concept model, taxonomy by defaults),
> D099 (framework-owned child geometry, declarative semantics),
> D100 (CustomPaint as recorded Leaf)
> Spec: `.steering/WIDGET_PROTOCOL.md` (planning) +
> `.steering/WIDGET_AUTHORING_GUIDE.md` (as-shipped, Step 6)
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
Exit: `grep measure_cache rosace-widgets` → nothing; layout runs once
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
Trace: emit node count under ROSACE_TRACE=perf.
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
