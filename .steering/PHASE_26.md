# Phase 26 — Pervasive Default Animation (D108)

> Status: IN PROGRESS (scoped 2026-07-09; Step 1 landed 2026-07-09)
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

**Landed.** New dispatcher-owned `TreeNode::pressed: bool`
(`render_tree.rs`, mirrors `hovered` exactly) + `RenderTree::set_pressed`
+ `PaintCtx::pressed()`. Wired into `tezzera/src/engine.rs`'s real
MouseDown/MouseUp handlers — MouseDown resolves the target via the same
`hover_test` the existing hover-tracking MouseMove handler already uses
(deliberately reused, not a new hit-resolution path — lower risk than
threading node ids through `hit_test`'s recursive scroll-transform
wrapping). `Button` and `ListTile` (the two widgets that already called
`ctx.hovered()` for their own styling) now ease a single "emphasis" scalar
through `animate_to` across three levels — idle (0.0), hover (0.5, matches
the old flat lift exactly, so no visual regression when animations are
off), press (1.0, double the hover lift, so a tap reads as visually
distinct). `Chip` was investigated and correctly left alone — unlike
Button/ListTile it has no `on_press`/hover wiring of its own; it's only
interactive via the external `.on_press()` (`Pressable`), which paints the
child on a *different* render-tree node than the one press/hover state
lands on. Giving `Pressable`-wrapped widgets press feedback is a real,
separate architectural question (does `Pressable` need to expose its own
node's interaction state down to the child?) — flagged, not solved here.

**Verified for real**: two new integration tests in `tezzera/src/
engine.rs` drive an actual headless `FrameEngine` (the same struct the
real desktop/web/iOS/Android paths all use) with synthetic
`InputEvent::MouseDown`/`MouseUp`, exactly like `tezzera-web-seo`/Phase 25
used a headless `FrameEngine` for build-time SEO extraction — not a
render-tree-only unit test. One confirms `MouseDown` sets `pressed` on a
real node and `MouseUp` clears it; the other confirms the eased scalar
actually advances toward the 1.0 press target over successive frames (with
a synthetic `frame_dt` for determinism, not real wall-clock time — avoids
a flaky test whose convergence speed depends on machine speed) and
converges to 1.0. Also scaffolded a fresh `tzr new`-generated desktop app
and ran it for real (screenshot confirms correct rendering, no visual
regression). **Honest gap**: this environment has no native-desktop GUI
automation tool (no `cliclick`/Quartz installed, no computer-use tool for
non-browser windows) to literally move the OS mouse cursor and hold a
button down for a live screenshot sequence — unlike Phase 25's Chrome
automation, which was available for the web target. What WAS verified
instead is the exact same dispatch code a real click would run, driven
with synthetic events through the real `FrameEngine`, plus visual
confirmation the app still renders correctly. If a literal physical-click
screenshot sequence is wanted, it needs either a human at the mouse or an
explicitly-authorized install of a click-automation tool.
Full `cargo build --workspace` / `cargo test --workspace --no-fail-fast`
clean (zero failures) after this step.

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
