# Phase 26 — Pervasive Default Animation (D108)

> Status: IN PROGRESS (scoped 2026-07-09; Step 1 starting)
> Started: 2026-07-09
> Completed: —
> Decision: **D108** — extend TEZZERA's existing theme-global, zero-per-app
> animation model (`PaintCtx::animate_to`, governed by `ThemeData.animation`)
> from the four widgets that use it today to press feedback, real momentum
> scroll, default-on nav transitions, and image load-in fades. The
> "abundant animation library + authoring framework" half of D108's original
> vision is deliberately deferred past this phase (see Out of Scope below).

## Why This Phase

D108 (`.steering/DECISIONS.md`) recorded a vision at the user's request on
2026-07-08, explicitly as a NOTE ONLY — "don't just start coding from this,"
because it spans many crates and needed real scoping first. This phase is
that scoping, done by first auditing what already exists (not assumed from
the note) rather than designing from scratch.

**The audit's key finding, which reshapes this phase's real size**: most of
the "new infrastructure" the vision implies already exists in the codebase,
just unwired from the paths real apps actually use:

- `tezzera-scroll` already has a real momentum/friction physics engine
  (`physics.rs::MomentumState`, `ScrollPhysics::Momentum`) — but the actual
  `ScrollView` widget apps use (`tezzera-widgets/src/tree/scroll_view.rs`)
  talks to a *different*, instant-offset `ScrollController` instead. The
  momentum engine is orphaned — referenced only by its own crate's doctest.
- `tezzera-nav-anim` already has `NavigatorAnimated`/`ScreenTransition`
  (slide/fade transitions, spring-backed) — but the real navigation path
  (`tezzera-nav::Navigator`/`ScreenNav`) never references the crate at all.
  Also orphaned.
- The theme-global animation primitive apps actually use today,
  `PaintCtx::animate_to` (`tezzera-widgets/src/tree/mod.rs:615`), already
  respects `ThemeData.animation.enabled`/`duration_ms` correctly and is the
  right foundation to extend, not replace. `Switch`/`Checkbox`/`Radio`/
  `SegmentedControl` already animate through it.

So this phase is mostly **wiring existing engines into the real widget
paths**, plus two genuinely new pieces of animation (press feedback, image
fade-in) built on the existing `animate_to` primitive — a materially smaller
lift than "build momentum physics and a transition system from scratch"
would have been.

## Out of Scope (deliberately, not silently dropped)

- **Per-category `AnimationConfig`.** Today there's one global
  `duration_ms` for all animated widgets. D108's note flags per-category
  duration/curve (press vs. scroll vs. nav vs. fade) as a likely future
  need, but nothing in Steps 1-4 actually requires it — all four reuse the
  theme's single global duration, matching the four existing `animate_to`
  callers. Revisit only if a real need surfaces while building these
  steps, not preemptively.
- **List-item enter/exit animation.** Needs real diffing of list identity
  across rebuilds — a meaningfully different problem from scalar-easing
  the four steps below share. Its own future step/phase.
- **The animation-authoring framework / ready-to-use library.** A
  product-surface-sized effort in its own right, best scoped AFTER this
  phase's default-animation foundation is solid and has revealed what
  primitives actually get reused enough to warrant a library.

## Steps

### Step 1 — Press/tap feedback
`Button::paint` does instant, un-eased hover color-lift and has no
distinct pressed state at all; `Pressable`'s own doc comment says
press/hover visual feedback "arrives with the interaction-states work" —
this step is that work, scoped to visual feedback only (not a new gesture
system). Adds a per-node "press amount" driven through `animate_to`
(0→1 on press-down, back to 0 on release/cancel) to ease a color/opacity
lift on `Button`, `ListTile`, `Chip`, and other `Pressable`-driven widgets.

Exit: pressing a button in a running app shows a smooth eased color
transition (not an instant snap) on press and release, verified via real
interaction, and snaps instantly when `set_animations(false)`.

### Step 2 — Wire real momentum scroll
`ScrollView` currently writes scroll offset instantly via
`ScrollController::scroll_by`. Wires `tezzera-scroll`'s existing
`MomentumState`/`ScrollPhysics::Momentum` in: on drag release with
residual velocity, decay the offset via friction each frame until it
settles or a new drag interrupts it. Respects `theme.animation.enabled`
(disabled → instant stop at release, no coast).

Exit: a real drag-and-release in a running scrollable list visibly
continues scrolling with decreasing speed after the pointer lifts,
verified by interacting with a real running app, and stops instantly when
animations are disabled.

### Step 3 — Wire real nav transitions
`tezzera-nav`'s `Navigator`/`ScreenNav` has zero references to
`tezzera-nav-anim` today — pushes/pops are instant. Wires
`ScreenTransition` (slide) into the real push/pop path, default-on,
respecting `theme.animation.enabled`.

Exit: pushing/popping a screen in a real running app shows a slide
transition by default with zero app-level wiring (`tzr new`'s generated
Home→Counter navigation is the concrete test case), off when animations
are globally disabled.

### Step 4 — Image load-in fade
`Image`/`ImageWidgetImpl` swaps placeholder→loaded instantly. Adds an
`animate_to`-driven opacity ramp (0→1) starting the frame an image's
pixel data becomes available.

Exit: loading a real image in a running app shows a visible fade-in over
the theme's animation duration, verified via a screenshot sequence across
frames, not just code reading.

## Sequencing

Each step gets its own real-app exit-bar verification before the next
starts — same discipline `PHASE_25.md` followed, and for the same reason:
Phases 24 and 25 both found real, previously-unknown bugs specifically
during real-app verification, not code review. Steps 1 and 4 are
independent and could be reordered; Steps 2 and 3 (wiring an orphaned
engine into a real path) carry the higher integration risk.

## Migration Rule

No new widget opts in to anything — every change here is inside an
existing widget's own paint/interaction path, governed entirely by the
existing `theme.animation.enabled` switch an app can already flip. No
public API changes expected beyond whatever Steps 2-3's wiring needs
internally.
