# Plan — one `refresh()`, and cache boundaries that make it mean something

Status: PROPOSED, not started. Written 2026-08-14, revised same day after
verifying where caches actually live.

## The goal, in one line

`widget.refresh()` repaints that widget and nothing else — and the developer
never thinks about paint versus layout.

## Why the developer must not choose

An earlier draft of this plan exposed `refresh_paint()` and
`refresh_layout()`. That was wrong, and it was wrong against this project's
own purpose.

Whether a change affects SIZE depends on font metrics, the OS text scale,
the incoming constraints and the widget's internals. A caller cannot know.
And guessing wrong fails silently: pick the paint-only refresh when the size
changed and you get stale layout, which presents as a rendering bug rather
than an API misuse — expensive to diagnose, easy to ship.

The framework can simply *measure*. Re-running layout and comparing the
result to the cached size is both cheaper and more correct than any
annotation a developer could write. So:

**One `refresh()`. The comparison decides the consequence.**

This is `RenderObject`'s model — `decoration = x` marks needs-paint,
`constraints = y` marks needs-layout, and the caller says neither. We take
the principle and skip the machinery.

## What is actually true today (verified)

**1. Only ONE component exists.** `walk_element` never recurses into
`Element::Native`'s `children` (grep `n.children` in `rosace/src/lib.rs` —
no hits), and a `Widget` always becomes a `NativeElement` with
`children: vec![]`. So the walk finds one `Element::Component`: the root.

**2. `subtree_dirty` flattens everything.** `root_is_dirty || hover_frame`
propagates down and sets `paint_dirty = true` on every node. Both caches
consult `!paint_dirty`, so both are defeated. Changing one `Text` from `1`
to `2` re-layouts and re-records all 100 widgets on screen. **Hover does the
same** — same line.

**3. Caches exist ONLY at element boundaries.** `cached_size`,
`last_constraints` and `cached_picture` are written in exactly one place:
`walk_element` (`rosace/src/lib.rs:444`, `445`, `499`). Every other
occurrence in the repo is test code.

Nested widget nodes — the ones `PaintCtx::child` creates for a `Column`'s
children — get **`cached_rect` only**. Inside a component's widget tree,
`Column::layout` calls `child.layout()` and `Column::paint` calls
`child.paint()` as plain recursive calls. There is no cache boundary in
there to skip work at.

**This is the finding that matters.** Without per-node caches, `refresh()`
would be honest in its API and still repaint the whole subtree, because
there is nothing to skip. The API was never the hard part.

**4. The precedent already exists.** `RepaintBoundary` caches a `Picture`
and replays it, using `ctx.capture(rect, ..)` and `ctx.replay_offset(..)` —
both already built. Its cache lives on the WIDGET instance (`self.cache`),
which survives only because the cached element holds the same instance. Move
that cache onto the NODE and it works for every widget, without the widget
knowing.

## The design

### One call

```rust
r.update(|s| s.count += 1);   // that is the whole API
```

The node is marked dirty. On the next frame:

1. That node re-runs `layout` with its recorded constraints.
2. **Size unchanged** → re-record its picture only. Parent untouched,
   siblings replay from cache, damage covers this node.
3. **Size changed** → mark the parent dirty and repeat upward, stopping as
   soon as an ancestor's size stops changing.

Step 3 is Flutter's relayout-boundary propagation, derived by comparison
instead of declared by anyone.

### Nested nodes become cache boundaries

Give `PaintCtx::child`'s nodes the same three fields `walk_element` sets, and
the same two checks:

* skip layout when `last_constraints` match and `!paint_dirty`
* replay `cached_picture` when `!paint_dirty` and the rect is unchanged

Then a `Column` painting ten children re-records only the dirty one.

This is the bulk of the work and where the risk is. It changes the paint
path for every widget, so it lands behind a test that proves the cheap path
is taken, not merely that pixels are right.

### Thread-safety dictates the handle

Hit callbacks are `Arc<dyn Fn() + Send + Sync>`; the render tree is
`Rc<RefCell<..>>` and not `Send`. So a refresh handle cannot hold the tree.
It holds `Arc<Mutex<T>>` (the state, shared with the node) and a `NodeId`
(`Copy`). `update()` mutates directly, then pushes the id onto a
thread-local queue the engine drains.

Same queue-and-drain shape already used by `rosace_core::a11y::actions` and
`rosace_core::nav_back` — two working precedents, not a new invention.

### Identity

State is keyed by render-tree slot, which is positional: insert a row at the
top of a list and every row's state shifts down one. `Stateful::keyed(id, ..)`
uses `child_keyed`, which already exists, is tested, and is used today by
`ScreenTransitionView`.

## Stages

Each ships green and is independently useful.

**Stage 1 — targeted repaint frames.**
Split `forced_repaint` into "repaint everything" (resize) and "repaint the
nodes I marked". `root_is_dirty || hover_frame` becomes `root_is_dirty` on
targeted frames.
*Fixes hover-repaints-the-whole-screen on its own, with no new API.*

**Stage 2 — per-node cache boundaries.**
`cached_size` / `last_constraints` / `cached_picture` on nodes created by
`PaintCtx::child`, with the two skip checks. Reuses `ctx.capture` and
`ctx.replay_offset` from `RepaintBoundary`.
*The load-bearing stage. Everything after it is small.*

**Stage 3 — size-change propagation.**
After a dirty node re-layouts, compare against `cached_size`; if it differs,
mark the parent dirty and walk up until the size stops changing.

**Stage 4 — per-node state + the refresh queue.**
`ctx.widget_state::<T>(init)` following the `scroll_controller` pattern
exactly, plus the thread-local queue and its drain.

**Stage 5 — the `Stateful` widget.**
Ties it together. One `update`. Optional `keyed`.

**Stage 6 — prove it.**
Build 100 widgets, refresh one, assert the other 99 replay from cache and
the damage rect covers only the refreshed node. **Written to fail against
today's code first**, or it proves nothing.

**Stage 7 — arena reclamation.**
The arena never frees (`render_tree.rs:378`), so detached nodes keep their
state, controllers and cached pictures forever. Tolerable when it held
animation channels; a real leak once apps and cached pictures live there.
*A prerequisite for calling this done, not an optimisation.*

## Phase 2 — config comparison (approved, scoped separately)

This plan makes `refresh()` precise. It does NOT make atom writes precise.

A `GlobalAtom` change — a theme flip, an OS text-scale change — still marks
the root dirty, so `build()` regenerates the whole widget tree. A node then
has no way to know its new widget is equivalent to the old one: the widget
is a fresh object, and the only way to find out whether the picture changed
is to re-record it, which is the work we were trying to skip.

The fix is Flutter's `updateRenderObject` shape — hand the node the new
config, let it compare against the old, mark dirty only on a real
difference. That needs widgets to be comparable: `PartialEq`, or a cheap
config hash stored on the node. Mechanical across 82 widget files, but wide.

Deliberately deferred, not forgotten. Refreshes are every keystroke; theme
flips are user-initiated and rare, so the common case is worth fixing first.

## Non-goals

* **Component nesting / honouring `Element.key`.** Still a real gap. This
  plan does not need it — per-node caches give per-widget granularity
  without it. Revisit after.
* **Removing `Atom`.** It stays. `GlobalAtom` backs the theme, media query
  and app lifecycle — framework internals with no other home. What changes
  is that local widget state stops needing it.
* **An `InheritedWidget` equivalent.** Separate decision. `PaintCtx` already
  carries `theme` and `font` down, which is the shape it would generalise.

## Risk, stated plainly

Stage 2 changes the paint path for all 82 widget files. The failure mode is
a stale cache — a widget that should have repainted and did not — which is
invisible in a headless test that only checks the final pixels. Stage 6's
test must assert **which path was taken**, not just the output.

The mitigating fact: `RepaintBoundary` has been doing exactly this caching
in production, so the primitives are proven; what changes is where the cache
lives.
