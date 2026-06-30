# TEZZERA — PHASE 1
> Foundation: Working desktop app in pure Rust
> Timeline: 3–6 months
> Target: 60fps desktop app, state works, layout works, tracing works

---

## PHASE 1 GOAL

A developer can write a working desktop app in pure Rust.
No macros required yet. Pure trait implementations.
Everything observable via terminal tracing.

---

## EXIT CRITERIA
> Phase 1 is NOT done until ALL of these pass.

```
□ Counter app renders on desktop at 60fps
□ State updates trigger correct re-renders
□ Layout constraints work for Column/Row/Stack
□ Text renders correctly with basic fonts
□ on_mount / on_update / on_unmount fire correctly
□ ErrorBoundary catches panics and shows fallback
□ TezzeraTrace events appear in terminal
□ Time travel ring buffer captures last 1000 events
□ tzr dev command starts the app
□ tzr build --target desktop produces a binary
□ All tezzera-core tests pass
□ All tezzera-state tests pass
□ All tezzera-layout tests pass
□ All tezzera-render tests pass
□ All tezzera-trace tests pass
□ No warnings in release build
□ No unsafe code without SAFETY comments
□ Every public API has doc comments
□ DECISIONS.md has no OPEN items for Phase 1 scope
```

---

## STEP-BY-STEP PLAN

### STEP 1 — Repository Setup
**Do this first. Verify before moving on.**

```bash
mkdir tezzera
cd tezzera
git init
```

Create workspace Cargo.toml:
```toml
[workspace]
members = [
    "tezzera-macros",
    "tezzera-trace",
    "tezzera-core",
    "tezzera-state",
    "tezzera-layout",
    "tezzera-render",
    "tezzera-widgets",
    "tezzera-cli",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["TEZZERA Contributors"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/tezzera-ui/tezzera"
```

**Verify:**
```bash
cargo check  # must pass
```

**Commit:**
```bash
git add .
git commit -m "chore: initialize TEZZERA workspace"
```

---

### STEP 2 — tezzera-trace (build first)
**Everything depends on this. Must be right.**

What to build:
- TezzeraTrace enum (all variants as per DECISIONS.md D039)
- TracingBus struct
- TraceSubscriber trait
- RingBufferSubscriber (1000 events default)
- ConsoleSubscriber (with filter support)
- FileSubscriber
- trace!() macro (zero cost in release)
- TRACING_BUS global

**Verify:**
```rust
// This test must pass before moving on
#[test]
fn trace_emits_in_debug() {
    let sub = TestSubscriber::new();
    TRACING_BUS.add_subscriber(sub.clone());
    trace!(TezzeraTrace::ComponentMount {
        id: ComponentId(1),
        name: "TestComponent",
        location: location!(),
    });
    assert_eq!(sub.event_count(), 1);
}

#[test]
fn trace_zero_cost_in_release() {
    // Verify with: cargo build --release
    // Binary should contain no trace strings
}
```

**Commit:**
```bash
git commit -m "feat(trace): implement TezzeraTrace bus and subscribers"
```

---

### STEP 3 — tezzera-core: Types and Traits
**The foundation. Get this right.**

What to build (in order):

**3a — Basic types**
```rust
// Build these first
pub struct ComponentId(u64);
pub struct AtomId(u64);
pub struct Location { file: &'static str, line: u32 }
pub struct Size { pub width: f32, pub height: f32 }
pub struct Point { pub x: f32, pub y: f32 }
pub struct Rect { pub origin: Point, pub size: Size }
```

**3b — Element tree**
```rust
pub enum Element {
    Component(ComponentElement),
    Native(NativeElement),
    Text(TextElement),
    Empty,
}

pub struct ComponentElement {
    pub id: ComponentId,
    pub key: Option<Key>,
    pub children: Vec<Element>,
}
```

**3c — TezzeraComponent trait**
```rust
pub trait TezzeraComponent: 'static {
    fn build(&self, ctx: &mut Context) -> Element;

    // Lifecycle — default implementations (do nothing)
    fn on_mount(&self) {}
    fn on_unmount(&self) {}
    fn on_update(&self, _prev: &Self) where Self: Sized {}
}
```

**3d — RenderObject trait**
```rust
pub trait RenderObject: 'static {
    fn layout(&mut self, constraints: Constraints) -> Size;
    fn paint(&self, canvas: &mut Canvas, size: Size);
    fn hit_test(&self, point: Point, size: Size) -> bool {
        // default: entire bounding box
        point.x >= 0.0 && point.x <= size.width &&
        point.y >= 0.0 && point.y <= size.height
    }
}
```

**3e — Context**
```rust
pub struct Context {
    component_id: ComponentId,
    // state, lifecycle hooks go here
}
```

**3f — ChildContainer trait**
```rust
pub trait ChildContainer: Sized {
    fn child(self, element: impl Into<Element>) -> Self;
    fn children(self, elements: Vec<impl Into<Element>>) -> Self;
    fn child_if(self, element: Option<impl Into<Element>>) -> Self;
    fn prepend(self, element: impl Into<Element>) -> Self;
}
```

**3g — ErrorBoundary**
```rust
pub struct ErrorBoundary {
    fallback: Box<dyn Fn(&TezzeraError) -> Element>,
    child: Element,
}
```

**Verify each step before moving to next.**

Tests required:
```rust
#[test]
fn component_builds_element() { }

#[test]
fn lifecycle_fires_in_order() { }

#[test]
fn error_boundary_catches_panic() { }

#[test]
fn child_container_order_preserved() { }
```

**Commit after each sub-step (3a, 3b, 3c, 3d, 3e, 3f, 3g)**

---

### STEP 4 — tezzera-state: Atom System
**The reactive heart. Must be correct before layout.**

What to build (in order):

**4a — Atom<T> basic**
```rust
pub struct Atom<T: 'static> {
    id: AtomId,
    value: T,
    subscribers: Vec<ComponentId>,
}

impl<T: 'static> Atom<T> {
    pub fn get(&self) -> &T { }
    pub fn set(&mut self, value: T) { }
    pub fn update(&mut self, f: impl FnOnce(&T) -> T) { }
}
```

**4b — Three scopes**
- Local: use_atom() in Context
- AtomProvider widget
- GlobalAtom<T>

**4c — Subscription system**
- Auto-subscribe on .get()
- Notify on .set() / .update()

**4d — RefreshEngine**
- Dirty set collection
- DFS timestamp tree index
- Root finding (prune descendants)
- Single rebuild pass

**4e — Batching**
- Automatic within sync blocks
- batch() manual API
- Priority::Immediate / Normal / Background

**4f — Derived atoms**
- Lazy computation
- Auto-invalidation

Tests required:
```rust
#[test]
fn atom_notifies_subscriber_on_set() { }

#[test]
fn multiple_atom_changes_cause_one_rebuild() { }

#[test]
fn refresh_engine_prunes_descendants() { }

#[test]
fn batch_causes_single_rebuild() { }

#[test]
fn derived_atom_recomputes_lazily() { }

#[test]
fn global_atom_accessible_anywhere() { }

#[test]
fn scoped_atom_isolated_per_provider() { }
```

**Benchmark required:**
```rust
#[bench]
fn bench_1000_atom_updates() { }
// Must stay under 1ms for 1000 updates
```

---

### STEP 5 — tezzera-layout: Flexure Engine
**Most complex part of Phase 1. Take your time.**

What to build (in order):

**5a — Constraint types**
```rust
pub struct Constraints {
    pub min_width: f32,
    pub max_width: AxisBound,
    pub min_height: f32,
    pub max_height: AxisBound,
}

pub enum AxisBound {
    Bounded(f32),
    Unbounded,
    Shrink,
}
```

**5b — Measure pass**
- Top-down constraint propagation
- Children report their size
- Parent places children

**5c — Basic layout widgets**
Build in this order — each one tests the engine:
1. SizedBox (simplest — fixed size)
2. Spacer (flexible space)
3. Column (vertical stack)
4. Row (horizontal stack)
5. Stack (z-axis)
6. Expanded (fill available)
7. Flex (flex layout)
8. Grid
9. Wrap
10. AspectRatio
11. FractionallySizedBox

**5d — Sizing modifiers**
```rust
pub enum Width {
    Fixed(f32),
    Fill,
    Shrink,
    Fraction(f32),
    Min(f32),
    Max(f32),
    Range(f32, f32),
}
// Same for Height
```

**5e — Intrinsic sizing**
- IntrinsicHeight, IntrinsicWidth, IntrinsicSize
- Two-pass layout only when used
- Dev warning inside scroll (Phase 2)

**5f — Baseline alignment**
- Row.align(Alignment::Baseline)
- Per-child .align_self()

**5g — RTL support**
- Logical sides (start/end)
- Physical sides (left/right)
- Directionality widget
- Auto-mirror on RTL locale

**5h — Text layout**
- cosmic-text integration
- HarfBuzz shaping
- fontdue rasterization
- Glyph cache
- Text, RichText widgets (basic)

Tests required — every layout widget:
```rust
#[test]
fn column_stacks_children_vertically() { }

#[test]
fn row_stacks_children_horizontally() { }

#[test]
fn expanded_fills_available_space() { }

#[test]
fn fraction_width_is_of_available_space() { }

#[test]
fn constraints_propagate_correctly() { }

#[test]
fn impossible_layout_detected() { }
// Expanded in Unbounded column = error

#[test]
fn rtl_mirrors_row_direction() { }

#[test]
fn baseline_alignment_aligns_text() { }
```

---

### STEP 6 — tezzera-render: Skia Pipeline
**First pixels on screen.**

What to build:

**6a — Skia setup**
- skia-safe crate integration
- Surface creation (desktop via winit)
- Basic clear + present loop

**6b — RenderPipeline**
- Walk element tree
- Call RenderObject.paint() in order
- Canvas wrapper

**6c — Dirty region tracking**
- Track which rects need repaint
- Only repaint dirty areas
- Full repaint fallback

**6d — Layer compositing**
- Static layer cache
- GPU texture for static content
- Invalidate on change

**6e — Frame loop**
- 60fps target (Phase 1)
- 120fps (Phase 2+)
- winit event loop integration
- Frame timing

**6f — Image handling**
- Background thread decode
- Memory cache (LRU 50MB)
- Basic formats: PNG, JPEG

Tests required:
```rust
#[test]
fn render_produces_correct_pixels() {
    // Snapshot test — compare to golden
}

#[test]
fn dirty_regions_only_repaint_changed_area() { }

#[test]
fn frame_time_under_budget() {
    // 60fps = 16.67ms budget
}
```

---

### STEP 7 — Integration: Hello World
**All systems connected. First real app.**

Build the counter app:
```rust
use tezzera::prelude::*;

struct Counter;

impl TezzeraComponent for Counter {
    fn build(&self, ctx: &mut Context) -> Element {
        let count = ctx.state(0i32);

        Column::new()
            .child(Text::new(format!("Count: {}", count.get())))
            .child(
                Button::new("Increment")
                    .on_press(|| count.update(|n| n + 1))
            )
            .into_element()
    }
}

fn main() {
    TezzeraApp::new()
        .child(Counter)
        .run();
}
```

**Verify:**
```
□ App opens a window
□ Text renders correctly
□ Button is clickable
□ Count increments on click
□ Re-render is correct
□ No flicker
□ Terminal shows TezzeraTrace events
□ 60fps maintained
```

**Commit:**
```bash
git commit -m "feat: Phase 1 complete — counter app works"
```

---

### STEP 8 — tezzera-cli: tzr dev + tzr build
**Make the development experience work.**

**8a — tzr dev**
```
- Start winit event loop
- Watch for file changes
- Recompile on change
- Terminal trace output
- --trace=... filter flags
```

**8b — tzr build --target desktop**
```
- Release build
- Strip debug info
- Verify binary size reasonable
- Verify no tracing in release binary
```

---

### STEP 9 — Phase 1 Verification
**Run ALL exit criteria. Sign off every item.**

```
□ Counter app renders on desktop at 60fps
□ State updates trigger correct re-renders
□ Layout constraints work for Column/Row/Stack
□ Text renders correctly with basic fonts
□ on_mount / on_update / on_unmount fire correctly
□ ErrorBoundary catches panics and shows fallback
□ TezzeraTrace events appear in terminal
□ Time travel ring buffer captures last 1000 events
□ tzr dev command starts the app
□ tzr build --target desktop produces a binary
□ All tezzera-core tests pass
□ All tezzera-state tests pass
□ All tezzera-layout tests pass
□ All tezzera-render tests pass
□ All tezzera-trace tests pass
□ No warnings in release build
□ No unsafe code without SAFETY comments
□ Every public API has doc comments
□ DECISIONS.md has no OPEN items for Phase 1 scope
```

**Only when ALL boxes are checked → begin Phase 2.**

---

## PHASE 1 DO NOT LIST

```
✗ Do not implement scroll — Phase 2
✗ Do not implement navigation — Phase 2
✗ Do not implement animation — Phase 2
✗ Do not implement hot reload — Phase 2
✗ Do not implement WASM target — Phase 2
✗ Do not implement platform APIs — Phase 3
✗ Do not implement FFI — Phase 3
✗ Do not implement iOS/Android — Phase 3
✗ Do not add features not in exit criteria
✗ Do not skip writing tests
✗ Do not skip writing doc comments
✗ Do not merge code with warnings
✗ Do not add dependencies without discussion
```

---

## APPROVED DEPENDENCIES — PHASE 1

```
skia-safe       → Skia rendering
winit           → window + event loop
cosmic-text     → text layout
harfbuzz-rs     → text shaping (via cosmic-text)
fontdue         → font rasterization
serde           → serialization (for trace protocol)
serde_json      → JSON (dev tools later)
rmp-serde       → MessagePack (trace protocol)
tokio           → async runtime (state only)
rayon           → parallel layout
log             → logging facade
env_logger      → logger implementation (dev only)
thiserror       → error types
```

**Any new dependency needs approval before adding.**
**Add to this list when approved.**
