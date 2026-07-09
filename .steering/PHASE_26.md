# Phase 26 — Pervasive Default Animation (D108)

> Status: IN PROGRESS (scoped 2026-07-09; Step 1 landed, Step 2 paused with
> a known open issue — see Known Issue #9 in CRATE_CONTRACTS.md — moving to
> Step 3)
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

## Platform never hardcodes behavior (added 2026-07-09, governs Steps 2/3/5)

Stated explicitly by the user and already the project's own D105 principle
(`tezzera-core/src/platform.rs:1-7`): widgets/engines never hard-branch on
platform (`if platform == Ios { ... }`). Platform only ever selects a
DEFAULT preset through the existing theme-resolution mechanism
(`Themes::platform(Platform::Ios, cupertino())`), and every default must
be explicitly overridable by app code regardless of the detected platform
— an Android app that wants iOS-style bounce scroll just sets it. Steps
2 and 3 resolve their platform defaults through `ThemeData`'s existing
type-keyed `ext` map (`tezzera-theme/src/theme.rs:83-86`, D105 Phase 23
Step 4 — built exactly for this: "lets a custom widget stash and read its
own theme-style struct without editing this type"), the same shape
`AppBarStyle` already proved out — not a new mechanism.

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

### Step 2 — Wire real momentum + bounce scroll (expanded 2026-07-09)
`ScrollView` currently writes scroll offset instantly via
`ScrollController::scroll_by`. Wires `tezzera-scroll`'s existing
`MomentumState`/`ScrollPhysics::Momentum` in: on drag release with
residual velocity, decay the offset via friction each frame until it
settles or a new drag interrupts it. Respects `theme.animation.enabled`
(disabled → instant stop at release, no coast).

Adds `ScrollPhysics::Bounce { friction, spring_stiffness }` (overscroll
resists then springs back — the iOS feel; Android's overscroll glow is a
separate visual, not a different physics model, and is out of scope). The
default physics resolves per-platform via a new `ScrollStyle` ext struct
on `ThemeData` (`cupertino()` → `Bounce`, `material()`/desktop/web →
`Momentum`) but an explicit `.physics(...)` on `ScrollView` always wins —
see the platform rule above.

Exit: a real drag-and-release in a running scrollable list visibly
continues scrolling with decreasing speed (or bounces back, if
`Bounce`-configured) after the pointer lifts, verified by interacting with
a real running app; an explicit override to the non-default physics works
regardless of detected platform; stops instantly when animations are
disabled.

**Landed, with a real scope correction mid-implementation.** Investigating
the real input model (not assumed) found `ScrollView` had NO drag-to-pan
gesture at all — only wheel/trackpad `Scroll` events existed; nothing
turned a mouse/touch drag into a scroll offset change. User confirmed:
build drag-to-pan first (with velocity tracked from the REAL drag speed,
not an assumed constant), then momentum/bounce on top — this step ended up
delivering both, not just wiring existing physics onto an existing
gesture. Also found a real layer-rule conflict: `tezzera-theme` (Layer 4)
can't depend on `tezzera-scroll` (Layer 5), so `cupertino()`/`material()`
can't construct a `ScrollStyle` ext value directly. Resolved by keeping the
platform-default computation as ONE pure function inside `tezzera-scroll`
itself (`ScrollStyle::default_for_platform`, reads `tezzera-core::Platform`
which IS a lower layer) — an app's theme `ext` override and an explicit
per-`ScrollView` `.physics(...)` both still take priority over it, so the
"platform is a default-picker only, never hardcoded" rule holds exactly as
designed, just resolved in a different crate than first planned.

New `ScrollController` methods (`tezzera-scroll/src/controller.rs`):
`drag_delta`/`end_drag` (streamed absolute drag position → per-move delta),
`track_velocity`/`velocity()` (real px/s from the actual offset delta each
frame — not assumed), `apply_momentum` (rubber-band-aware offset step,
resists overscroll under `Bounce`, hard-clamps otherwise), `settle_bounce`
(exponential ease back to bounds once velocity settles, same shape as
`animate_to`), `coast` (one frame of decay-or-settle, orchestrating
`MomentumState`), `stop_coasting` (hard stop for the animations-disabled
case). `ScrollView::paint_base` wires these together via `ctx.on_press_at`
(the same positional-hit mechanism sliders use) and reuses Step 1's
`ctx.pressed()` — no new engine.rs plumbing needed, since declaring the
drag region makes `hover_test` (which Step 1's MouseDown handler already
calls) pick it up for free. Scoped to the base (CPU) scroll path only —
the GPU-layer path's separate non-reactive offset channel is flagged as
real follow-up, not silently extended.

**Two real bugs found and fixed via the headless `FrameEngine` integration
tests** (same technique as Step 1 — synthetic `MouseDown`/`MouseMove`/
`MouseUp` through the real engine dispatch, not controller-level unit
tests alone):
1. **Missing sign inversion.** The first implementation applied the raw
   drag delta directly, so dragging up moved the offset the wrong way
   (content chasing the cursor instead of following it, backwards from
   every real scrollable surface). Fixed by negating, matching the
   existing wheel-scroll callback's own sign convention exactly.
2. **A spurious "fresh drag" reset caused by the well-known 1-frame lag**
   between an input event and `ctx.pressed()` observing it (the same lag
   `ctx.hovered()` already has). The first implementation reset
   `last_drag_point` whenever it saw `was_pressed` transition false→true —
   but because of the lag, that transition is observed on the SAME frame
   as the drag's first `MouseMove`, one frame after `MouseDown`'s own
   immediate callback invocation had already correctly established the
   drag's starting point. The reset wiped that baseline out from under the
   very next `drag_delta` call, making the first move of every drag
   silently do nothing. Root-caused via targeted print debugging (atom ids
   and pointer addresses to rule out a sharing bug first) after an
   isolated standalone `Atom<Option<T>>` round-trip test proved the atom
   primitive itself was correct — the bug was in this step's own new
   control-flow logic, not the state layer. Fixed by removing the
   redundant reset entirely; `end_drag` on release (which does NOT have
   this lag problem, since it fires from the same frame's dispatch that
   also updates `pressed`) is the only reset actually needed.

**A third, unrelated pre-existing bug found along the way**: running the
new tests alongside the existing suite intermittently failed with
`disabling_animations_stops_coasting_immediately_on_release` racing
`drag_pans_content_and_momentum_coasts_after_release` — `tezzera_theme`'s
theme is a process-wide `GlobalAtom`, and `cargo test` runs test functions
on parallel threads by default, so one test's `set_animations(false)`
could flip the flag mid-coast for the other. Fixed with a `static ...
Mutex` guard around both tests (`tezzera/src/engine.rs`) — confirmed
stable across repeated runs afterward. **While chasing this down, found
the SAME class of bug already existing, unrelated to this phase's own
code**: `tezzera-state/src/frame_scheduler.rs`'s
`request_frame_sets_flag`/`take_clears_flag`/`multiple_requests_collapse_to_one`
tests race each other the same way and are flaky under
`cargo test --workspace`'s full parallel load (always pass in isolation).
Logged in `CRATE_CONTRACTS.md`'s Known Issues (#8) rather than fixed here
— pre-existing, outside this step's scope, not introduced by this work.

**Verified for real**: the two new `tezzera/src/engine.rs` integration
tests (real `FrameEngine`, synthetic but real input events) cover drag
panning content in real time, momentum continuing after release
proportional to actual drag speed, and animations-disabled producing an
immediate hard stop with zero coast. Full `cargo build --workspace` /
`cargo test --workspace --no-fail-fast` clean throughout (the one
exception, `tezzera-state`'s pre-existing parallel-test flake, is
unrelated and logged separately — `CRATE_CONTRACTS.md` Known Issue #8).

**Status: PAUSED with a known unresolved issue, not claimed as fully
working — see `CRATE_CONTRACTS.md` Known Issue #9.** Real on-device
testing (not just the headless engine tests above) went through many
rounds against a real running macOS app: drag-to-pan direction, a
dt-unit-mismatch bug, frame-rate-dependent friction decay, unbounded
overscroll, a stale scrollbar read, a per-frame-flag wheel-gating bug, an
overscroll-recovery timing bug, and a velocity clamp — each confirmed and
fixed with a regression test (44 tests in `tezzera-scroll` total). The
LAST round used a real screen recording (frame-extracted with a one-off
Swift/AVFoundation script) to root-cause a genuine oscillation to a real
platform quirk: macOS delivers trackpad momentum as the OS's own event
stream after the user's fingers lift, and winit 0.30.13 collapses that
into the same event phase as real finger movement (confirmed by reading
winit's own macOS backend source), so TEZZERA's own momentum system was
fighting the OS's. The fix (wheel input no longer injects synthetic
velocity, applies directly instead) changed the failure mode but did not
fully resolve it in the reporter's live testing. Per explicit user
direction, this is being logged and left open rather than pursued
further right now — drag-to-pan and the underlying momentum primitives
are solid; `ScrollPhysics::Bounce`'s real-trackpad feel specifically is
not. `ScrollPhysics::Momentum` (the default for every platform except
iOS/macOS) was not implicated in any of the live testing.

### Step 3 — Wire real nav transitions (expanded 2026-07-09)
`tezzera-nav`'s `Navigator`/`ScreenNav` has zero references to
`tezzera-nav-anim` today — pushes/pops are instant. Wires
`NavigatorAnimated`/`ScreenTransition` into the real push/pop path,
default-on, respecting `theme.animation.enabled`.

The default transition STYLE resolves per-platform via a new
`NavTransitionStyle` ext struct on `ThemeData` (same mechanism as Step 2's
`ScrollStyle`), always overridable via an explicit `Navigator::
transition(...)` call regardless of detected platform.

Exit: pushing/popping a screen in a real running app shows the
platform-default transition with zero app-level wiring (`tzr new`'s
generated Home→Counter navigation is the concrete test case), an explicit
override to a different style works regardless of platform, off entirely
when animations are globally disabled.

### Step 5 — Hero / shared-element transitions (new 2026-07-09)
Confirmed genuinely greenfield (grepped "hero"/"shared_element"/"morph" —
zero real hits). Depends on Step 3 landing first (needs its transition
progress signal to interpolate against).

Revives the existing-but-completely-dead `Key`/`Element::with_key`
identity primitive (`tezzera-core/src/{types,element}.rs` — no widget has
a `.key(...)` builder today, and the reconciler never reads it; today's
real identity, confirmed in `walk_element`/`RenderTree`, is purely
positional and resets every navigation) as a widget-facing `.hero_tag(id)`
builder. A new `HeroController` captures a hero-tagged widget's world
rect + `Picture` right before its screen is pruned from the render tree
on navigation, then paints it at the LERP'd rect between outgoing and
incoming positions via the existing overlay-layer mechanism
(`overlay_api.rs`) while Step 3's transition is in progress. Fully opt-in
— no tag used, no behavior change.

Exit: a real running app with two screens sharing a `.hero_tag(...)`'d
image shows it visually morphing between their rects during a push
transition, verified via a real screenshot sequence across frames.

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
during real-app verification, not code review. Step 1 is done. Steps 2
and 3 are independent of each other and can be done in either order; Step
4 is independent of everything and can slot in wherever convenient. Step
5 must come after Step 3 (needs its progress signal).

## Migration Rule

No new widget opts in to anything — every change here is inside an
existing widget's own paint/interaction path, governed entirely by the
existing `theme.animation.enabled` switch an app can already flip. No
public API changes expected beyond whatever Steps 2-3's wiring needs
internally.
