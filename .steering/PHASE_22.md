# Phase 22 — API Consistency: One Law, Fewer Widgets

> Status: COMPLETE
> Started: 2026-07-03
> Completed: 2026-07-08
>
> Progress:
> - Step 1 ✅ vocabulary sweep (44d565a) — .background() everywhere
> - Step 2 ✅ constructor law (ed4a8fe) — Divider::new, transparent
>   Box<dyn Widget>; Tooltip(label, child) ruled legal (different types)
> - Step 3 ✅ one box (f54accd) — ColoredBox/SizedBox/Padding/Center/
>   ListView deleted; Alignment + Container.align; Spacer::gap
> - Step 4 ✅ layout slimmed (7668559) — element widgets deleted, math
>   kept; Grid/Wrap/AspectRatio retained (not duplicates)
> - Step 5 ✅ scroll API (01f6452) — new()=live, horizontal(), fixed(),
>   Column/Row::scrollable
> - Step 6 ✅ AppBarNavExt::back_button in facade prelude; ScreenNav was
>   already the only prelude nav type
> - Step 7 ✅ NAMING.md pointer done 2026-07-03; CRATE_CONTRACTS/README
>   deferred to Phase 21 Step 6, which THEN also didn't cover it (the
>   authoring guide isn't a crate-contracts refresh) — actually landed
>   2026-07-08 as a full rewrite (commit 369fe1f): the old doc covered 16
>   of 34 crates and named crates/widgets that don't exist. Found real,
>   previously-unknown issues in the process (rosace-anim is dead code;
>   rosace-gesture and rosace-test-utils both depend on rosace-platform,
>   an undocumented Layer-5 cross-service dependency; rosace-style is
>   unintegrated) — recorded in CRATE_CONTRACTS.md's new "Known Issues"
>   section, not yet fixed.
> Decisions: D093 (constructor law), D094 (vocabulary), D095 (consolidation),
> D096 (builder styling), D097 (scroll + nav canonicalization)
> Constitution: `.steering/API_DESIGN.md`

## Why This Phase

The widget API audit (2026-07-02) found:
- Background color has three names: `.background()` (AppBar/Card/Scaffold/
  NavRail/Tab), `.color()` (Container — sets background!), `.bg()` (ListTile)
- Constructor chaos: `Card::new(child)` vs `Container::new().child()` vs
  `Padding::new(insets, child)` vs `SizedBox::new()` vs `Tooltip::new(label, child)`
- Six widgets doing one job: ColoredBox, SizedBox, Padding, Center,
  Container, Expanded::empty
- A full parallel element-based widget set still exported from rosace-layout
- Two navigation systems, and ScrollView's default constructor silently
  doesn't scroll

Every one of these produced real friction while writing the demos this week.
Breaking changes are cheap now (no external users) and get more expensive
every phase.

## Migration Rule

- Demos and tests stay green after every step; each step is one commit.
- Removal means REMOVAL — no deprecated aliases left behind (no users yet).
- Every step updates all call sites (widgets, demos, tests, doc comments,
  rsc new templates in rosace-cli).

## Steps

### Step 1 — Vocabulary sweep (D094)
Rename builders to the canonical table: Container `.color()` → `.background()`,
ListTile `.bg()` → `.background()`, audit every widget against §3.
No behavior changes. Exit: grep for `fn bg(`, `fn color(` on surface-color
widgets returns nothing.

### Step 2 — Constructor law sweep (D093)
- `Padding::new(insets, child)` → interim `Padding::new(child).insets(..)`
  (dies in Step 3 anyway)
- `Tooltip::new(label, child)` → `Tooltip::new(child).label(..)`? NO —
  label is required content: keep `(label, child)`? Illegal under two-required
  rule → `Tooltip::new(child).text(label)` with debug assert if text unset.
- Divider gets `new()` (= horizontal), keeps `::horizontal()/::vertical()`.
- `Box<dyn Widget>` implements `Widget` so builders accept boxed children
  (kills the `boxed()` adapter in app_demo).
Exit: API_DESIGN §1 table holds for every exported widget.

### Step 3 — Consolidation (D095)
- Container absorbs: `.align(Alignment)` (from Center), everything else it
  already has. Then delete ColoredBox, SizedBox, Padding, Center.
- `Spacer::flex()` replaces `Expanded::empty()`.
- Audit ListView: if non-virtualizing Column wrapper → delete.
- Migrate all demos + `rsc new` templates.
Exit: deleted files gone from tree/mod.rs, prelude, facade; demos green.

### Step 4 — rosace-layout slimming (D095)
Remove element-based widget structs (Column, Row, Stack, SizedBox, Spacer,
Flex, Expanded, Grid, Wrap, AspectRatio); keep Constraints, alignments,
layout_column/row/flex math, LayoutResult. Grid/Wrap/AspectRatio math stays
as free functions for future tree widgets.
Exit: rosace-layout exports no `impl From<_> for Element` widgets.

### Step 5 — Scroll API (D097)
`ScrollView::fixed(child, offset)` for snapshot mode; `ScrollView::new(child,
atom)` = live; `Column::scrollable(atom)` / `Row::scrollable(atom)` sugar.
Exit: no demo uses a ScrollView that cannot scroll unintentionally.

### Step 6 — Nav canonicalization (D097)
`AppBar::back_button(&nav)`; ScreenNav re-exported as THE nav API;
Navigator/Route/history/guards + nav-anim Navigator out of prelude
(internal or feature-gated). app_demo drops its manual back-button block.
Exit: prelude exposes exactly one navigation type.

### Step 7 — Docs
Update NAMING.md with a pointer to API_DESIGN.md §3; refresh CRATE_CONTRACTS
for the slimmed rosace-layout; refresh README examples.

## DO NOT

- Do not add a `.style(struct)` API in this phase (D096 defers it).
- Do not keep compatibility aliases "just in case".
- Do not introduce new widgets during the sweep — presets are named
  constructors (API_DESIGN §5 rule of thumb).
