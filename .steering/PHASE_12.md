# Phase 12 — VSync, LayoutCtx, Coordinate System, Overlay Layer

> Status: COMPLETE
> Started: 2026-06-30
> Completed: 2026-07-01
> Target: Production render loop — zero idle CPU, real coordinates, overlay layer

## Why This Phase

Phases 1–11 built a complete feature surface. This phase makes the internals
world-class by addressing four structural gaps:

1. **`ControlFlow::Poll` burns 100% CPU at idle.** Every frame repaints
   everything unconditionally. Real frameworks (Flutter, Compose, SwiftUI) sleep
   until an input event or atom change arrives, then render exactly one frame.

2. **Layout is measured twice per frame.** `Column`, `Row` and `Stack` call
   `child.layout()` once to size children and again inside `paint()` to position
   them. Each `Text` widget computes its width heuristically. Accurate, cached
   layout requires a `LayoutCtx` with font access and a per-node size cache.

3. **Dynamic lists have no stable identity.** A reordered 1000-item list rebuilds
   every item because nothing tracks "same data, moved position". The `Key`
   mechanism (D055) gives items stable identity so the reconciler can match
   by key across reorders instead of by position.

4. **No authoritative coordinate system or overlay layer.** Widgets have no way
   to know where they landed in window-pixel coordinates, and there is no way
   to paint popup/dialog/tooltip content above the main widget tree. `RectReader`
   (D057) surfaces final rects to user code. The overlay layer (D058) provides
   a second paint pass that always composites on top.

---

## Steps

### Step 1 — Frame scheduler in `tezzera-state` ✅ (D054)
- Add `src/frame_scheduler.rs`:
  - `static FRAME_REQUESTED: AtomicBool` — set by Atom::set(), cleared by platform
  - `static WAKEUP_FN: OnceLock<Box<dyn Fn() + Send + Sync>>` — platform registers
  - `pub fn register_wakeup(f: impl Fn() + Send + Sync + 'static)` — called at startup
  - `pub fn request_frame()` — sets flag + calls wakeup fn
  - `pub fn take_frame_requested() -> bool` — AcqRel swap, clears flag
- `Atom::set()` and `Atom::update()` call `request_frame()` after write
- `batch::flush()` calls `request_frame()` once after all dirty atoms are flushed
- Export from `tezzera_state::frame_scheduler::{request_frame, take_frame_requested, register_wakeup}`

### Step 2 — VSync event loop in `tezzera-platform` ✅ (D054)
- Introduce `pub struct FrameRequest;` in `tezzera-platform/src/app.rs`
- Change `EventLoop::new()` → `EventLoop::<FrameRequest>::with_user_event().build()`
- Change `ControlFlow::Poll` → `ControlFlow::Wait`
- `ApplicationHandler<FrameRequest>` impl gains:
  - `fn user_event(...)` — calls `window.request_redraw()`
  - `fn about_to_wait(...)` — if `take_frame_requested()` → `window.request_redraw()`
- At startup after window creation: `register_wakeup(move || proxy.send_event(FrameRequest))`
- Result: app idles at 0% CPU; exactly one frame renders per atom change

### Step 3 — `LayoutCtx` + accurate text measurement (D056)
- Add `pub struct LayoutCtx<'a>` to `tezzera-widgets/src/tree/mod.rs`:
  ```rust
  pub struct LayoutCtx<'a> {
      pub constraints: Constraints,
      pub font: &'a FontCache,
      pub theme: &'a ThemeData,
  }
  ```
- Change `Widget::layout` signature: `fn layout(&self, constraints: Constraints) -> Size`
  → `fn layout(&self, ctx: &LayoutCtx) -> Size`
- `Text::layout()` — uses `font.measure_text(text, px)` instead of `len * 0.6 * px` heuristic
- `Column::layout()` / `Row::layout()` — cache child sizes to avoid re-measuring in `paint()`
- All 30+ widget `layout()` impls updated to accept `LayoutCtx`
- `WidgetApp::render()` and `App::launch()` updated to build and pass `LayoutCtx`

### Step 4 — Animation VSync integration (D059) ✅

**Goal**: Animation is driven by real wall-clock time, not a hardcoded timestep.
Ticking is fully automatic — the platform injects `dt` before every render pass,
exactly as it already paints widgets. User never calls `tick(dt)`.

**`tezzera-animate/src/clock.rs`** — new file:
```rust
static FRAME_DT: AtomicU32 = AtomicU32::new(/* 1/60 as bits */ 0x3C888889);

pub fn set_frame_dt(dt: f32) {
    let clamped = dt.clamp(0.001, 0.1);
    FRAME_DT.store(clamped.to_bits(), Ordering::Release);
}

pub fn frame_dt() -> f32 {
    f32::from_bits(FRAME_DT.load(Ordering::Acquire))
}
```
- Default `1/60` so animations work even if platform never calls `set_frame_dt`
- Export from `tezzera_animate` root

**`tezzera-animate/src/lib.rs`**:
- Add `pub mod clock;`
- `pub use clock::{set_frame_dt, frame_dt};`

**`tezzera-animate/src/spring_hook.rs`**:
- Replace `s.step(1.0 / 60.0)` → `s.step(crate::frame_dt())`

**`tezzera-animate/src/animation_hook.rs`** — new file:
```rust
pub struct Progress { value: Atom<f32> }
impl Progress { pub fn get(&self) -> f32 { self.value.get() } }

#[derive(Clone)]
pub struct AnimCtrl {
    state: Atom<ControllerState>,
    value: Atom<f32>,
}
impl AnimCtrl {
    pub fn play(&self)  { self.state.update(|s| s.start()) }
    pub fn pause(&self) { self.state.update(|s| s.pause()) }
    pub fn reset(&self) { self.state.update(|s| s.reset()) }
}

pub fn use_animation(ctx: &mut Context, duration: Duration) -> (Progress, AnimCtrl) {
    let state: Atom<AnimationController> = ctx.state(AnimationController::new(duration));
    let value: Atom<f32> = ctx.state(0.0_f32);
    // Tick each build() — advances by real dt, atom write self-perpetuates frame loop
    let s = state.get();
    if s.state() == AnimationState::Running {
        let mut next = s.clone();
        let progress = next.tick(crate::frame_dt());
        value.set(progress);
        state.set(next);
    }
    (Progress { value: value.clone() }, AnimCtrl { state, value })
}
```
- Export `use_animation`, `Progress`, `AnimCtrl` from `tezzera_animate`

**`tezzera-platform/Cargo.toml`**:
- Add `tezzera-animate = { path = "../tezzera-animate" }`

**`tezzera-platform/src/app.rs`**:
- Add `last_frame_time: Option<Instant>` to the app handler struct
- In `WindowEvent::RedrawRequested`:
  ```rust
  let now = Instant::now();
  let dt = self.last_frame_time
      .map(|t| t.elapsed().as_secs_f32())
      .unwrap_or(1.0 / 60.0)
      .clamp(0.001, 0.1);
  tezzera_animate::set_frame_dt(dt);
  self.last_frame_time = Some(now);
  // → existing render logic follows
  ```

**Tests**:
- `clock::set_frame_dt` round-trips through `frame_dt()`
- `use_spring` advances faster with dt=0.032 than dt=0.008 (frame-rate independence)
- `use_animation` reaches progress 1.0 after feeding cumulative dt = duration
- `AnimCtrl::pause()` stops progress advancing

### Step 5 — Size cache in widget tree ✅
- `WidgetBox` (the `Box<dyn Widget>` wrapper) gains:
  ```rust
  struct CachedLayout {
      constraints: Constraints,
      size: Size,
  }
  ```
- `WidgetBox::layout()` checks cache before calling inner; writes result after
- `WidgetBox::mark_dirty()` clears the cache
- Column/Row read child size from `WidgetBox::cached_size()` in `paint()` — zero re-measure

### Step 6 — Key mechanism (D055) ✅
- `pub struct Key(pub u64)` in `tezzera-core/src/element.rs`
- `Element::key: Option<Key>` field
- `Widget::into_element(self) -> Element` default impl
- `.with_key(key: impl Into<Key>)` builder method on all widgets via blanket impl
- String and integer `impl Into<Key>` (hash to u64)
- Reconciler stub: when same-type children are keyed, match by key not position

### Step 7 — RectReader widget (D057) ✅
- `pub struct RectReader` in `tezzera-widgets/src/tree/rect_reader.rs`
- Fields: `pub atom: Atom<Option<Rect>>`, `pub child: BoxedWidget`
- `layout()` — delegates to child, returns child size unchanged
- `paint()` — fires `self.atom.set(Some(ctx.rect))` then delegates to child
- `RectReader::new(atom, child)` constructor
- Export from `tezzera-widgets` prelude
- Composable over any widget — no widget modification required
- Test: RectReader fires atom with the correct rect on paint

### Step 8 — Overlay layer (D058) ✅
- `pub struct OverlayEntry { pub position: Point, pub widget: BoxedWidget }`
  in `tezzera-widgets/src/tree/overlay.rs`
- Thread-local `OVERLAY_ENTRIES: RefCell<Vec<OverlayEntry>>` — global registry
- `pub fn push_overlay(entry: OverlayEntry)` — called during main paint pass
- `pub fn drain_overlays() -> Vec<OverlayEntry>` — called by platform after main pass
- `App::launch()` render loop gains a second recorder pass:
  1. Clear `OVERLAY_ENTRIES` at frame start
  2. Main tree paints into `recorder_main` (existing)
  3. Drain overlays → paint each into `recorder_overlay` at declared position
  4. Canvas plays `picture_main` then `picture_overlay` — overlays always on top
- `OverlayEntry::new(position: Point, widget: impl Widget + 'static)` constructor
- Export `OverlayEntry`, `push_overlay` from `tezzera-widgets` prelude
- Test: overlay entry paints after main content (appears on top in draw order)

### Step 9 — Phase 12 showcase (updated) ✅
- Update `tezzera-examples/src/bin/phase12_demo.rs` with two additional panels:
  4. **Animation VSync Panel** — `use_spring` and `use_animation` side by side.
     A progress bar animated via `use_animation` at real wall-clock speed.
     A counter that springs to a new value when a button is pressed.
     Proves frame-rate independence — same animation speed regardless of render rate.
  5. **Coordinate + Overlay Panel** — a button tagged with `RectReader`, clicking it
     opens a dropdown overlay positioned exactly at the button's bottom-left using
     the captured rect. Proves the coordinate system and overlay layer end-to-end.

### Step 10 — Co-location Overlay API (D062) ✅

**Goal**: Overlay entries declared on the trigger widget, not in a global push
call. The framework wires correct input/focus/scrim defaults per API type.

**`tezzera-widgets/src/tree/overlay_api.rs`** — new file:
```rust
// Builder trait added to all widgets via blanket impl:
pub trait OverlayApi: Sized {
    fn dropdown(self, open: Atom<bool>, content: impl Fn() -> BoxedWidget + 'static) -> Self;
    fn sheet(self, open: Atom<bool>, content: impl Fn() -> BoxedWidget + 'static) -> Self;
    fn dialog(self, open: Atom<bool>, content: impl Fn() -> BoxedWidget + 'static) -> Self;
    fn tooltip(self, content: impl Fn() -> BoxedWidget + 'static) -> Self;
}
```

Each method stores the atom + closure. During `paint()`, if `open.get() == true`,
the widget calls `ctx.push_overlay(entry)` with the appropriate `OverlayEntry`
pre-configured per type (see D062 for exact InputBehavior/FocusBehavior/ScrimConfig).

`RectReader` wrapping is automatic — the overlay is positioned at the trigger
widget's `ctx.rect.bottom_left()` for anchored types (dropdown, tooltip).

**Tests**:
- `.dropdown(open, ...)` with `open=true` produces an `OverlayEntry` with
  `PassThrough` input, `PassThrough` focus, no scrim
- `.dialog(open, ...)` with `open=true` produces `Block` + `Trap` + scrim
- `.sheet(open, ...)` with `open=true` produces `PassThrough` + `PassThrough` + scrim

### Step 11 — FocusNode graph (D063) ✅

**Goal**: Replace flat `tab_index: Option<i32>` with node-to-node focus wiring.
Linear tab order remains the default (no API change needed for simple cases).

**`tezzera-a11y/src/focus_node.rs`** — new file:
```rust
pub struct FocusNode(Arc<FocusNodeInner>);
struct FocusNodeInner {
    pub next: Mutex<Option<FocusNode>>,
    pub prev: Mutex<Option<FocusNode>>,
    pub focused: Atom<bool>,
    pub id: u64,    // unique, assigned by FocusNode::new()
}

impl FocusNode {
    pub fn new() -> Self { ... }
    pub fn request(&self) { self.0.focused.set(true); }
}
```

**Widget builder methods (blanket impl)**:
```rust
.focus_node(node: FocusNode) -> Self
.focus_next(node: FocusNode) -> Self
.focus_prev(node: FocusNode) -> Self
```

**`FocusManager::sync_with_overlay()`** updated:
- Collects `FocusNode`s from current tree + overlay entries in z-order
- For traversal: if node has `.next`, follow the explicit edge;
  otherwise fall back to the next node in tree-order
- Explicit `.next`/`.prev` form a doubly-linked traversal override

**Tests**:
- Default (no `.focus_next`) → tree order traversal
- Explicit `a.focus_next(b)` → Tab goes a → b even if b precedes a in tree
- `FocusNode::request()` sets `focused` atom → widget receives focus

---

## Exit Criteria

```
□ App idles at 0% CPU (verified: frame counter stops when mouse is still)
□ Atom::set() triggers exactly one redraw per change (no double-renders)
□ Text::layout() uses real glyph metrics (no heuristic multipliers)
□ Column/Row do not call child.layout() twice per frame
□ use_spring uses real wall-clock dt — no hardcoded 1/60
□ use_animation reaches progress 1.0 after wall-clock duration elapses
□ AnimCtrl::pause() stops animation; play() resumes from same position
□ Animation speed is frame-rate independent (same result at 60Hz and 120Hz)
□ RectReader fires atom with correct window-pixel Rect after paint
□ OverlayEntry content always composites above main tree content
□ Popup positioned via RectReader atom appears at correct screen coordinates
□ .dropdown() / .sheet() / .dialog() produce correct OverlayEntry shape
□ Overlay positioned at trigger widget's window-pixel rect via RectReader
□ FocusNode explicit neighbor → Tab follows declared edge not tree order
□ FocusNode::request() focuses a node programmatically
□ All workspace tests pass with zero warnings
□ cargo check --release --workspace: zero warnings
```

---

## Approved dependencies
- No new crates — `std::sync::atomic`, `std::sync::OnceLock`, `std::cell::RefCell` sufficient
- winit's `EventLoopProxy<T>` (already a dep) is the wake mechanism

## DO NOT
- DO NOT implement the GPU compositor (Phase 8 of render pipeline plan)
- DO NOT implement parallel layout (Rayon) — that is Phase 13
- DO NOT change the `Widget` trait name to `RenderObject` yet — rename when reconciler lands
- DO NOT implement z-index sorting within the overlay layer — insertion order is z-order
