# Showcase coverage

Measured 2026-08-13 against `examples/showcase`, not estimated. Method:
enumerate every type in `rosace-widgets/src/tree` that implements `Widget`,
enumerate each one's builder methods (`pub fn x(mut self, ..) -> Self`), then
parse the showcase for `Type::new(..)` and walk the method chain that follows
it at paren-depth 0.

Scoping to the chain matters. A naive "is `.color(` called anywhere?" gives
86% and is meaningless — every widget has a `.color`, so one call anywhere
marks it covered everywhere. Per-widget, the real figure is 71%.

## Never constructed at all — 14 of 75

**User-facing, genuinely missing:**
`Autocomplete` · `Dismissible` · `InteractiveViewer` · `ListView` ·
`PullToRefresh` · `TabBar` · `TabView` · `Stack` · `Expanded` · `Positioned`

**Wrappers/infrastructure — arguably fine to omit, but `Semantics` should be
demonstrated since apps are expected to reach for it:**
`RepaintBoundary` · `WithFocus` · `WithOverlay` · `Semantics`

## Builder coverage: 277/389 = 71%

Worst offenders (exercised / available):

| Widget | | Never shown |
|---|---|---|
| `CircularProgress` | 0/4 | color, diameter, thickness, track |
| `FloatingActionButton` | 0/9 | everything — one bare `::new()`, no icon, no `on_press` |
| `ScrollView` | 0/9 | axis, physics, scrollbar_* , controller, offset, gpu_layer |
| `TextInput` | 4/19 | adornments, controller, cursor_style, filters, field, focus |
| `ListTile` | 3/11 | leading, trailing, selected, sizes, no_divider |
| `Button` | 4/11 | background, color, disabled_if, radius, size overrides |
| `ProgressBar` | 2/6 | height, label, radius, width |
| `Text` | 2/5 | max_lines, size, weight |

## Why this matters more than it looks

**The widgets absent from the showcase are the same ones the audit found
bugs in.** `Dismissible`, `PullToRefresh` and `Autocomplete` were built and
shipped without ever being put on screen; `ListView` — the virtualized list
— is never demonstrated, and it is where the audit found no `Role::List`, no
item count, an unclamped scrollbar thumb and a hardcoded scrollbar colour.

That is not a coincidence. The 46-item device pass (WIDGET_FINDINGS Part B)
could only report on what it could see. A widget with no showcase screen has
no path to being found broken by hand — it is only reachable by an audit
like this one.

The same holds one level down for builders: `ScrollView` appears on nearly
every screen, so it looks well covered, but always as plumbing at defaults.
Its scrollbar styling, physics and axis options have never been rendered.

## Suggested order

1. Screens for the six missing user-facing widgets, `ListView` first (it is
   the one with known unfixed findings and the widest real-world use).
2. A "customisation" section on the existing screens for the 0/N widgets —
   FAB and CircularProgress are one small screen each.
3. `Semantics`, so the a11y escape hatch has a worked example.
