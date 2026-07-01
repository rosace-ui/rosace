# TEZZERA — Render Engine Architecture

> This document is the authoritative reference for how TEZZERA renders a frame,
> routes input, manages focus, and handles overlays and navigation.
> Decisions referenced here are locked in DECISIONS.md.

---

## The Four Trees

Every frame is the result of four trees working together. Each tree has a
distinct responsibility. No tree does another's job.

```
┌──────────────────────────────────────────────────────────────────┐
│  1. COMPONENT TREE  (ephemeral — rebuilt on every state change)  │
│                                                                  │
│  User-written. Component::build() returns an Element.            │
│  Zero pixels. Zero layout. Pure description.                     │
└─────────────────────────────┬────────────────────────────────────┘
                              │ reconcile
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│  2. ELEMENT TREE  (persistent — reconciled across frames)        │
│                                                                  │
│  Stable identity. Holds component state slots (ctx.state).       │
│  Fires on_mount / on_unmount. Assigns ComponentId by position    │
│  or by Key (D001, D055).                                         │
└─────────────────────────────┬────────────────────────────────────┘
                              │ layout + paint
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│  3. RENDER OBJECT TREE  (persistent — Phase 13+)                 │
│                                                                  │
│  Each node caches (Constraints → Size). layout() skips clean     │
│  subtrees. paint() records DrawCommands into PictureRecorder,    │
│  not pixels. RepaintBoundary nodes own an isolated Picture.      │
│  Currently: stateless DFS walk each frame (Phase 12 and below).  │
└─────────────────────────────┬────────────────────────────────────┘
                              │ composite
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│  4. LAYER / SCENE TREE  (Phase 14+)                              │
│                                                                  │
│  PictureLayer   — isolated display list                          │
│  TransformLayer — scroll / translate / scale (free after blit)   │
│  OpacityLayer   — fade entire subtrees                           │
│  ClipLayer      — pixel-accurate clipping                        │
│  Currently: single flat PictureRecorder (Phase 12 and below).    │
└─────────────────────────────┬────────────────────────────────────┘
                              │ rasterize
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│  COMPOSITOR                                                      │
│  Phase 1–13: CPU — TinySkiaCanvas, play_picture()               │
│  Phase 15:   GPU — wgpu textures, GPU blending                   │
└──────────────────────────────────────────────────────────────────┘
```

---

## Frame Lifecycle

One frame, start to finish:

```
1.  VSync signal arrives (winit RedrawRequested)
2.  Compute wall-clock dt = now - last_frame_time           (D054, D059)
3.  tezzera_animate::set_frame_dt(dt)                       (D059)
4.  Atom::take_frame_requested() clears pending flag
5.  Build pass: Component::build() for dirty components
6.  Reconcile: diff new elements vs Element tree
7.  Layout pass: walk RenderObject tree with LayoutCtx      (D056)
        LayoutCtx { constraints, font, theme }
        Child contexts via ctx.with_constraints()
8.  Paint pass — main tree:
        Widgets push DrawCommands into PictureRecorder A
        RectReader atoms fire with final screen Rects        (D057)
        Hit targets registered into flat Vec<HitTarget>
9.  Paint pass — overlay stack:
        For each OverlayEntry (bottom → top z-order):
            Draw scrim FillRect if entry.scrim.is_some()    (D058)
            Record entry.widget into PictureRecorder B
10. Composite:
        canvas.clear(background)
        canvas.play_picture(picture_A, font)                // main
        canvas.play_picture(picture_B, font)                // overlays
11. Present: softbuffer → screen (Phase 15: wgpu surface)
12. FocusManager::sync_with_overlay(overlay_stack, tree)    (D060)
13. last_frame_time = now
14. If any animation still running → request_frame()        (D059)
```

---

## Coordinate System

All coordinates are **window-pixel space** — origin at top-left of the window,
x right, y down. Units are logical pixels (not physical — DPI scaling is applied
by the platform before coordinates reach widget code).

`ctx.rect` in any `paint()` call is the widget's exact rect in window-pixel
space. This is the authoritative coordinate — not estimated, not computed from
parent.

`RectReader` (D057) surfaces this to user code:
```rust
let anchor: Atom<Option<Rect>> = ctx.state(None);
RectReader::new(anchor.clone(), Button::new("Open"))
// After first paint: anchor.get() == Some(Rect { origin: (x,y), size: (w,h) })
```

---

## Overlay System  (D058)

### What it is

A second render pass that always composites above the main tree. Handles:
popups, dropdowns, context menus, dialogs, bottom sheets, toasts, tooltips.

### What it is NOT

- Not a navigation system (that is the Route Stack — see below)
- Not a z-index system within the main widget tree (use Stack widget)
- Not a GPU compositor layer (that is Phase 14's Layer tree)

### Entry Structure

```rust
pub struct OverlayEntry {
    pub id:       LayerId,
    pub position: LayerPosition,
    pub widget:   BoxedWidget,     // interactive content only
    pub input:    InputBehavior,
    pub focus:    FocusBehavior,
    pub scrim:    Option<ScrimConfig>,
}

pub enum LayerPosition {
    Absolute(Point),    // anchored to screen coord — popups, tooltips, dropdowns
    Centered,           // window center — dialogs, alerts
    BottomAnchored,     // bottom edge — bottom sheets, toasts
    Fill,               // entire window — full-screen overlays
}

pub enum InputBehavior {
    PassThrough,   // misses fall through to entries below and main tree
    Block,         // misses are swallowed (or trigger scrim dismiss if configured)
}

pub enum FocusBehavior {
    PassThrough,   // Tab continues through entries below after this one
    Trap,          // Tab cycles within this entry only — never escapes
    Inert,         // no focusable nodes (toasts, decorative overlays)
}

pub struct ScrimConfig {
    pub color:  Color,
    pub on_tap: Option<Arc<dyn Fn() + Send + Sync>>,
    // None = silently absorb taps outside widget rect
    // Some(f) = call f when tap lands outside widget rect (tap-to-dismiss)
}
```

### Common Patterns

```
Dropdown / context menu:
  position: Absolute(anchor.bottom_left())
  input:    PassThrough       ← clicks outside fall to main tree
  focus:    PassThrough       ← Tab reaches items below when exhausted
  scrim:    None

Modal dialog:
  position: Centered
  input:    Block             ← nothing below receives input
  focus:    Trap              ← Tab cannot leave dialog
  scrim:    Some { color: rgba(0,0,0,120), on_tap: Some(dismiss) }

Non-dismissable dialog:
  scrim:    Some { color: rgba(0,0,0,180), on_tap: None }
                              ← tap absorbed silently, dialog cannot be dismissed

Bottom sheet:
  position: BottomAnchored
  input:    PassThrough       ← taps above sheet reach main tree
  focus:    PassThrough
  scrim:    Some { color: rgba(0,0,0,60), on_tap: Some(dismiss) }

Toast / snackbar:
  position: BottomAnchored
  input:    PassThrough
  focus:    Inert             ← keyboard users never reach it
  scrim:    None
```

### Input Routing

On every pointer event, scan overlay stack **top → bottom**:

```
Point P hits entry.widget rect?
  YES → deliver event to widget, STOP
  NO  + Block → fire scrim.on_tap if configured, else swallow, STOP
  NO  + PassThrough → continue to next entry below
No entry claimed it → deliver to main tree hit targets
```

### Stacked Dialogs

Each dialog is an independent entry. The stack handles them naturally:

```
entries (insertion order):
  [0] Dialog1  Centered, Block, scrim{on_tap: dismiss1}
  [1] Dialog2  Centered, Block, scrim{on_tap: dismiss2}   ← topmost

Click outside Dialog2:
  Dialog2 widget: miss + Block → fire dismiss2, STOP
  (Dialog1 never sees it)

Dialog2 closes → pop [1]:
  Click outside Dialog1:
  Dialog1 widget: miss + Block → fire dismiss1, STOP
```

---

## Focus System  (D060)

### Rules

1. Find the topmost overlay entry with `FocusBehavior::Trap`
2. If one exists — Tab cycles **only** within that entry's focusable nodes
3. If none — Tab cycles globally: main tree nodes + all overlay entries in z-order
4. `Inert` entries are skipped entirely in all Tab traversal

### Tab ordering

- Default: natural tree order (DFS)
- `tab_index: Some(n)` — explicit position; lower = earlier; ties → tree order
- `tab_index: Some(-1)` — focusable by pointer, skipped by Tab

### Sync

`FocusManager::sync_with_overlay(stack, tree)` is called after every paint pass.
It rebuilds the focus order from the current overlay stack + main tree.
If a previously focused node no longer exists (its entry was popped), focus clears.

---

## Navigation — Route Stack  (D061)

### What it is

A stack of screens. Only the top screen is alive. All others are frozen.
Completely separate from the overlay system.

```
Navigator stack (top = active):
  [0] HomeScreen    — FROZEN: state alive, zero CPU, not rendered
  [1] SettingsScreen — FROZEN
  [2] ProfileScreen  — ACTIVE: rendered, hit-testable, focusable
```

### Frozen vs Active

| Property          | Active | Frozen |
|-------------------|--------|--------|
| Rendered          | ✓      | ✗      |
| Hit-testable      | ✓      | ✗      |
| Focusable         | ✓      | ✗      |
| Atom state alive  | ✓      | ✓      |
| Hook slots alive  | ✓      | ✓      |
| Scroll position   | ✓      | ✓ (preserved) |

### Lifecycle

```
push(route):
  current top → Frozen (stops rendering, atoms preserved)
  new route   → Active (on_mount fires, rendering begins)

pop():
  current top → on_unmount fires, clear_component() releases atoms
  route below → Active (resumes rendering, state intact, scroll intact)
```

### Not overlays

A `Dialog` opened via the overlay system is **not** a route push. It lives above
the active route. A `push()` replaces the screen. A `Dialog` shares the screen.

Use overlay for: popups, dialogs, sheets, toasts, tooltips.
Use push/pop for: new screens, settings pages, detail views, auth flows.

---

## VSync + Animation  (D054, D059)

```
tezzera-state:   Atom::set() → request_frame() → wake event loop
tezzera-animate: FRAME_DT clock, set_frame_dt(dt) called by platform
tezzera-platform: tracks last_frame_time, computes real dt each frame
                  dt clamped to [0.001, 0.1]s

use_spring → reads frame_dt() → self-perpetuates via atom write while unsettled
use_animation → reads frame_dt() → self-perpetuates while AnimationState::Running
AnimCtrl::play() / pause() / reset() — full user API, no tick(dt) ever exposed
```

Animation is frame-rate independent. At 120Hz dt=8ms, at 60Hz dt=16ms — the
spring and animation controller reach the same value at the same wall-clock time.

---

## Hit Testing — Current vs Target

**Current (Phase 12):** Flat `Vec<HitTarget>` rebuilt every frame. Linear scan
on every click. No z-ordering within the main tree. Only `Button` registers hits.

**Target (Phase 13+):** Hit test walks the RenderNode tree depth-first,
deepest child first (mirrors visual stacking). First node whose cached rect
contains the point and has a hit handler wins. Overlay entries are tested before
main tree. O(depth) not O(n).

---

## Crate Responsibilities

```
tezzera-trace      zero-cost debug events
tezzera-state      Atom, hook state, frame scheduler (D054)
tezzera-core       Component, Element, Context, Key (D055)
tezzera-layout     layout algorithms: column/row/grid/wrap/flex
tezzera-render     SkiaCanvas, DrawCommand, PictureRecorder, FontCache
tezzera-theme      ThemeData, ColorScheme, ShadowLayer
tezzera-animate    AnimationController, use_spring, use_animation, frame clock (D059)
tezzera-a11y       A11yTree, A11yNode, FocusManager (D060)
tezzera-widgets    Widget trait, LayoutCtx, PaintCtx, all widget impls,
                   OverlayEntry, RectReader, overlay registry (D057, D058)
tezzera-nav        Navigator, Route stack (D061)
tezzera-platform   VSync event loop, window, input routing, frame lifecycle (D054)
tezzera            Umbrella: App, reconciler, render loop, prelude
```

---

## Key System  (D055)

### What it solves

Without keys, the reconciler matches children by **position**: child 0 maps to
old child 0, child 1 to child 1, etc. Reorder a 1000-item list and every item
is treated as new — all state is lost, all items remount.

With keys, sibling nodes that share a key are matched **by identity** regardless
of their new position. Swap item 7 and item 412: the reconciler knows they moved
and preserves their state.

Keys are **local to a sibling list**. They do not need to be globally unique.
Strings and integers are both valid. The reconciler FNV-hashes them to `u64`.

### API

```rust
// On any widget:
ListView::item(user.id).with_key(user.id)
Text::new(&user.name).with_key("name")   // local — unique within siblings
```

**Builder method on all widgets via blanket impl:**
```rust
pub trait WithKey: Sized {
    fn with_key(self, key: impl Into<Key>) -> KeyedWidget<Self>;
}
impl<W: Widget + 'static> WithKey for W { ... }
```

**`Key` struct (tezzera-core/src/element.rs):**
```rust
pub struct Key(pub u64);
impl From<&str>  for Key { fn from(s: &str) -> Key { Key(fnv_hash(s)) } }
impl From<u64>   for Key { fn from(n: u64) -> Key { Key(n) } }
impl From<i32>   for Key { fn from(n: i32) -> Key { Key(n as u64) } }
impl From<usize> for Key { fn from(n: usize) -> Key { Key(n as u64) } }
```

### Reconciler algorithm

```
Given old children [A, B, C, D, E] and new children [C, A, E, F]:
1. Partition new children into keyed and unkeyed lists
2. Build a HashMap<Key, RenderNode> from old keyed children
3. For each new child (in order):
   keyed:   look up in HashMap → if found, reuse (move), else mount new
   unkeyed: match by position within the unkeyed sublists → update or mount
4. Old nodes not claimed by step 3 → unmount (on_unmount, drop state)
```

### No GlobalKey

Flutter has `GlobalKey` — a key that reaches across the tree for direct widget
access. We deliberately omit this. It violates tree encapsulation and is
unnecessary for the use cases it's meant to solve (overlay positioning, form
validation). TEZZERA solves these with `RectReader` and `FocusNode` instead.

---

## Semantic Tree  (D064)

### Current state

`tezzera-a11y` has a complete data model (`A11yTree`, `A11yNode`, `Role`,
`FocusManager`) but it is entirely **disconnected from the widget tree**. The
semantic tree is never populated or sent to the platform.

### Target state

Semantics are built during the **paint pass** alongside `HitTarget` registration.
Every widget that implements `semantics()` contributes a node to the current
frame's `A11yTree`. The tree receives `ctx.rect` as the node's bounds, exactly
like hit targets.

### Mechanisms

**Automatic — interactive widgets self-annotate:**
Standard interactive widgets (Button, Checkbox, Slider, TextInput, Switch)
implement `Widget::semantics()` and return a `SemanticConfig` based on their
own properties. The user does nothing.

```rust
// In Button::semantics():
Some(SemanticConfig {
    role:  Role::Button,
    label: Some(self.label.clone()),
    ..Default::default()
})
```

**Builder methods — override or augment:**
```rust
Image::file("photo.png")
    .accessibility_label("A sunset over the mountains")

Container::new()
    .accessibility_role(Role::Navigation)

Text::new("3 unread messages")
    .accessibility_live()
```

**`Semantics` wrapper — explicit annotation for any widget:**
```rust
Semantics::new(Role::Article)
    .label("News item")
    .child(Column::new()...)
```

### `SemanticConfig` struct

```rust
pub struct SemanticConfig {
    pub role:     Role,
    pub label:    Option<String>,  // primary spoken text
    pub hint:     Option<String>,  // secondary instruction ("double tap to activate")
    pub value:    Option<String>,  // current value for sliders, inputs
    pub checked:  Option<bool>,    // for checkboxes, switches
    pub disabled: bool,
    pub live:     bool,            // screen reader announces changes immediately
    pub hidden:   bool,            // exclude entirely from a11y tree
}
```

### `Widget` trait extension

```rust
pub trait Widget: Send + Sync {
    fn layout(&self, ctx: &LayoutCtx) -> Size;
    fn paint(&self, ctx: &mut PaintCtx);
    fn flex_factor(&self) -> f32 { 0.0 }
    fn semantics(&self) -> Option<SemanticConfig> { None }  // NEW
}
```

### Paint pass integration

During `paint()`, alongside `ctx.register_hit_target(...)`:
```rust
if let Some(config) = self.semantics() {
    ctx.register_semantic_node(config, ctx.rect);
}
```

`PaintCtx::register_semantic_node()` appends to the frame's `A11yTree`.
After the full paint pass, the tree is complete with real window-pixel bounds.
`FocusManager::sync_with_overlay()` includes a11y nodes in the same pass.

### Platform binding timeline

Platform AT-SPI (Linux), UIA (Windows), and AXKit (macOS) bindings are
**Phase 21** work. Until then, the tree is built and available in memory but
not forwarded to the OS. This means:
- All the data model is correct when platform bindings arrive
- Tests can verify the tree shape without a real screen reader
- `cargo test` for a11y does not require platform-specific setup

---

## Co-location Overlay API  (D062)

See DECISIONS.md D062 for the full API. Summary:

`.sheet()`, `.dialog()`, `.dropdown()`, `.tooltip()` are **builder methods**
on trigger widgets. They take an `Atom<bool>` (open state) and a content
closure. The framework translates each to the correct `OverlayEntry` shape —
correct `InputBehavior`, `FocusBehavior`, and `ScrimConfig` pre-wired.

This is syntactic sugar. The engine is D058 `OverlayEntry` + registry.
`push_overlay()` remains available for programmatic/advanced use.

---

## FocusNode Graph  (D063)

See DECISIONS.md D063 for the full API. Summary:

`FocusNode` is a shared handle (`Arc`) attached to focusable widgets with
`.focus_node(node)`. Explicit neighbors are wired with `.focus_next()` /
`.focus_prev()`. Default (no neighbors) = natural tree order. FocusManager
builds traversal order from the graph at sync time. `FocusNode::focused`
is a reactive `Atom<bool>` — widgets read it to draw their focus ring.

Grid navigation, carousel arrow keys, gamepad D-pad are all expressible
without a platform-layer workaround.

---

## What Is NOT in the Render Engine

These are separate concerns handled by other systems:

- **Text shaping / BiDi** — `tezzera-text`, `tezzera-bidi`
- **Gesture recognition** — `tezzera-gesture` (swipe, pinch, long-press)
- **Network images** — `tezzera-net`
- **Style sheets** — `tezzera-style`
- **Hot reload** — `tezzera-hot-reload`
- **GPU compositor** — Phase 15, `tezzera-compositor` (not yet built)
- **Platform AT-SPI / UIA / AXKit** — Phase 21, screen reader integration

---

## Implementation Status

```
Phase 12 (current):
  ✅ VSync frame scheduler (D054)
  ✅ LayoutCtx — font + theme in layout pass (D056)
  ✅ Accurate text measurement — real glyph metrics
  ✅ Display list recording — PictureRecorder in PaintCtx
  ✅ Animation VSync clock (D059) — FRAME_DT atomic, set_frame_dt, use_spring uses frame_dt()
  ✅ use_animation hook — Progress, AnimCtrl, play/pause/reset, self-perpetuating frame loop
  ✅ Size cache — Column + Row cache measure() results by constraints
  ✅ Key mechanism (D055) — Key(u64), FNV hash, From impls, Element::with_key(), NativeElement.key
  ✅ RectReader (D057) — fires atom with window-pixel Rect during paint
  ✅ Overlay layer (D058) — second PictureRecorder pass, OverlayEntry, push_overlay, drain_overlays

Phase 12 additions (D062, D063):
  ✅ Co-location Overlay API — OverlayApi trait, WithOverlay<W>, .dropdown/.sheet/.dialog/.tooltip
  ✅ FocusNode graph — FocusNode(Arc), .request()/.release(), set_next/prev, FocusApi trait, WithFocus<W>

Phase 13 additions (D065–D068):
  ✅ Persistent RenderNode tree — cached last_constraints, cached_size, cached_picture, cached_rect, paint_dirty, hit_handlers
  ✅ Reconciler — match by tag+position; keyed sibling groups matched by Key first
  ✅ Dirty-flag layout + paint — constraints match → skip layout; !paint_dirty + rect match → replay Picture, skip widget.paint()
  ✅ Dirty-component tracking — atom.set() → mark_dirty(subscribers) → element_cache skips build() for non-dirty components
  ✅ subtree_dirty propagation — rebuilt components mark child native nodes for repaint
  ✅ PartialEq on Size/Point/Rect — enables rect-equality cache check

Phase 14:
  ⬜ RepaintBoundary — isolated PictureLayer per boundary
  ⬜ TransformLayer — zero-repaint scroll
  ⬜ Focus system wired end-to-end (D060)
  ⬜ Navigation route stack (D061)

Phase 15:
  ⬜ wgpu GPU compositor
  ⬜ Texture atlas, GPU blending
  ⬜ 120fps capable
```
