# Responsive + MediaQuery — design (2026-07-24, user)

Two complementary mechanisms. Neither is `LayoutBuilder` — that one is rejected
because in a `ScrollView` the offered constraint is **infinite** (unbounded
axis), so a builder that reads `max_height` gets `∞` and breaks. We measure the
**real placed rect** instead, and expose global environment as a reactive atom.

## Part A — Global MediaQuery (a `GlobalAtom`)
App-level environment, **reactive**. NOT just width — supplies:
- **brightness** (dark / light) and the active **theme**
- **orientation** (portrait / landscape)
- **window / screen size** (logical)
- **text scale**, **reduce-motion**, **platform**
- **safe-area** insets (this ALREADY exists as `use_safe_area()` — the exact
  pattern to generalize)

Mechanism: `static MEDIA_QUERY: GlobalAtom<MediaQuery>`; `use_media_query()` to
read (reading subscribes the component, so it rebuilds when it changes);
`set_media_query(..)` called by the platform/engine on startup and on
resize / theme-toggle / orientation-change. This is Flutter's `MediaQuery` but
as a clean reactive atom — and `safe_area.rs` already proves the pattern works.

## Part B — Per-widget post-layout callback (`on_layout(rect)`)
The user's idea, and it's the right primitive: a widget optionally gets a
callback **after it's laid out and placed on the canvas**, with its **actual
world-space rect** (finite, real) — and **only when the rect changes** (build →
place → callback; then silent until the rect differs). This is SwiftUI's
`onGeometryChange` / a per-widget `ResizeObserver`.

Why it beats `LayoutBuilder`: it reads the **resolved** size, not the offered
constraint — so it's correct inside a `ScrollView` (where the constraint is
infinite but the placed width is real).

Feasibility (already mostly there): the render tree **already stores
`cached_rect` per node** (added for DevTools O2 — every painted node's
world-space rect). So: add an optional `on_layout: Arc<dyn Fn(Rect)>` per node
+ a `last_reported_rect`; after the place/paint walk, diff `cached_rect` vs
`last_reported_rect` and fire the callback only on change. No per-frame spam;
works in scroll views.

Loop safety: a callback that `set`s state may change layout → re-fire. Guarded
by (a) fire-only-on-change and (b) layout converging to a stable rect (same as
`ResizeObserver`'s loop breaker). Document it; don't resize yourself from your
own `on_layout` without a fixed point.

## How they combine (the responsive story)
- **Global MediaQuery** → app-shell decisions: "window is 400px / portrait /
  dark" → mobile drawer vs desktop rail; dark theme.
- **`on_layout`** → local decisions that work anywhere, including scroll views:
  "THIS panel became 300px → reflow its grid to 1 column."
- Ergonomic sugar on top: `Responsive`/breakpoint helpers reading MediaQuery;
  a `MeasureSize`/`.on_layout()` modifier for the local case.

## Status
Planned. Foundational for dark-mode/orientation and the web/multi-platform
story, so a near-term maturity item (dark-mode + orientation likely wanted for
0.0.1-dev). Both build on existing mechanisms (`GlobalAtom`, `cached_rect`).
