# Phase 13 — Persistent RenderNode Tree + Reconciler

> Status: COMPLETE
> Started: 2026-07-01
> Completed: 2026-07-01
> Target: Surgical widget updates — only dirty subtrees re-layout and re-paint

## Why This Phase

Phase 12 renders 100% of every widget on every atom change. A counter increment
repaints every button, card, and text in the entire window. This is wasteful.

Phase 13 introduces a **persistent RenderNode tree** that caches each widget's
last layout size and paint Picture. On the next frame only the widgets whose
ancestors changed run layout + paint again. Everything else replays its cached
display list — zero work.

The companion change is **O(depth) hit testing**: the existing flat
`Vec<HitTarget>` scan is replaced by a depth-first walk of the RenderNode tree.

---

## Decisions

- D065 — Persistent RenderNode Tree
- D066 — Reconciler Algorithm (type + position + key matching)
- D067 — Dirty-Flag Layout and Paint
- D068 — O(depth) Hit Testing

---

## Steps

### Step 1 — Make `Picture` cloneable (rosace-render) ✅
- Add `#[derive(Clone)]` to `Picture` in `rosace-render/src/picture.rs`
- This allows caching pictures in `RenderNode`

### Step 2 — `RenderNode` struct (rosace/src/render_node.rs)
```rust
use std::sync::Arc;
use rosace_core::types::{Key, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Picture;

pub struct RenderNode {
    pub tag:   &'static str,
    pub key:   Option<Key>,

    // Layout cache
    pub last_constraints: Option<Constraints>,
    pub cached_size:      Option<Size>,

    // Paint cache
    pub cached_picture: Option<Arc<Picture>>,
    pub cached_rect:    Option<Rect>,
    pub paint_dirty:    bool,

    // Hit testing
    pub hit_handlers: Vec<Arc<dyn Fn() + Send + Sync>>,

    // Tree structure
    pub children: Vec<RenderNode>,
}
```

- `RenderNode::new(tag, key)` — creates a dirty node with all caches empty
- `RenderNode::invalidate()` — sets `paint_dirty = true`, clears picture cache
- Export from `rosace` crate (internal use only, not public API)

### Step 3 — Reconciler (rosace/src/reconcile.rs)
```rust
pub fn reconcile(old: &mut Vec<RenderNode>, new_elements: &[Element]) { ... }
```

DFS position-based algorithm:
1. For each `(index, new_element)` in `new_elements`:
   - Find `old[index]` if it exists
   - Match: same tag + key agreement → **stable**: keep node, clear hit_handlers
   - Mismatch or no old node → **fresh**: insert new `RenderNode::new(...)`, paint_dirty=true
2. Truncate `old` if `new_elements` is shorter (old nodes beyond len are dropped)

For `Element::Component`:
  - Recursively reconcile children of the expanded subtree
  
For `Element::Native`:
  - Reconcile at the current position, then recurse into `element.children`
  
For `Element::Text` and `Element::Empty`:
  - Use a synthetic tag ("__text__", "__empty__") for matching
  - These nodes have no children

**Keyed children** (bonus — if any sibling has a key):
  - Partition siblings into keyed and unkeyed sublists
  - Match keyed by key first; match unkeyed by position within unkeyed sublist
  - Unmatched old keyed nodes → dropped

### Step 4 — Dirty-flag layout + paint walk (rosace/src/render_walk.rs)
Replace `walk_element()` in `rosace/src/lib.rs` with two separate passes:

**Layout pass** — `layout_tree(node, element, ctx)`:
```
if node.last_constraints == Some(ctx.constraints) && !node.paint_dirty:
    return node.cached_size.unwrap()   // ← skip entire subtree
else:
    size = widget.layout(&ctx)
    node.last_constraints = Some(ctx.constraints)
    node.cached_size = Some(size)
    node.paint_dirty = true
    return size
```

**Paint pass** — `paint_tree(node, rect, ctx)`:
```
node.cached_rect = Some(rect)
if !node.paint_dirty && node.cached_picture.is_some():
    canvas.play_picture(node.cached_picture.as_ref(), font)  // ← skip re-paint
else:
    let mut recorder = PictureRecorder::new()
    widget.paint(&mut child_ctx)   // child_ctx uses recorder
    let picture = recorder.finish()
    node.cached_picture = Some(Arc::new(picture))
    node.hit_handlers = child_ctx.take_hit_handlers()
    node.paint_dirty = false
```

### Step 5 — O(depth) hit testing (rosace/src/hit_test.rs)
```rust
pub fn hit_test(node: &RenderNode, x: f32, y: f32) -> bool {
    // Test children first (depth-first, deepest wins)
    for child in node.children.iter().rev() {
        if hit_test(child, x, y) { return true; }
    }
    // Then this node
    if let Some(rect) = node.cached_rect {
        if x >= rect.origin.x && x <= rect.origin.x + rect.size.width
        && y >= rect.origin.y && y <= rect.origin.y + rect.size.height {
            for h in &node.hit_handlers {
                (h)();
                return true;
            }
        }
    }
    false
}
```

Overlay entries still tested first in insertion order, before main tree.

### Step 6 — Wire into App::launch (rosace/src/lib.rs)
- Add `render_tree: Vec<RenderNode>` to the closure captured by `PlatformWindow::run()`
- Before each paint: call `reconcile(&mut render_tree, &element_children)`
- Run layout pass → paint pass from render_tree
- On mouse click: call `hit_test` on render_tree (deepest child first)
- Remove the old `Vec<HitTarget>` from `PaintCtx` (now tracked in RenderNodes)

### Step 7 — Phase 13 demo (rosace-examples/src/bin/phase13_demo.rs)
A demo that makes the dirty-flag behavior visible:
1. **Frame counter panel** — shows `frame_counter` (increments each render). A separate label shows a high-frequency clock. Proves that the clock updates without re-rendering static panels.
2. **1000-item list panel** — renders 1000 text items. A counter shows how many items re-painted last frame. Updating one item should show count=1, not count=1000.
3. **Key reorder panel** — a list of 5 items that can be shuffled. With keys: shuffle → only moved items re-paint. Without keys: every item re-paints.

---

## Exit Criteria

```
□ RenderNode tree is created and persists across frames in App::launch
□ Reconciler matches nodes by tag+position; key matching for keyed siblings
□ Layout skipped for nodes where constraints match cached value
□ Paint skipped for nodes where paint_dirty=false and cached_picture exists
□ Hit testing walks RenderNode tree depth-first (deepest child wins)
□ Overlays still tested before main tree
□ phase13_demo renders correctly and shows dirty-node counts
□ All workspace tests pass with zero warnings
□ cargo check --release --workspace: zero warnings
```

---

## Approved dependencies
- No new crates. Uses only `std::sync::Arc` for Picture sharing.

## DO NOT
- DO NOT add GPU compositing (Phase 15)
- DO NOT add parallel layout (Rayon)
- DO NOT rename Widget trait to RenderObject yet
- DO NOT implement RepaintBoundary (Phase 14)
- DO NOT wire focus system end-to-end (Phase 14)
