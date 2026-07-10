# Phase 14 — Focus End-to-End + Navigation Route Stack + RepaintBoundary

> Status: COMPLETE
> Started: 2026-07-01
> Completed: 2026-07-01

## Why This Phase

Phase 12 added the FocusNode graph (D063) and Phase 13 added the RenderNode
picture cache. Phase 14 completes three missing wires:

1. **Focus wired end-to-end (D069)**: Tab key cycles through focusable widgets;
   focused widget renders with visual focus ring. FocusManager drives the cycle.

2. **Navigation route stack (D070)**: Navigator component manages a screen stack.
   push_route/pop_route. Frozen routes kept in memory, not re-rendered.

3. **RepaintBoundary (D071)**: Explicit per-subtree Picture isolation. Invalidating
   one boundary does not repaint siblings.

---

## Decisions

- D069 — Focus System End-to-End Wiring
- D070 — Navigation Route Stack Wiring  
- D071 — RepaintBoundary Widget

---

## Steps

### Step 1 — Focus wiring (rosace-a11y + rosace + rosace-widgets)

**FocusManager (rosace-a11y/src/focus_manager.rs)**:
```rust
pub struct FocusManager {
    pub focused_id: Option<u64>,    // currently focused node id
    order: Vec<u64>,                // DFS tab-order, rebuilt each frame
}
impl FocusManager {
    pub fn sync(&mut self, ordered_ids: Vec<u64>) { self.order = ordered_ids; }
    pub fn focus_next(&mut self) { ... }
    pub fn focus_prev(&mut self) { ... }
    pub fn set(&mut self, id: u64) { self.focused_id = Some(id); }
    pub fn clear(&mut self) { self.focused_id = None; }
    pub fn is_focused(&self, id: u64) -> bool { self.focused_id == Some(id) }
}
```

**PaintCtx extension** (rosace-widgets/tree):
- Add `focused_id: Option<u64>` to `PaintCtx`
- Pass it from `App::launch` frame state

**App::launch changes**:
- Add `focus_manager: FocusManager` to frame-persistent state
- Collect `focusable_ids: Vec<u64>` during walk (widgets that impl FocusApi register themselves)
- After paint pass: `focus_manager.sync(focusable_ids)`
- On `KeyboardInput { key: Tab }` event:
  - `Shift+Tab` → `focus_manager.focus_prev()`
  - `Tab` → `focus_manager.focus_next()`

**Widget focus rendering**:
- Focused widget detects via `ctx.focused_id == Some(self.focus_id)`
- Draws a focus ring (2px border, accent color)
- `Button`, `TextInput`, `Checkbox`, `Slider` update to check

### Step 2 — Navigator (rosace-nav)

**Fill in rosace-nav/src/stack.rs**:
```rust
pub struct RouteEntry {
    pub id:        RouteId,
    pub component: Arc<dyn Component>,
}

pub struct Navigator {
    routes: Atom<Vec<Arc<RouteEntry>>>,
}

impl Navigator {
    pub fn new(root: impl Component + 'static) -> Self { ... }
    pub fn push(&self, c: impl Component + 'static) { ... }
    pub fn pop(&self) { ... }
    pub fn can_pop(&self) -> bool { self.routes.get().len() > 1 }
}

impl Component for Navigator {
    fn build(&self, ctx: &mut Context) -> Element {
        let routes = ctx.state(self.routes.get());
        // Only build the top route.
        if let Some(top) = routes.get().last() {
            top.component.build(ctx)
        } else {
            Element::Empty
        }
    }
}
```

**Route transitions** (no animation in Phase 14 — instant switch):
- `push()` → atom.set adds new entry to vec
- `pop()` → atom.set removes last entry; fires `on_unmount` for the popped component

### Step 3 — RepaintBoundary (rosace-widgets + rosace)

**Widget** (rosace-widgets/src/tree/repaint_boundary.rs):
```rust
pub struct RepaintBoundary<W: Widget> {
    pub child: W,
}
impl RepaintBoundary<W> {
    pub fn new(child: W) -> Self { ... }
}
impl Widget for RepaintBoundary<W> {
    fn layout(&self, ctx: &LayoutCtx) -> Size { self.child.layout(ctx) }
    fn paint(&self, ctx: &mut PaintCtx) { self.child.paint(ctx) }
    fn tag() -> &'static str { "RepaintBoundary" }
}
```

**In walk_element** — special handling for `"RepaintBoundary"` tag:
- The child of a RepaintBoundary is always laid out fresh (constraints may change)
- Paint: if `!node.paint_dirty && cached_picture.is_some()` → replay own picture, skip entire child subtree
- This requires child native elements not to increment `native_idx` when the boundary's picture is replayed — the subtree is frozen

Implementation shortcut: RepaintBoundary records its ENTIRE child subtree into a single sub-Picture on first paint, then replays it as one unit.

### Step 4 — Phase 14 demo (rosace-examples/src/bin/phase14_demo.rs)

Three panels:
1. **Focus demo** — Tab through 4 buttons/inputs, active focus shows ring
2. **Navigator demo** — push/pop between two screens (screen A, screen B)
3. **RepaintBoundary demo** — fast-updating clock inside a boundary; counter outside; show that clock repaints don't affect the counter panel

---

## Exit Criteria

```
✅ Tab key cycles focus through focusable widgets in DFS order
✅ Shift+Tab cycles backwards
✅ Focused widget shows a visible focus ring
✅ ScreenNav.push() transitions to new screen
✅ ScreenNav.pop() returns to previous screen (state preserved via atom)
✅ RepaintBoundary: child subtree is a single cached Picture unit
✅ RepaintBoundary: delegates layout and paint to child (cache at walk_element level)
✅ phase14_demo renders correctly (Focus / Navigator / RepaintBoundary panels)
✅ All workspace tests pass with zero warnings
```

---

## Approved dependencies
- No new crates.

## DO NOT
- DO NOT animate route transitions (Phase 15)
- DO NOT add GPU compositor (Phase 15)
- DO NOT add accessibility tree serialization (Phase 16)
- DO NOT add IME / complex text input (Phase 16)
