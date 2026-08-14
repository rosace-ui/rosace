# Plan — widget-owned state, and retiring `Atom` from widget APIs

Status: PROPOSED, awaiting approval. Written 2026-08-14, after reading the
widgets rather than reasoning from the framework's history.

**This is not a refactor.** The pattern it describes is already the house
style in the most-used widgets; what remains is one new method and nine
widget signatures.

---

## 1. What already works, verified

`Button` holds no state and uses no `Atom`:

```rust
let target = if self.disabled { 0.0 }
             else if ctx.pressed() { 1.0 }
             else if ctx.hovered() { 0.5 }
             else { 0.0 };
let emphasis = ctx.animate_to(target, 0.0);
```

`pressed` and `hovered` are node fields the input dispatcher owns;
`set_pressed`/`set_hover` mark `needs_paint` on that exact node.
`animate_to` stores the animation value on the node and asks for the next
frame. Nothing rebuilds, nothing global is involved.

`Switch::new(on: bool)` + `.on_change(|bool|)`, `Checkbox::new(checked: bool)`
and `Radio::new(selected: bool)` already take a plain value and report
changes through a callback. `Tooltip` reacts to the pointer through
`ctx.hoverable()` / `ctx.hovered_within()`.

So the target design is already proven in production widgets. The gap is
narrower than it looks.

## 2. What is left, and it is three problems, not one

Fourteen places still take an `Atom`:

**(a) Open/close state — 9, the real target.**
`Accordion(expanded)`, `Dropdown(open)`, `Drawer(open)`,
`Autocomplete(open)`, `Dialog::emit(open)`, `overlay_api::{dropdown, sheet,
dialog, toast}(open)`, `Toast::show(open)`, `Snackbar::show(open)`.

This is widget-local UI state living in a public constructor. To open an
accordion the CALLER must construct an `Atom`.

**(b) Controlled value — 2, already the right shape.**
`Carousel::page(Atom<usize>)` is OPTIONAL — internal by default, external
when you want it. `TransformLayer::new(.., scroll_y: Atom<f32>)` is
required and should become optional the same way.

**(c) Not state at all — 3, leave alone.**
`RectReader::new(Atom<Option<Rect>>, ..)` writes a measured rect back OUT to
the caller. `RepaintBoundary::repaint_when(Atom<u64>)` is an explicit
invalidation trigger. These are channels, not state, and converting them
would be a different design with different reasoning.

## 3. The design

### One method, no new widget kind

```rust
let open = ctx.widget_state(|| false);   // node-owned, survives repaints
open.set(true);                          // marks this node dirty
```

There is deliberately **no `Stateful` widget type**. Every widget can
already hold node state — `Button` does — so a second widget kind would add
a concept, a name to argue about, and a migration, to enable something the
node already supports. A widget that never calls `widget_state` never gets
state.

This also answers "does every widget need the refresh capability?": they
already have it. Today only the dispatcher can reach it. This opens the same
door to the widget itself.

### No explicit `refresh()` in the common path

Mutating through the handle marks the node. There is no second call to
forget and no way to mutate without invalidating. `refresh()` survives only
as the escape hatch for state the framework cannot see (a widget wrapping
something external), plus the public node-level mark that plugins need.

### Which flag gets marked — the rules, and why each holds

| cause | `needs_layout` | `needs_paint` | why |
|---|---|---|---|
| rebuild, resize, theme | yes | yes | every widget object is new; nothing is comparable |
| type change in a slot | yes | yes | a different widget entirely |
| **widget state mutation** | yes | yes | see below |
| hover / press | no | yes | `LayoutCtx` exposes only `constraints`, `font`, `theme` — it has no node access, so size provably cannot depend on them |
| animation tick | no | yes | `animate_to` lives on `PaintCtx`; `layout` cannot reach it |

The third row is the load-bearing one. The framework cannot know whether a
state change affected size — that depends on font metrics, text scale and
the widget's internals. So it does not ask. It marks both, re-measures **that
one widget**, and compares the result against `cached_size`. Unchanged size
stops there; changed size propagates to the parent and repeats.

Measuring is cheaper and more correct than any annotation a caller could
write, and a wrong annotation fails silently as a rendering bug. This is the
same principle that rejected `refresh_paint`/`refresh_layout`.

### Storage and identity

State lives on the node, indexed positionally within that node's paint (the
hook shape), and is therefore governed by machinery that already exists:

* `adopt_tag` clears it when a slot changes widget type — already built and
  tested, so a `Button` can never inherit a `TextField`'s state.
* `finalize`'s truncate detaches it when the widget leaves the tree, so a
  returning widget starts fresh — Flutter's dispose semantics.

Positional indexing carries React's conditional-hook footgun: calling
`widget_state` inside an `if` shifts every later index. Mitigation is a
debug-only assertion that a node's state count matches its previous paint,
which turns a silent state-swap into a loud failure in development.

### Threading

The tree is `Rc<RefCell<..>>` and not `Send`; hit callbacks are
`Arc<dyn Fn() + Send + Sync>`. So the handle cannot hold the tree. It holds
`Arc<Mutex<T>>` (shared with the node) plus a `NodeId` (`Copy`), and `set`
mutates then queues the id. `dirty_set` already routes marks to the owning
thread for exactly this reason (`mark_dirty_for_thread`), so an off-thread
mutation is handled rather than silently lost.

## 4. Benefits

**1. The public API loses a required parameter it never should have had.**

```rust
// today
Accordion::new("Details", expanded_atom, body)
// after
Accordion::new("Details", false, body).on_change(|open| ...)
```

Nine widgets stop demanding that callers construct state plumbing to use
them. It also makes them usable from `view!`/template inflation, where
constructing an `Atom` per instance is awkward.

**2. It stops bypassing the caching work just landed.**

An `Atom` write dirties the COMPONENT, so `build()` re-runs, which makes the
frame **structural**, which by design ignores every per-node cache. Today,
toggling one accordion in a list of twenty repaints all twenty plus
everything else that component owns — the exact interaction users perform
most often is the one that defeats the caches. Node-owned state produces a
**targeted** frame, so one widget repaints.

**3. Consistency.** `Button`, `Switch`, `Checkbox` and `Radio` already work
this way. Nine widgets currently do not, for no reason a user can see.

**4. Plugins get widget granularity.** `external::Subscribers` is
component-granularity by its own documentation, so a BLoC or signal library
can only trigger a whole `build()`. Making the node-level mark public gives
third-party state libraries the same precision the built-ins get — which is
what "plugins are a core principle" has to mean in practice.

**5. `Atom` is demoted, not deleted.** It stays for what it is genuinely good
at — `GlobalAtom` backing theme, media query and app lifecycle — and as the
OPT-IN external control for widgets that want it. Local widget state stops
being forced through a global-ish reactive primitive.

## 5. Advantages over the alternatives

**vs. making `Atom` writes precise (deferred Phase 2).** That needs config
comparison so a node can tell a fresh-but-identical widget from a changed
one — `PartialEq` or a config hash across 82 widget files. Mechanical but
wide. Node-owned state needs none of it, because nothing rebuilds and there
is nothing to compare.

**vs. Flutter's `StatefulWidget`.** No second widget kind, no `State` class,
no `initState`/`didUpdateWidget`/`dispose` ceremony for the common case. The
node already provides the lifetime, and `adopt_tag` + truncate already
provide the identity and disposal rules.

**vs. leaving it alone.** The caches landed in `1f29c8f` only pay off on
targeted frames. Leaving widget state on atoms means the most common
interactions stay structural, and the work does not reach users.

## 6. Scope, stated so it cannot creep

**In:**
1. `PaintCtx::widget_state::<T>()` plus the node storage and the mark queue.
2. Convert the 9 open/close widgets to `value + on_change`, keeping the
   `Atom` as an OPTIONAL override (`Carousel::page`'s existing shape).
3. `TransformLayer::scroll_y` becomes optional the same way.
4. Public node-level dirty mark for plugins.

**Out:**
* No new widget kind, no `Stateful` type, no naming decision.
* No engine or walker changes.
* No `Element` work — that is A7, deliberately deferred.
* `RectReader` and `RepaintBoundary::repaint_when` untouched.
* No change to `GlobalAtom` or to `Atom` itself.

## 7. Risks

* **Conditional `widget_state` calls** shift positional indices. Mitigated by
  a debug assertion on count stability; named here so it is not discovered
  as a bug report.
* **Nine public API changes** are breaking for any app already passing an
  `Atom`. The optional-override form means the old call site can be kept
  working through a `.controlled_by(atom)` builder rather than a hard break.
* **State outliving its widget** if a node is detached but never freed —
  that is the arena leak, task 17, and it should land alongside rather than
  after.
