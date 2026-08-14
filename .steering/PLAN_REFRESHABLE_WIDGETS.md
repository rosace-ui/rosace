# Plan — refreshable widgets, and honest rebuild granularity

Status: PROPOSED, not started. Written 2026-08-14.

## The problem, measured

Change one `Text` from `1` to `2` in a screen of 100 widgets and **all 100
re-layout and re-record their pictures**, and the damage rect covers the
screen.

The chain, verified in code rather than assumed:

1. `atom.set()` marks its subscribers dirty — precisely, by `ComponentId`
   (`rosace-state/src/atom.rs`).
2. Only ONE component exists. `walk_element` never recurses into
   `Element::Native`'s `children` (grep for `n.children` in
   `rosace/src/lib.rs` — no hits), and a `Widget` always becomes a
   `NativeElement` with `children: vec![]`. So the walk finds exactly one
   `Element::Component`: the root.
3. The root rebuilds → `build()` produces all 100 widgets afresh.
4. `subtree_dirty` (`rosace/src/lib.rs`, the `root_is_dirty || hover_frame`
   argument) propagates down and sets `node.paint_dirty = true` on every
   node.
5. Both caches consult `!paint_dirty` — the layout cache and the picture
   replay — so both are defeated. Everything re-layouts, everything
   re-records, damage unions every rect.

**The reactivity machinery is not the problem.** Subscription-by-read,
subscriber-precise marking, per-component dirty checks and per-node damage
tracking are all correct and already fine-grained. They are being handed a
tree with one component in it, so "the dirty subtree" is always "everything".

Two consequences worth naming plainly:

* The README's "subscriber-precise rebuilds — no re-render-the-world" is
  currently the opposite of what happens.
* **Hover repaints the whole screen too** — `hover_frame` is OR'd into
  `subtree_dirty` on the same line.

## What we take from Flutter and Android, and what we refuse

Not a port. Each borrowing is one idea, taken because it fits what is
already here.

**From Flutter — the config/state split.** A `StatefulWidget` is cheap and
disposable; its `State` is retained by the element. We take exactly that
shape: the widget is rebuilt freely, the state lives on the render-tree node.

We refuse the packaging. No `createState()`, no `State<T>` subclass, no
`setState(() {})` whose closure exists only to signal. Closures give the same
thing without the ceremony. We also refuse the third tree — Flutter has
Widget/Element/RenderObject; ROSACE has element tree + render tree, and two
is enough.

**From Android — `invalidate()` versus `requestLayout()`.** A View
distinguishes "my pixels changed" from "my size changed", and the second is
much more expensive because it propagates to the parent. Our `TreeNode`
already caches `cached_size` and `cached_picture` separately, so we get the
same distinction almost free:

* `refresh_paint()` — invalidate the picture, keep the cached size. A
  counter's text changing width-neutrally, a colour change, a hover state.
* `refresh_layout()` — invalidate both. The widget's size may change, so the
  parent must re-measure.

Defaulting to the cheap one and making the expensive one explicit is the
whole value of copying this.

We refuse Android's `ViewGroup` mutation model — our widgets are values
rebuilt from a builder, not long-lived mutable objects.

## The design

```rust
Stateful::new(
    // init — runs ONCE per render-tree node, not per frame
    || Counter { n: 0 },
    // build — runs on every paint of THIS node only
    |s: &Counter, r: &Refresh<Counter>| {
        let r = r.clone();
        Button::new(s.n.to_string())
            .on_press(move || r.update(|s| s.n += 1))
    },
)
```

`r.update(f)` mutates the state and marks **only that node** dirty. Its
siblings, ancestors and the other 99 widgets keep `paint_dirty == false`,
hit the layout cache and the replay path, and contribute nothing to damage.

### Why this needs no component nesting

The retained-state pattern already exists and is proven three times over:
`ctx.scroll_controller()`, `ctx.focus_node()` and the animation channels all
lazily create per-node state and keep it across frames, keyed by render-tree
slot (`rosace-widgets/src/tree/mod.rs`). `Stateful` is a generic version of
code that already ships.

And a repaint-without-rebuild frame already exists: animation drives one via
`take_animation_request()` → `forced_repaint`. On such a frame the root
component is NOT dirty, so the cached element is reused and no `build()`
runs — exactly the path a targeted refresh needs.

### Thread-safety, which dictates the shape

Hit callbacks are `Arc<dyn Fn() + Send + Sync>`, and the render tree is
`Rc<RefCell<..>>` — not `Send`. So a `Refresh` handle **cannot hold the
tree**. It holds:

* `Arc<Mutex<T>>` — the state, shared with the node
* `NodeId` — plain `Copy`

`update()` locks and mutates directly, then pushes the `NodeId` onto a
thread-local refresh queue. The engine drains it on the next frame and marks
those nodes dirty. Same queue-and-drain shape already used for accessibility
actions (`rosace_core::a11y::actions`) and the back intent
(`rosace_core::nav_back`) — a pattern with two working precedents rather
than a new invention.

### Identity

State is keyed by render-tree slot, which is positional — so a conditionally
inserted sibling shifts slots and state migrates to the wrong widget.

The render tree **already solves this**: `keyed_children` and
`prune_keyed_children` exist and work (`PaintCtx::child_keyed`). `Stateful`
takes an optional key and uses the keyed path. Unlike the element tree's
unused `Element::key`, this one is real, tested and in use today by
`ScreenTransitionView`.

## Staged plan

Each stage ships green and is independently useful.

**Stage 1 — targeted repaint frames.**
Split `forced_repaint` into "repaint everything" (resize) and "repaint the
nodes I marked". The one-line `root_is_dirty || hover_frame` becomes
`root_is_dirty` for targeted frames.
*Fixes hover-repaints-everything on its own, before any new API exists.*

**Stage 2 — generic per-node state.**
`ctx.widget_state::<T>(init)`, following the `scroll_controller` pattern
exactly. Store as `Option<Arc<Mutex<dyn Any + Send + Sync>>>` on `TreeNode`.

**Stage 3 — the refresh queue.**
Thread-local `Vec<NodeId>` plus `refresh_paint`/`refresh_layout`, drained by
the engine, marking `paint_dirty` (and clearing `cached_size` for the layout
variant).

**Stage 4 — the `Stateful` widget.**
Ties 2 and 3 together with the closure API above, plus `keyed` identity.

**Stage 5 — prove it.**
A test that builds 100 widgets, refreshes one, and asserts the other 99
replay from cache and the damage rect covers only the refreshed node. This
is the stage that makes the claim true rather than plausible, and it should
be written to FAIL against today's code first.

**Stage 6 — arena reclamation.**
The render-tree arena never frees (documented at `render_tree.rs:378`), so
detached nodes keep their state, controllers and cached pictures forever.
Tolerable when it was animation channels; a real leak once apps put their
state there. Either reclaim orphans in `finalize` or add a free list.

*Stage 6 is a prerequisite for calling this done, not an optimisation.*

## Non-goals

Explicitly NOT in this plan, to keep it small:

* **Component nesting / honouring `Element.key`.** Still a real gap, but
  this plan does not need it. Revisit after.
* **Removing `Atom`.** It stays as-is. `GlobalAtom` still backs the theme,
  media query and app lifecycle — framework internals with no other home.
  What changes is that local widget state stops needing it.
* **An `InheritedWidget` equivalent.** Subtree-scoped values are a separate
  decision. `PaintCtx` already carries `theme` and `font` down, which is the
  shape it would generalise.
* **A lighter state cell.** One atom costs ~150-200 bytes and 3-4
  allocations (two `Vec`s for subscribers, a `Mutex`, an id) to hold a
  `bool`. Component-owned state has exactly one subscriber and needs none of
  that — but `Stateful` sidesteps it by not using `Atom` at all, so the
  optimisation is moot here.

## Why this order

Stage 1 delivers a real fix with no new API. Stage 5 is the proof, written
to fail first. Stage 6 is where it becomes honest rather than merely
working — shipping stages 1-5 without it trades a rebuild problem for a
memory leak.
