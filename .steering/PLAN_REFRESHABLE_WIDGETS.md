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

**2b. `Row`/`Column` no longer double-measure** (fixed 2026-08-14,
`9b4ba78`). Both ran two passes over their children, the first keeping only
the main-axis extent and the second re-measuring the same children with
identical constraints. Now the first pass keeps each `Size` and the second
reuses it: 2× → 1× layout calls per non-flex child.

Worth recording because the obvious inference is wrong: it does **not**
compound with depth. Each `Row`/`Column` memoises `measure` by constraints
(`measure_cache`), so a parent's second call on a nested flex child hits
that cache rather than recursing. Measured on nested Columns at depth 5 —
486 → 243 leaf layouts, a flat 2×, not 2^depth.

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
2. **Size unchanged** → re-record its picture only. Siblings replay from
   cache, damage covers this node, and **nothing above it re-layouts**.
3. **Size changed** → mark the parent dirty and repeat upward, stopping as
   soon as an ancestor's size stops changing.

Step 3 is Flutter's relayout-boundary propagation, derived by comparison
instead of declared by anyone.

### Correction: "parent untouched" is false, and why

An earlier draft of step 2 said *"Parent untouched."* That cannot hold,
because `Picture` is a flat `Vec<DrawCommand>` (`rosace-render/src/picture.rs:9`)
— fully flattened, with no nested or by-reference sub-pictures.

If a parent is clean and replays *its own* cached picture, that picture
still holds the **old** child's commands. The child's new colour would
never reach the screen: a stale cache that renders plausible pixels, which
is the failure mode this whole plan is meant to avoid.

So three different things propagate three different distances, and
conflating them is what produced the wrong claim:

| | how far up |
|---|---|
| **Layout** | stops at the first ancestor whose size is unchanged — for a colour change, immediately |
| **Display-list assembly** | to the app root, always |
| **Rasterization** | damage rect only |

Assembly is not recomputation, and that distinction is the entire win: on
a ten-child `Column` with one child dirty, the nine siblings' `layout` and
`paint` **do not run** — their recorded commands are copied. Ancestors
re-run `paint` (their own background plus `paint_child` calls) but not
`layout`. Ten widgets' paint logic becomes one, plus nine memcpys.

**The additive fix, deliberately not in Stage 2.** Make pictures nest — a
parent holding `Arc<Picture>` references to its children instead of their
flattened commands. Then a dirty child swaps one `Arc`, assembly is O(1)
rather than O(commands), the parent genuinely *is* untouched, and the walk
stops at the dirty node instead of reaching the root.

That is a change to `Picture`/`DrawCommand` in `rosace-render`, one layer
below this work. Stage 2 is correct without it. Land the
correct-but-copying version first rather than changing two layers at once;
revisit when assembly shows up in a profile.

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

Each ships green and is independently useful — **except the first two,
which do not separate.** See below.

**Stage 1 — targeted repaint frames. CANNOT SHIP ALONE (verified
2026-08-14).**
The original claim was that splitting `forced_repaint` into "repaint
everything" and "repaint the nodes I marked" fixes
hover-repaints-the-whole-screen *on its own, with no new API*. That is
wrong, and the reason is finding 3 above taken to its conclusion.

Instrumenting the one place `cached_picture` is written
(`rosace/src/lib.rs:499`) over a ten-child `Column` prints exactly one line:

    [PICCACHE] node 1 tag=..::column::Column PAINTED

**One** node holds a picture for the entire screen. So "repaint only the
nodes I marked" and "repaint everything" are the same instruction — there
is one node to mark. Worse, dropping `hover_frame` from `subtree_dirty`
would leave that single node clean, so the whole screen would replay a
picture recorded *before* the hover: hover would stop working rather than
get cheaper.

Stage 1 has no standalone effect. It is the frame-classification half of
Stage 2 and lands with it.

**Stage 2 — per-node cache boundaries.** *(absorbs Stage 1)*
`cached_size` / `last_constraints` / `cached_picture` on nodes created by
`PaintCtx::child`, with the two skip checks.
*The load-bearing stage. Everything after it is small.*

Two things found while verifying, which shape the work:

**The marking already exists.** `RenderTree::set_hover` and `set_pressed`
(`render_tree.rs:665`, `684`) already set `paint_dirty = true` on the exact
node entered and the exact node left, and clear it on the old one. That
precision is thrown away today only because those nested nodes have nowhere
to cache — so the engine falls back to forcing a global repaint. Give them
a cache and hover becomes surgical with no new marking code.

**Frames must be classified, and this is the stale-cache hazard.** A nested
node's `paint_dirty` is set by hover/press, but *nothing* sets it when the
widget's content changes — a rebuilt tree hands the node a brand-new widget
object it cannot compare against the old one (that is Phase 2 below). If
nested nodes cache unconditionally, a `Text` going `1` → `2` would replay
the stale `1`. So:

* **Structural frame** — element rebuilt, `global_dirty`, or resize:
  propagate dirty downward, everything repaints. Today's behaviour, kept
  deliberately, because it is the safe one.
* **Targeted frame** — no rebuild, only hover/press/marked nodes changed:
  do not propagate; only `paint_dirty` nodes repaint, siblings replay.

That classification *is* Stage 1, which is why the two fuse.

**The mechanical part.** Caching per node requires the framework to own a
sub-recorder around each child paint, which the current idiom does not
allow — `child.paint(&mut ctx.child(rect))` gives no "after the child
painted" hook. It becomes `ctx.paint_child(rect, child)`, with the
framework doing the cache check, the sub-recorder and the store. 80 call
sites across 48 files, uniform enough to convert mechanically.

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
