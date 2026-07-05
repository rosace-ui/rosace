# TEZZERA — DECISIONS
> Every architecture decision lives here.
> Locked decisions do not get re-debated.
> New decisions get added here before code is written.

---

## FORMAT

Each decision has:
- **ID**: D{number}
- **Status**: LOCKED | DEFERRED | OPEN
- **Question**: what was being decided
- **Decision**: what was decided
- **Reason**: why
- **Affects**: which crates / systems

---

## COMPONENT MODEL

### D001 — Component Identity
**Status**: LOCKED
**Question**: How does TEZZERA track if a component is the same instance across rebuilds?
**Decision**: Position in tree by default. Optional `.key(value)` when order can change.
**Reason**: Position default keeps simple cases simple. Keys available for dynamic lists. Compiler warns when dynamic lists have no keys.
**Affects**: tezzera-core

---

### D002 — Component Lifecycle
**Status**: LOCKED
**Question**: Do components have lifecycle hooks?
**Decision**: Yes. Three hooks: on_mount, on_update, on_unmount. All tree-driven.
- on_mount → fires once when added to tree, return fn for cleanup
- on_update → fires when own props change, receives previous props
- on_unmount → fires once when removed from tree
**Reason**: Real apps always need mount/unmount for connections, timers, resources. Reactivity to atoms is separate and automatic.
**Affects**: tezzera-core
**Rules**:
- Hooks only at component top level
- Compiler error if inside condition or loop
- Cleanup return from on_mount auto-runs on unmount

---

### D003 — Component Composition
**Status**: LOCKED
**Question**: How do you put custom content inside a component?
**Decision**: Three tiers:
1. Builder (.child, .children, .builder, .child_if, .prepend, .append)
2. Named slots (.header, .body, .footer)
3. Macro sugar (compiles to Tier 1 + 2)
**Reason**: Different tiers serve different needs. All compile to same thing.
**Affects**: tezzera-core, tezzera-macros
**Multi-child API**: ChildContainer trait on all multi-child widgets
**Order guarantee**: Children render in exact addition order

---

### D004 — Error Boundaries
**Status**: LOCKED
**Question**: What happens when a component panics?
**Decision**: Two-layer system:
- Layer 1: TezzeraResult for expected failures, propagate with ?
- Layer 2: ErrorBoundary for unexpected panics, shows fallback
**Reason**: Expected and unexpected failures need different handling.
**Affects**: tezzera-core
**Rules**:
- Errors bubble up to nearest ErrorBoundary
- App-level fallback as final safety net
- Dev mode: full overlay with stack trace
- Production: clean fallback, silent logging
- ErrorBoundary cannot catch its own errors
- Async errors must use TezzeraResult

---

### D005 — Lazy Components
**Status**: LOCKED
**Question**: Should components load code only when needed?
**Decision**: Route components lazy by default. Non-route eager by default.
- #[lazy] to opt-in non-route components
- #[eager] to opt-out route components
- Loading state required for all lazy components
**Reason**: Large apps need code splitting. Routes are natural split points.
**Affects**: tezzera-core, tezzera-macros, tezzera-cli
**Dev mode**: All eager, loading instant

---

## STATE SYSTEM

### D006 — State Primitive
**Status**: LOCKED
**Question**: What is the core state primitive?
**Decision**: Atom<T> — a reactive value. When changed, subscribers rebuild.
**Reason**: Simplest possible primitive. Everything else builds on top.
**Affects**: tezzera-state
**API**:
- use_atom(default) — local
- atom!(default) — provided or global
- .get() — read, auto-subscribe
- .set(value) — write, notify
- .update(|v| ...) — atomic read-modify-write

---

### D007 — Atom Scope
**Status**: LOCKED
**Question**: Where does an atom live?
**Decision**: Three scopes:
- Local: use_atom() — component lifetime only
- Provided: atom!() + AtomProvider — subtree scoped
- Global: GlobalAtom — app lifetime, anywhere
**Reason**: Different concerns need different scopes. No prop drilling.
**Affects**: tezzera-state
**Rules**:
- Local atoms cannot escape their component
- Scoped atoms outside provider = compile error
- Global atoms overused = lint warning
- Provider can be nested, inner wins

---

### D008 — Atom Persistence
**Status**: LOCKED
**Question**: Does atom state survive hot reload, backgrounding, restart?
**Decision**: Opt-in per atom. Three levels:
- #[persist(reload)] — survives hot reload
- #[persist(session)] — survives backgrounding
- #[persist(permanent)] — survives restart
- #[no_persist] — explicitly blocked
- #[persist(permanent, encrypted)] — secure storage
**Reason**: Not all state should persist. Developer decides.
**Affects**: tezzera-state, tezzera-platform
**Rules**:
- Only provided and global atoms can persist
- Type must impl Serialize + Deserialize
- Type change → graceful reset, never crash
- Migration support for permanent atoms

---

### D009 — Async Atoms
**Status**: LOCKED
**Question**: How does TEZZERA handle async operations?
**Decision**: use_async family with five states: Idle, Loading, Success, Error, Refreshing
- use_async → auto fetch on mount
- use_async_lazy → manual trigger
- use_async_when → conditional
- use_async_all! → parallel
**Reason**: Async is everywhere. Must be first class.
**Affects**: tezzera-state
**Guarantees**:
- Race conditions impossible — latest wins
- Cancellation automatic on unmount
- No memory leaks

---

### D010 — Atom Batching
**Status**: LOCKED
**Question**: When multiple atoms change, how many rebuilds happen?
**Decision**: Automatic batching within sync blocks. Manual batch() for explicit control.
Priority levels: Immediate, Normal (default), Background.
**Reason**: Multiple atoms changing = one logical operation = one rebuild.
**Affects**: tezzera-state

---

### D011 — Smart Refresh Engine
**Status**: LOCKED
**Question**: How does TEZZERA minimize rebuilds?
**Decision**: Find dirty roots, prune descendants, rebuild minimum set.
Algorithm:
1. Collect dirty components from atom changes
2. Prune descendants (parent dirty = skip children)
3. Rebuild from roots only
4. Single layout pass
5. Single paint pass
**Reason**: Parent rebuild covers children. No double work.
**Affects**: tezzera-state, tezzera-core
**Tree index**: DFS timestamps, O(1) ancestor lookup

---

### D012 — External State
**Status**: LOCKED
**Question**: How does TEZZERA connect to external sources?
**Decision**: Stream<T> as universal bridge. Typed adapters on top.
Built-in: use_websocket, use_query, use_file_watch, use_sensor, use_network_status, use_app_lifecycle
**Reason**: Everything external is a stream of values.
**Affects**: tezzera-state, tezzera-platform
**Rule**: All connections auto-cleaned on unmount

---

## LAYOUT ENGINE

### D013 — Layout Engine Name
**Status**: LOCKED
**Decision**: Flexure
**Affects**: tezzera-layout

---

### D014 — Constraint Model
**Status**: LOCKED
**Decision**: Constraints with AxisBound: Bounded(f32) | Unbounded | Shrink
Three-pass layout: Measure (top-down), Place (bottom-up), Paint
**Affects**: tezzera-layout

---

### D015 — Fractional Sizing
**Status**: LOCKED
**Decision**: Modifier primary (.width(Width::fraction(0.5))). FractionallySizedBox for complex cases.
Fraction is of AVAILABLE space, not screen size. Respects parent constraints.
**Affects**: tezzera-layout

---

### D016 — Intrinsic Sizing
**Status**: LOCKED
**Decision**: Explicit opt-in only. IntrinsicHeight, IntrinsicWidth, IntrinsicSize widgets.
Zero cost when not used. Built into Dialog, Tooltip, BottomSheet.
Dev warning when used inside ScrollView.
**Affects**: tezzera-layout

---

### D017 — Baseline Alignment
**Status**: LOCKED
**Decision**: Opt-in per Row. Row.align(Alignment::Baseline) or per-child .align_self(Alignment::Baseline).
Default is top alignment.
**Affects**: tezzera-layout

---

### D018 — Overlay System
**Status**: LOCKED
**Decision**: Six layers: 0=Content, 1=Navigation, 2=Modal barrier, 3=Modals, 4=Overlays, 5=DevTools
Overlay::show(), Modal::show() APIs. Auto-reposition if off screen.
**Affects**: tezzera-layout, tezzera-render

---

### D019 — Text Layout
**Status**: LOCKED
**Decision**: cosmic-text foundation, HarfBuzz shaping, fontdue rasterization, Skia rendering.
BiDi automatic. Font fallback chain. Glyph cache (GPU atlas). Layout cache.
Desktop: subpixel. Mobile: grayscale.
**Affects**: tezzera-layout, tezzera-render

---

### D020 — RTL Support
**Status**: LOCKED
**Decision**: Day 1. Automatic mirroring on RTL locale.
Logical sides (.padding_start/end) auto-mirror. Physical (.padding_left/right) never mirror.
Icons: .mirror_in_rtl(bool). Force LTR: Directionality::ltr().
**Affects**: tezzera-layout

---

## SCROLL

### D021 — Bidirectional Scroll
**Status**: DEFERRED (Phase 3)
**Decision**: Phase 1+2 = 1D only. API reserved: ScrollView2D::new()
**Affects**: tezzera-scroll

---

### D022 — Sticky Headers
**Status**: LOCKED
**Decision**: Built into VirtualList, day 1. .sticky_headers(true) default.
**Affects**: tezzera-scroll

---

### D023 — Pull to Refresh
**Status**: LOCKED
**Decision**: Built into ScrollView. .pull_to_refresh(|| async { }).
Platform feel per target. Desktop: not shown.
**Affects**: tezzera-scroll

---

### D024 — Infinite Scroll
**Status**: LOCKED
**Decision**: .on_end_reached() + .end_threshold(n) on VirtualList.
PaginatedState pattern built-in.
**Affects**: tezzera-scroll, tezzera-state

---

### D025 — Scroll Restoration
**Status**: LOCKED
**Decision**: Automatic per route. .restore_position(false) to opt out.
App restart = position reset. Session only.
**Affects**: tezzera-scroll, tezzera-nav

---

## NAVIGATION

### D026 — Route Definition
**Status**: LOCKED
**Decision**: #[routes] enum with #[route("/path")] attributes. Type-safe. Auto deep link.
**Affects**: tezzera-nav, tezzera-macros

---

### D027 — Nested Navigation
**Status**: LOCKED
**Decision**: Full nested navigation, unlimited depth. Each navigator independent history.
Tab switch: each tab remembers its stack.
**Affects**: tezzera-nav

---

### D028 — Navigation Guards
**Status**: LOCKED
**Decision**: Async guards via use_before_leave(). Global guards via Navigator::guard().
NavigationDecision: Allow | Block | RedirectTo(route)
**Affects**: tezzera-nav

---

### D029 — Back Button
**Status**: LOCKED
**Decision**: use_back_handler() per screen. Default: pop if history, else exit.
BackHandlerResult: Pop | Block | Custom(fn)
**Affects**: tezzera-nav, tezzera-platform

---

### D030 — Keep Alive
**Status**: LOCKED
**Decision**: Opt-in per tab. keep_alive: true. Memory budget with LRU eviction.
KeepAlive widget for non-tab use.
**Affects**: tezzera-nav, tezzera-core

---

### D031 — Web URL Sync
**Status**: LOCKED
**Decision**: Automatic. Browser back/forward = Navigator pop/push. Query params supported.
Hash routing option: tzr build --web-routing=hash
**Affects**: tezzera-nav, tezzera-platform

---

## RENDERING

### D032 — Renderer
**Status**: LOCKED
**Decision**: **D032**: Renderer backend — **tiny-skia** (pure Rust, CPU) for MVP. Swap to skia-safe (C++ Skia, GPU) at v1.0. Isolated in `tezzera-render/src/canvas.rs` (~100 lines to swap). Rationale: skia-safe requires C++ toolchain (30-60 min build), breaks wasm32 target, needs Emscripten. tiny-skia builds in seconds and is WASM-compatible.
**Affects**: tezzera-render

---

### D033 — Image Handling
**Status**: LOCKED
**Decision**: Always decode on background thread. Three-level cache: memory → disk → network.
Formats: PNG, JPEG, WebP, AVIF, GIF, SVG, APNG.
Memory: LRU 50MB. Disk: LRU 200MB.
**Affects**: tezzera-render

---

### D034 — Custom Painters
**Status**: LOCKED
**Decision**: CustomPaint widget with full SkiaCanvas access. Hit tester required. repaint_when for efficiency.
**Affects**: tezzera-render

---

### D035 — Accessibility
**Status**: LOCKED
**Decision**: Semantic tree built always. Platform bridges:
iOS=UIAccessibility, Android=AccessibilityNodeInfo, Web=ARIA, Desktop=OS APIs
FocusScope for focus management and trapping.
**Affects**: tezzera-render, tezzera-platform

---

### D036 — HDR / Wide Color
**Status**: DEFERRED (Phase 3)
**Decision**: sRGB for Phase 1+2. API reserved: .color_space(ColorSpace::DisplayP3)
**Affects**: tezzera-render

---

## UI CUSTOMIZATION

### D037 — Customization Levels
**Status**: LOCKED
**Decision**: Five levels:
1. Theme tokens (#[derive(TezzeraTheme)])
2. Component styling (modifier chain)
3. Component override (WidgetOverride trait)
4. Custom RenderObject (RenderObject trait)
5. Custom render pipeline (TezzeraRenderer trait)
**Affects**: tezzera-render, tezzera-theme, tezzera-core

---

### D038 — Theme System
**Status**: LOCKED
**Decision**: #[derive(TezzeraTheme)] — exhaustive, typed. Partial theme = compile error.
All tokens required. Switching theme triggers full re-render.
**Affects**: tezzera-theme

---

## OBSERVABILITY

### D039 — Tracing System
**Status**: LOCKED
**Decision**: TezzeraTrace enum, TracingBus, zero cost in production.
All systems emit traces. No system merges without traces.
Subscribers: RingBuffer, DevTools, File, Console, IDE.
Protocol: serde, versioned, language-agnostic.
**Affects**: tezzera-trace, ALL crates

---

### D040 — Dev Tools Transport
**Status**: LOCKED
**Decision**: Native = shared memory + Unix socket. WASM = WebSocket.
Same protocol both ways. Dev tools = separate process.
MessagePack serialization.
**Affects**: tezzera-trace, tezzera-devtools

---

### D041 — Hot Reload Limits
**Status**: LOCKED
**Decision**:
Can reload: build() logic, styles, handlers, atom defaults, strings
Needs restart: new deps, atom type change, new files, FFI changes, macro changes
On limit: auto full rebuild, clear message, no silent failure
**Affects**: tezzera-devtools, tezzera-cli

---

## PLATFORM

### D042 — App Lifecycle
**Status**: LOCKED
**Decision**: GlobalAtom<LifecycleState> + use_app_lifecycle() hook.
States: Active, Inactive, Background, Suspended.
**Affects**: tezzera-platform

---

### D043 — Permissions
**Status**: LOCKED
**Decision**: Unified async API. Permission::camera().rationale("...").request().await
PermissionStatus: Granted | Denied | PermanentlyDenied
**Affects**: tezzera-platform

---

### D044 — Localization
**Status**: LOCKED
**Decision**: Day 1. TOML format. use_locale() hook. LOCALE.set() triggers full re-render + RTL.
**Affects**: tezzera-theme, tezzera-layout

---

### D045 — Haptics
**Status**: LOCKED
**Decision**: Semantic API. Haptic::light/medium/heavy/success/warning/error/selection()
Desktop/WASM = silent no-op.
**Affects**: tezzera-platform

---

### D046 — Safe Areas
**Status**: LOCKED
**Decision**: Edge to edge by default. Scaffold handles automatically.
Padding::safe_area() for manual. .ignore_safe_area(true) for full bleed.
**Affects**: tezzera-platform, tezzera-layout

---

### D047 — Minimum OS Versions
**Status**: LOCKED
**Decision**: iOS 16+, Android API 24+, macOS 12+, Windows 10 1903+, Ubuntu 20.04+
Web: Chrome 90+, Firefox 90+, Safari 15+
**Affects**: tezzera-platform

---

## FFI

### D048 — FFI Bridges
**Status**: LOCKED
**Decision**: #[tezzera_ffi(c|swift|kotlin|js)] macros. Safe wrappers auto-generated.
All return TezzeraResult. catch_unwind at every boundary.
**Affects**: tezzera-ffi

---

### D049 — Synchronous Bridge
**Status**: LOCKED
**Decision**: sync_bridge::call<T>() for zero-serialization sync calls. SharedMemory for hot path.
**Affects**: tezzera-ffi

---

### D050 — FFI Memory Ownership
**Status**: LOCKED
**Decision**: Rust allocates → Rust frees. C allocates → C frees via ForeignBox.
Ownership transfer explicit. Never cross ownership silently.
**Affects**: tezzera-ffi

---

## CONCURRENCY

### D051 — Concurrency Model
**Status**: LOCKED
**Decision**: Single UI thread + Tokio async runtime + Rayon worker pool.
Atoms only written from UI thread. Workers communicate via channels.
**Affects**: tezzera-core, tezzera-state

---

## CLI

### D052 — CLI Name
**Status**: LOCKED
**Decision**: tzr
**Commands**: tzr dev, tzr build, tzr test, tzr analyze, tzr snapshot

---

## TESTING

### D053 — Golden Files
**Status**: LOCKED
**Decision**: Per-platform golden files. tests/goldens/desktop|mobile|web/
Threshold: 0%=pass, <1%=warn, >1%=fail. Configurable per test.
**Affects**: tezzera-test

---

### D054 — VSync Frame Scheduling
**Status**: LOCKED
**Decision**: `ControlFlow::Wait` + `EventLoopProxy<FrameRequest>`. `tezzera-state`
holds a global `OnceLock<Box<dyn Fn() + Send + Sync>>` wakeup fn. `Atom::set()`
calls `request_frame()` which sets an `AtomicBool` and invokes the wakeup fn.
Platform registers the proxy at startup. `AboutToWait` + `user_event` both call
`window.request_redraw()` when the flag is set. App idles at 0% CPU.
**Affects**: tezzera-state, tezzera-platform

---

### D055 — Key Mechanism
**Status**: LOCKED
**Decision**: `Key(u64)` newtype. `impl From<&str>` and `impl From<u64>` via
FNV hash. `Element::key: Option<Key>`. Reconciler matches keyed siblings by key
before falling back to position-based matching. No cross-tree key uniqueness
requirement — keys are local to their parent's child list.
**Affects**: tezzera-core, tezzera (reconciler)

---

### D056 — LayoutCtx
**Status**: LOCKED
**Decision**: `Widget::layout` changes from `(constraints: Constraints) -> Size`
to `(ctx: &LayoutCtx) -> Size` where `LayoutCtx { constraints, font, theme }`.
Font access in layout allows accurate glyph-metric-based text measurement.
`LayoutCtx::with_constraints(c)` creates a child context with tighter constraints.
**Affects**: tezzera-widgets

---

### D059 — Animation VSync Integration
**Status**: LOCKED
**Decision**: `tezzera-animate` owns a global frame-delta clock:
`static FRAME_DT: AtomicU32` (f32 bits stored as u32). Platform writes the
real elapsed time via `tezzera_animate::set_frame_dt(dt)` at the start of
every `RedrawRequested` event, before the render pass. All animation hooks
(`use_spring`, `use_animation`) read `frame_dt()` — never hardcode a timestep.
`dt` is clamped to `[0.001, 0.1]` seconds to survive tab-out / system sleep.
Platform adds `tezzera-animate` as a dependency. No registry, no callbacks —
the existing self-perpetuating atom pattern keeps frames coming while an
animation is running. The platform also tracks `last_frame_time: Instant` to
compute wall-clock dt.
A new `use_animation(ctx, duration) -> (Progress, AnimCtrl)` hook wraps
`AnimationController` with the same automatic ticking — user never calls
`tick(dt)` manually. `AnimCtrl::play()`, `pause()`, `reset()` are the full API.
`Progress::get()` returns `0.0..=1.0`, updating every frame while running.
**Reason**: Hardcoded `1/60` timestep is wrong on 120Hz monitors and broken
under frame drops. Animation should be frame-rate independent and driven by the
platform's real clock, exactly as widget painting is driven by VSync.
**Affects**: tezzera-animate, tezzera-platform

---

### D057 — RectReader / Geometry Callback
**Status**: LOCKED
**Decision**: A `RectReader` wrapper widget captures the screen-space `Rect`
of any child after layout and writes it into a user-supplied `Atom<Option<Rect>>`.
Fires inside `paint()` using `ctx.rect` — the exact window-pixel rect already
computed by the layout pass. No extra measurement, no separate pass.
`RectReader::new(atom, child)` — composes over any widget without modifying it.
The atom update triggers a frame, allowing other widgets to read the rect and
position themselves accordingly.
**Reason**: Real-world apps need to know where a widget landed so they can
position overlays, tooltips, dropdowns, and other context-sensitive UI relative
to it. This is the missing link between layout and the overlay system.
**Affects**: tezzera-widgets

---

### D058 — Overlay Layer (revised)
**Status**: LOCKED
**Decision**: A second `PictureRecorder` (overlay recorder) runs after the main
tree paint pass. The canvas replays main picture first, overlay picture second.
The overlay stack is an ordered `Vec<OverlayEntry>` — insertion order = z-order.

```rust
pub struct OverlayEntry {
    pub id:       LayerId,
    pub position: LayerPosition,   // Absolute(Point) | Centered | BottomAnchored | Fill
    pub widget:   BoxedWidget,     // interactive content only
    pub input:    InputBehavior,   // PassThrough | Block
    pub focus:    FocusBehavior,   // PassThrough | Trap | Inert
    pub scrim:    Option<ScrimConfig>,
}

pub struct ScrimConfig {
    pub color:  Color,
    pub on_tap: Option<Arc<dyn Fn() + Send + Sync>>,
    // None = absorb silently. Some(f) = call f when scrim area tapped (dismiss).
}
```

**Scrim** is renderer-owned — drawn as a FillRect before the widget, no hit
target registered for it. The `on_tap` callback fires when a click lands outside
the widget rect and the entry has a scrim. Scrim and `Block` are independent:
a decorative scrim can be `PassThrough`; a true modal can have no scrim.

**Input routing** — scan overlay stack top → bottom on every input event:
- Point hits `entry.widget` rect → deliver to widget, stop
- Point misses + `Block` → if `scrim.on_tap` exists fire it, else swallow, stop
- Point misses + `PassThrough` → continue down
- Nothing claimed → deliver to main tree

**Multiple dialogs** stack naturally. Each dialog entry is `Fill + Block + scrim`.
Dialog2 on top of Dialog1: clicking outside Dialog2 fires Dialog2's scrim dismiss
(or is swallowed). Dialog1 becomes active again once Dialog2 is popped.

**Bottom sheet** — `position: BottomAnchored`, `input: PassThrough`, optional scrim.
Clicks above the sheet miss the widget rect and fall through to main (PassThrough).
Tapping the scrim above the sheet calls `scrim.on_tap` to dismiss.

**Registry** is cleared at the start of every frame and rebuilt during paint.
**Reason**: Clean separation of visual (scrim as renderer rect), input (Block vs
PassThrough), and focus (Trap vs PassThrough). Multiple stacked modals work by
insertion order. Scrim tap-to-dismiss is explicit, not implicit.
**Affects**: tezzera-widgets, tezzera (App::launch render loop)

---

### D060 — Focus System
**Status**: LOCKED
**Decision**: `FocusManager` (already in `tezzera-a11y`) is extended to be
overlay-aware. Focus scope is determined by the topmost overlay entry with
`FocusBehavior::Trap` — Tab cycles only within that entry's focusable nodes.
When no Trap entry exists, Tab cycles globally across main tree + all overlay
entries in z-order (bottom to top).

Tab ordering within a scope uses `tab_index: Option<i32>` on widgets:
- `None` → natural tree order (default)
- `Some(n)` → explicit position; lower = earlier; ties broken by tree order
- `Some(-1)` → focusable by click but excluded from Tab cycle

`FocusManager::sync_with_overlay(stack, tree)` rebuilds focus order each frame
from the current overlay stack + main tree. Called after the paint pass.
**Affects**: tezzera-a11y, tezzera-widgets, tezzera (render loop)

---

### D061 — Navigation Route Stack
**Status**: LOCKED
**Decision**: `Navigator` in `tezzera-nav` manages a `Vec<Route>`. Only the
top route is active — rendered, hit-testable, focusable. All other routes are
**frozen**: component state (atoms, hook slots) is preserved in memory, but no
layout pass, no paint pass, and no hit targets or focus nodes are registered
from them.

```rust
pub struct Route {
    pub id:        RouteId,
    pub component: Box<dyn Component>,
    pub state:     FrozenState,   // atom values, hook slots preserved
}

pub enum FrozenState { Active, Frozen }
```

`Navigator::push(route)` → freezes current top, activates new route.
`Navigator::pop()` → drops top route (fires `on_unmount`, clears atom state
via `tezzera_state::clear_component()`), unfreezes and re-activates the route below.

Routes are **not** overlay entries. They replace the screen. Overlays sit above
the active route's render output. The navigator is orthogonal to the overlay stack.

A frozen route's atoms retain their values. Scroll positions, text inputs, and
all component state survive navigation round-trips. State is only cleared on
explicit pop (not on freeze).
**Reason**: Back-navigation should feel instant — no rebuild, no lost state.
Frozen routes cost memory but zero CPU. Clear separation from overlays prevents
the two systems from coupling.
**Affects**: tezzera-nav, tezzera (render loop)

---

### D062 — Co-location Overlay API
**Status**: LOCKED
**Decision**: Overlay entries are declared on the widget that triggers them, not
pushed manually to a global registry. Builder methods on interactive widgets
resolve to `OverlayEntry` pushed automatically by the framework:

```rust
Button::new("Open")
    .dropdown(is_open.clone(), || DropdownMenu::new()...)

Button::new("Settings")
    .sheet(is_open.clone(), || SettingsSheet::new()...)

Button::new("Delete")
    .dialog(is_open.clone(), || {
        Dialog::new("Are you sure?")
            .action("Cancel", || is_open.set(false))
            .action("Delete", on_delete.clone())
    })
```

Each method takes `Atom<bool>` (open/closed state) and a builder closure.
When the atom is true, the framework pushes the corresponding `OverlayEntry`
to the registry with correct `InputBehavior`, `FocusBehavior`, and `ScrimConfig`
pre-configured for each type:
- `.dropdown()` → PassThrough, PassThrough, no scrim, Absolute(anchor.bottom_left())
- `.sheet()`    → PassThrough, PassThrough, scrim+dismiss, BottomAnchored
- `.dialog()`  → Block, Trap, scrim+dismiss, Centered
- `.tooltip()` → PassThrough, Inert, no scrim, Absolute(anchor.top_left())

The global `OverlayEntry` registry (D058) remains the engine. This is pure API
sugar — zero engine change. `push_overlay()` remains available for advanced use.
**Reason**: Co-location (SwiftUI style) — overlay declared where trigger is —
is more readable and less error-prone than a global registry call site.
**Affects**: tezzera-widgets

---

### D063 — FocusNode Graph
**Status**: LOCKED
**Decision**: Replace `tab_index: Option<i32>` with a `FocusNode` reference
type. A `FocusNode` is a shared handle (`Arc<FocusNodeInner>`) that can be
attached to any focusable widget and wired to its neighbors:

```rust
let username = FocusNode::new();
let password = FocusNode::new();
let submit   = FocusNode::new();

TextInput::new("Username").focus_node(username.clone())
TextInput::new("Password").focus_node(password.clone())
    .focus_next(submit.clone())          // Enter / Tab → submit
    .focus_prev(username.clone())        // Shift+Tab → username
Button::new("Login").focus_node(submit.clone())
    .focus_prev(password.clone())
```

Each `FocusNode` stores: `next: Option<FocusNode>`, `prev: Option<FocusNode>`,
`focused: Atom<bool>` (reactive — widget reads this to draw focus ring).
`FocusNode::new()` with no explicit neighbors falls back to natural tree order.
`FocusNode::request()` programmatically focuses a node (e.g. auto-focus on mount).

`FocusManager` builds traversal order from the graph at sync time. If a node
has explicit `next`, follow it. Otherwise fall back to next node in tree order.

Grid navigation, arrow-key flows, and gamepad D-pad are all expressible as
neighbor connections — impossible with a flat integer.
**Reason**: Flutter's FocusNode model. Integer tab_index cannot express
non-linear focus (grids, carousels, custom keyboard flows).
**Affects**: tezzera-a11y, tezzera-widgets (Phase 14)

---

### D064 — Widget Semantic API
**Status**: LOCKED
**Decision**: Every widget can optionally declare semantic information that
feeds the `A11yTree`. Two mechanisms, both compile to the same `SemanticConfig`:

**1. Automatic — standard widgets self-annotate:**
Button, Checkbox, Slider, TextInput, Switch automatically provide semantics
based on their own properties. No user action needed.

**2. Builder methods — override or augment:**
```rust
Image::file("photo.png")
    .accessibility_label("A sunset over the mountains")

Container::new()
    .accessibility_role(Role::Navigation)

Text::new("3 unread messages")
    .accessibility_live()       // screen reader announces changes
```

**3. `Semantics` wrapper — for non-interactive widgets:**
```rust
Semantics::new(Role::Article)
    .label("News item")
    .child(Column::new()...)
```

The `Widget` trait gains:
```rust
fn semantics(&self) -> Option<SemanticConfig> { None }
```

`SemanticConfig`:
```rust
pub struct SemanticConfig {
    pub role:     Role,
    pub label:    Option<String>,
    pub hint:     Option<String>,
    pub value:    Option<String>,
    pub checked:  Option<bool>,
    pub disabled: bool,
    pub live:     bool,      // announces changes to screen reader
    pub hidden:   bool,      // excludes from a11y tree entirely
}
```

During the paint pass, alongside registering `HitTarget`s, widgets with
`semantics()` returning `Some(config)` add an `A11yNode` to the current frame's
`A11yTree`. The node receives `ctx.rect` as its bounds. The tree is rebuilt
every frame, synced into `FocusManager`, and eventually sent to platform
AT-SPI/UIA/AXKit (Phase 21).

**Reason**: SwiftUI modifier style + Flutter Semantics widget — both patterns
available so simple cases are simple and complex cases are possible.
**Affects**: tezzera-widgets, tezzera-a11y, tezzera (render loop)

---

### D065 — Persistent RenderNode Tree
**Status**: LOCKED
**Question**: How do we avoid full-tree layout + paint every frame?
**Decision**: Each native widget node in the tree is backed by a `RenderNode` that persists across frames. It caches `(last_constraints, cached_size, cached_picture, cached_rect, paint_dirty)`. On each frame, the reconciler diffs the new element tree against the existing RenderNode tree. Clean nodes (unchanged constraints + not paint_dirty) skip layout and reuse their cached Picture. Dirty nodes re-layout + re-paint and update their cache.
**Reason**: Full-tree re-layout + re-paint on every atom change wastes CPU. Caching at the widget granularity gives surgical updates without requiring immutable widget props (we can always mark dirty when unsure).
**Affects**: `tezzera` (umbrella — reconciler + render loop), `tezzera-render` (Picture must be Clone)

---

### D066 — Reconciler Algorithm
**Status**: LOCKED
**Question**: How does the reconciler match new elements to existing RenderNodes?
**Decision**: DFS by position within sibling list. For each position: if `new.tag == old.tag` AND keys agree (both absent OR both present with same value) → stable node, inherit cache; else → replace (new node, paint_dirty=true). Keyed children within the same parent are matched by key first, then unkeyed by position. A mismatch always creates a fresh RenderNode (forces re-layout + re-paint).
**Reason**: Type+position matching is O(n) DFS and handles the common case (stable tree). Key matching handles reordered lists without losing state.
**Affects**: `tezzera` (umbrella — reconciler)

---

### D067 — Dirty-Flag Layout and Paint
**Status**: LOCKED
**Question**: When does a RenderNode skip vs redo layout/paint?
**Decision**:
- **Layout skip**: if `node.last_constraints == Some(incoming)` AND NOT `layout_dirty` → return `node.cached_size.unwrap()`, skip subtree.
- **Layout redo**: else → call `widget.layout(ctx)`, store constraints + size, set `paint_dirty = true`.
- **Paint skip**: if NOT `paint_dirty` AND `node.cached_picture.is_some()` → replay picture at `node.cached_rect`, no widget.paint() call.
- **Paint redo**: else → record fresh Picture via widget.paint(), store picture + rect, clear `paint_dirty`.
Layout-dirty is set by the reconciler when a node is replaced. Paint-dirty is set whenever layout reruns or the reconciler marks it dirty.
**Reason**: Two-pass dirty tracking avoids painting layout-clean subtrees even when parent re-layouts due to sibling size changes.
**Affects**: `tezzera` (umbrella — render loop)

---

### D068 — O(depth) Hit Testing
**Status**: LOCKED
**Question**: How do we replace the flat linear hit-target scan?
**Decision**: Walk the RenderNode tree depth-first, visiting children before parent (post-order). For each node, check `cached_rect.contains(pointer)` and presence of `hit_handlers`. The first matching node wins. Overlay entries are checked first (top-to-bottom in insertion order) before the main tree.
**Reason**: A DFS walk is O(depth × branching_factor) rather than O(n). For typical widget trees (depth ≈ 20, branching ≈ 4) this is ~80 checks vs hundreds. Deepest-child-first mirrors visual stacking — the frontmost widget wins.
**Affects**: `tezzera` (umbrella — hit test)

---

### D069 — Focus System End-to-End Wiring
**Status**: LOCKED
**Question**: How does the FocusNode graph built in Phase 12 (D063) actually drive keyboard input?
**Decision**: Extend `App::launch` to maintain a `FocusManager` state across frames. After each paint pass, call `focus_manager.sync(focusable_nodes)` to rebuild the Tab-order list from the current frame's focusable nodes. On `KeyboardInput { key: Tab }` event:
- No Trap overlay → cycle globally through the Tab-order list
- A Trap overlay active → cycle only within that overlay's focusable nodes
`FocusManager::request(node_id)` → stores the active focus node ID; the widget for that node is rendered with `is_focused = true` via the focus context. `FocusManager::release()` → clears active focus.
Implementation: Add `focused_id: Option<u64>` to App frame state. Pass a `FocusCtx { focused_id }` through the paint pass alongside `PaintCtx`. Widgets that implement `FocusApi` check `FocusCtx.is_focused(self_id)` to style themselves.
**Reason**: The FocusNode graph defines connectivity (who is next/prev). The FocusManager drives it. Together they replace ad-hoc focus state in each widget.
**Affects**: `tezzera` (umbrella), `tezzera-a11y`, `tezzera-widgets`

---

### D070 — Navigation Route Stack Wiring
**Status**: LOCKED
**Question**: How does `tezzera-nav` (Navigator, RouteStack) integrate with the App render loop?
**Decision**: `Navigator` is a root-level component that holds a `Vec<Route>` in app state. Each `Route` wraps a `Box<dyn Component>`. `Navigator::build()` renders only the top route. Frozen routes are held in memory but not rebuilt.

`push_route(component)` → creates a new Route entry, pushes to Vec, triggers rebuild.
`pop_route()` → drops top route, fires `on_unmount`, clears atom state via `clear_component()`, triggers rebuild.

Navigator stores the route stack in an `Atom<Vec<Arc<RouteEntry>>>`. Changes to this atom trigger a frame. Routes below the top are not walked by `walk_element` (frozen = invisible to layout, paint, and hit test).

Integration: `tezzera-nav` already has the `stack.rs` stub. Phase 14 fills it in and adds `Navigator` as a first-class component in the prelude.
**Reason**: D061 spec is fully described; this decision locks the wiring to the render loop. Route freezing happens at the element-walk level — non-top routes are simply not walked.
**Affects**: `tezzera-nav`, `tezzera` (umbrella), `tezzera::prelude`

---

### D071 — RepaintBoundary Widget
**Status**: LOCKED
**Question**: How should isolated PictureLayer caching be exposed to users?
**Decision**: `RepaintBoundary::new(child)` — a wrapper widget that maintains its own isolated `PictureRecorder`. On each paint pass, if the child's `paint_dirty` flag is false AND the boundary's cached Picture exists, the boundary replays only its own cached Picture into the parent recorder — zero widget.paint() calls inside the boundary.

`RepaintBoundary` forces a subtree boundary in the RenderNode tree. Any `Atom` write that touches a widget inside a `RepaintBoundary` only invalidates that boundary's Picture, not sibling boundaries.

Implementation: `RepaintBoundary` is a `NativeElement` with tag `"RepaintBoundary"`. Its `RenderNode` stores `own_picture: Option<Arc<Picture>>` in addition to the normal fields. In `walk_element`, when the current native element has tag `"RepaintBoundary"` and `!paint_dirty`, it replays `own_picture` directly.
**Reason**: Phase 13 caches Pictures per native widget position. RepaintBoundary formalizes the concept — a child subtree with its own isolated Picture whose invalidation is independent of siblings.
**Affects**: `tezzera-widgets`, `tezzera` (umbrella)

---

### D072 — GPU Backend Choice
**Status**: LOCKED
**Question**: Which GPU API should the compositor target?
**Decision**: **wgpu** (not raw Vulkan/Metal/DX12). wgpu selects the best native backend per OS at runtime (Metal on macOS, Vulkan/DX12 on Windows/Linux). Pure-Rust API, no C++ toolchain required. D032 is unaffected — tiny-skia remains the CPU rasterizer; wgpu is the display backend only. The swap is isolated to `tezzera-compositor` + `tezzera-platform`.
**Reason**: GPU blit via wgpu enables 120fps and future multi-layer GPU compositing without a full CPU readback per frame.
**Affects**: `tezzera-compositor` (new crate), `tezzera-platform`

---

### D073 — GPU Texture Pixel Format
**Status**: LOCKED
**Question**: What pixel format is used for the CPU→GPU upload?
**Decision**: Upload as `Rgba8Unorm`; the WGSL shader reads it directly. tiny-skia produces RGBA8. The wgpu surface format is queried from the adapter at init time and the compositor matches it — no manual format detection needed.
**Reason**: `Rgba8Unorm` is universally supported and matches tiny-skia's byte order directly.
**Affects**: `tezzera-compositor`

---

### D074 — Compositor Architecture
**Status**: LOCKED
**Question**: Where does wgpu initialization live and how does it integrate with tezzera-platform?
**Decision**: Standalone `tezzera-compositor` crate exports `GpuPresenter`. `tezzera-platform` depends on it and initializes `GpuPresenter` in `AppState::resumed()`. If wgpu init fails, `presenter = None` and the softbuffer fallback path activates silently. No feature flag — the GPU path is always attempted.
**Reason**: Keeps wgpu entirely out of the widget/render crates. Softbuffer fallback prevents crashes on CI/headless environments.
**Affects**: `tezzera-compositor` (new), `tezzera-platform`

---

### D075 — Compositor Shader
**Status**: LOCKED
**Question**: What is the compositor's render pipeline?
**Decision**: Minimal WGSL fullscreen-quad shader. Vertex shader generates 6 vertices from `vertex_index` (two triangles, no vertex buffer). Fragment shader samples the uploaded frame texture with nearest-neighbour filtering (pixels are already at physical resolution — no upscaling needed). No mipmaps, no sRGB correction (tiny-skia already handles gamma).
**Reason**: Minimum viable GPU blit. No vertex buffers, no index buffers, no uniform buffers. A single bind group with texture + sampler is all that's needed.
**Affects**: `tezzera-compositor`

---

### D076 — Layer Compositing Model
**Status**: LOCKED
**Question**: How should multiple render layers be composited?
**Decision**: Each logical layer (base, overlay) is a separate `SkiaCanvas`. Each canvas produces its own RGBA pixel buffer. `GpuPresenter::present_layers(&[CompositorLayer])` uploads N textures and composites them bottom-to-top via `SRC_ALPHA` over `ONE_MINUS_SRC_ALPHA` in two sequential render passes.
**Reason**: Isolates base/overlay rendering so overlay changes (dialog show/hide) do not force a base layer CPU re-render. Foundation for per-layer opacity and transform in Phase 17.
**Affects**: `tezzera-compositor`, `tezzera/src/lib.rs`, `tezzera-render`

---

### D077 — CompositorLayer Struct
**Status**: LOCKED
**Question**: What is the interface between the render loop and the GPU compositor for multi-layer presentation?
**Decision**: `pub struct CompositorLayer<'a> { pixels: &'a [u8], width: u32, height: u32, opacity: f32 }`. `GpuPresenter::present_layers(&[CompositorLayer])` composites them. The old `present()` is kept as a shim for backward compatibility.
**Reason**: Minimal struct — only what's needed for Phase 16. `opacity` is per-layer, applied as a scalar to the source alpha before blending.
**Affects**: `tezzera-compositor`

---

### D078 — Overlay Canvas Clear
**Status**: LOCKED
**Question**: How is the overlay canvas initialized each frame?
**Decision**: `SkiaCanvas::clear_transparent()` fills the pixmap with RGBA(0,0,0,0) before each overlay paint pass. Transparent pixels in the overlay texture pass through to the base layer via the blend equation.
**Reason**: Ensures overlay content from the previous frame does not persist when overlays are closed or repositioned.
**Affects**: `tezzera-render`, `tezzera/src/lib.rs`

---

### D079 — Multi-Layer WGSL Shader
**Status**: LOCKED
**Question**: How does the compositor shader blend N layers?
**Decision**: Two sequential render passes on the same surface target. Pass 1: blit base texture with `REPLACE` blend (opaque). Pass 2: blit overlay texture with `SRC_ALPHA` / `ONE_MINUS_SRC_ALPHA` blend (alpha-over). The overlay pipeline uses `blend: Some(wgpu::BlendState::ALPHA_BLENDING)`. Both passes use the same fullscreen-quad vertex shader.
**Reason**: Two-pass avoids binding limitations and works on all wgpu backends. Fragment output is `base_color * (1 − overlay.a) + overlay_color * overlay.a` — the standard Porter-Duff over operation.
**Affects**: `tezzera-compositor`

---

### D080 — TransformLayer Model
**Status**: LOCKED
**Question**: How does TransformLayer capture content and apply GPU-side scroll?
**Decision**: `TransformLayer<W>` wraps a child widget. `layout()` reports the viewport size. `paint()` shifts the child origin by `-scroll_y` (and `-scroll_x`) so content scrolls within the viewport. `CompositorLayer.offset` carries the UV-space scroll offset to the GPU; the shader returns transparent for out-of-range UV. Phase 17 uses CPU shift; Phase 18 adds full GPU-texture-per-scroll.
**Reason**: Establishes the widget API and GPU offset pipeline. Phase 18 replaces the CPU shift with a frozen-texture + uniform-only update path.
**Affects**: `tezzera-widgets`, `tezzera-compositor`

---

### D081 — Transform Uniform Buffer
**Status**: LOCKED
**Question**: How is the scroll offset passed to the WGSL shader?
**Decision**: A `wgpu::Buffer` with `UNIFORM | COPY_DST` usage holds `[f32; 4]` = `[offset_x, offset_y, 0.0, 0.0]`. The shader reads `@group(0) @binding(2) var<uniform> u_layer: LayerUniforms`. UV is shifted by `u_layer.offset`; out-of-range returns `vec4(0)` (transparent). Buffer is created via `create_buffer_init` each frame in Phase 17; Phase 18 will reuse a persistent buffer and `queue.write_buffer`.
**Reason**: Minimal change to bind group layout — add one uniform binding. No vertex buffer changes. All existing layers pass `(0.0, 0.0)` offset with zero overhead.
**Affects**: `tezzera-compositor`

---

### D082 — TransformLayer Size Limit
**Status**: LOCKED
**Decision**: Phase 17 caps at `MAX_TRANSFORM_DIM = 4096` physical pixels. Content exceeding this falls back to CPU clip scroll (unchanged behaviour). Cap is checked at capture time; a debug warning is emitted.
**Affects**: `tezzera-widgets`

---

### D083 — ScrollView Integration
**Status**: DEFERRED → Phase 18
**Decision**: Phase 17 provides `TransformLayer<W>` as a first-class widget. Phase 18 integrates it into `ScrollView` transparently. Users can use `TransformLayer` directly in Phase 17.
**Affects**: `tezzera-widgets`

---

### D084 — ScrollView::live reactive constructor
**Status**: LOCKED
**Decision**: `ScrollView::live(child, atom: Atom<f32>)` is a second constructor that stores the atom. In `paint()`, if `live_offset` is `Some`, the atom value overrides the static `offset` field. The owning component subscribed to the atom via `ctx.state()`; when the atom changes the component rebuilds and `paint` reads the new offset. The static `ScrollView::new` + `.offset(n)` path is unchanged for snapshot scenarios.
**Reason**: Reactive scrolling without gesture infrastructure. A button click writes the atom; the rebuild renders the new offset.
**Affects**: `tezzera-widgets`

---

### D085 — N-layer compositor
**Status**: LOCKED
**Decision**: `present_layers` iterates an arbitrary `&[CompositorLayer]` slice — no hard cap on layer count. Each layer gets its own texture upload + render pass. Performance is O(N) render passes. Phase 19 will batch into a texture atlas.
**Affects**: `tezzera-compositor`

---

### D086 — TransformLayer render-tree discovery deferred to Phase 19
**Status**: DEFERRED
**Decision**: Phase 18 does not add `PaintCtx.transform_layers`. TransformLayer uses the CPU-shift model from Phase 17. Full frozen-texture per layer (separate canvas capture, persistent GPU texture, uniform-only update) is Phase 19.
**Reason**: Adding `PaintCtx.transform_layers` requires platform changes to allocate canvases before the render walk. Phase 18 ships the reactive ScrollView win without that complexity.
**Affects**: `tezzera-widgets`, `tezzera-platform`

---

### D087 — TransformLayerEntry in PaintCtx
**Status**: LOCKED
**Decision**: `PaintCtx` carries `transform_entries: Rc<RefCell<Vec<TransformLayerEntry>>>`. A `TransformLayerEntry` holds: `picture: Picture` (recorded child), `child_size: Size`, `viewport_rect: Rect`, `scroll_x: f32`, `scroll_y: f32`. `TransformLayer::paint()` records child into a sub-`PictureRecorder`, finishes it, and pushes the entry. It does NOT emit to the main recorder. The `transform_entries` vec is shared (Rc-cloned) through `child()` like `hit_targets`.
**Affects**: `tezzera-widgets`, `tezzera-render`

---

### D088 — Platform TransformLayer replay (D088)
**Status**: LOCKED
**Decision**: After the main paint pass, `tezzera/src/lib.rs` iterates `transform_entries`. For each entry, it translates all DrawCommands by `(viewport.origin - scroll_offset)` using the new `DrawCommand::offset(dx, dy)` method, finishes a temporary PictureRecorder, and replays it onto the base canvas. This gives correct scroll positioning without a separate GPU layer per TransformLayer (that's Phase 20).
**Affects**: `tezzera`, `tezzera-render`

---

### D089 — GPU texture caching
**Status**: LANDED (Phase 20 Step 6)
**Decision**: Full "zero re-upload on scroll" (persistent wgpu Texture keyed by layer, reused across frames) is Phase 20. Phase 19 re-plays the Picture each frame into a new pixel buffer. The architecture is correct; the caching layer is an optimization.
**Implementation**: `GpuPresenter` holds `cached_layers: Vec<CachedLayer>` (persistent texture + bind group + uniform buffer per slot). `CompositorLayer::dirty` drives it: clean layers reuse their texture (no `write_texture`), offset changes are a `write_buffer`, and a frame where every layer is clean and unmoved skips the present entirely. Dirtiness flows `SkiaCanvas::frame_dirty` (set by the frame loop on repaint) → `take_frame_dirty` in the platform → `CompositorLayer::tracked`. Verified: idle/hover frames upload nothing.
**Affects**: `tezzera-compositor`, `tezzera-render`, `tezzera-platform`, `tezzera`

---

### D090 — ScrollView integration with TransformLayer
**Status**: FOUNDATION LANDED (Phase 20 Step 6) — ScrollView routing + no-repaint scroll remain
**Decision**: Phase 19 provides `TransformLayer<W>` as a direct-use widget. Phase 20 integrates it into `ScrollView::live` transparently.
**Implementation (foundation)**: The compositor now supports *placed* layers — `CompositorLayer::placed(pixels, w, h, dest, src_offset, dirty)` positions a quad at a screen-space `dest` rect (physical px) and samples a content-sized texture at `src_offset` (the shader maps `uv_min + corner*uv_span`, out-of-range UV → transparent for viewport/content clipping). The frame loop renders each `TransformLayerEntry` once into its own `SkiaCanvas` and publishes it via the `tezzera-platform::scroll_layer` thread-local registry; the platform composites `base + scroll layers + overlay`, retaining the scroll set across clean frames. Composes with D089 (a clean scroll layer skips re-upload; an offset-only change is a uniform write). Verified via app_demo "GPU Scroll Layer".
**Remaining**: route `ScrollView::live` through this (today it paints into the base canvas); make a scroll tick update only `src_offset` without a component repaint (the zero-CPU-paint payoff); hit-testing through the offset; content > `MAX_TL_DIM` (4096px) re-render windowing.
**Affects**: `tezzera-compositor`, `tezzera-platform`, `tezzera`, `tezzera-widgets`

---

### D091 — RenderTree owns all per-node retained state
**Status**: LOCKED
**Decision**: The persistent render tree (`RenderNode`) is the single owner of everything a widget produces that must outlive one frame: layout cache, cached Picture, hit regions, scroll regions, focus nodes, overlay attachments, and transform layers. `paint()` becomes side-effect free with respect to the frame: it records commands and *declares* regions/attachments onto its own RenderNode. The frame pipeline then derives the display list, hit-test order, focus cycle, overlay stack, and compositor layers from the tree — nothing is re-emitted per frame through `Rc<RefCell<Vec>>` side channels or thread-locals.
**Reason**: Three independent bugs came from the same disease: state produced only during `paint()` dies on cache-hit frames (hit handlers → fixed a1e91b8; TransformLayerEntries → D088 cache; overlay entries → cached_overlay_entries). Each got its own bolt-on cache. D091 makes the bug class unrepresentable and is the foundation for damage-rect repainting and real RepaintBoundary caching. The existing keyed reconciler (`tezzera/src/reconcile.rs`) becomes the actual tree-update mechanism (it is currently dead code — `walk_element` inlines its own tag matching).
**Affects**: `tezzera`, `tezzera-widgets`, `tezzera-a11y`

---

### D092 — Tree-walk hit testing with structural z-order
**Status**: LOCKED
**Decision**: Input dispatch walks the RenderTree back-to-front (overlay roots first, then main root; within a node, children in reverse paint order) instead of scanning a flat `Vec<HitTarget>` with insert-at-0 ordering tricks. A node can consume, pass through, or transform events (scroll offset translation). Scrims become ordinary nodes that consume misses — replacing the four-strip hit-rect workaround. Z-order is structural, not an artifact of registration order.
**Affects**: `tezzera`, `tezzera-widgets`, `tezzera-gesture`

---

### D093 — Constructor Law
**Status**: LOCKED
**Decision**: `Widget::new()` takes exactly the required content — content leaves take their content (`Text::new(str)`), required-child wrappers take the child (`Card::new(child)`), optional-child and multi-child widgets take nothing (`Container::new()`, `Column::new()`). Everything optional is a builder method. Never two required positional args of the same type. Named constructors are shortcuts, never replacements for `new()`. Full spec: `.steering/API_DESIGN.md` §1.
**Affects**: tezzera-widgets (all), tezzera-examples

---

### D094 — Property Vocabulary
**Status**: LOCKED
**Decision**: One builder name per concept across all widgets: `.background()` = surface fill (never `.color()`/`.bg()`), `.color()` = content/foreground only, `.border(color, width)`, `.radius()`, `.elevation()`, `.padding(EdgeInsets)`, `.width/.height/.size`, `.spacing()`, `.align(Alignment)`, `.on_press()`, `.on_change()`, `.disabled()`. Table in API_DESIGN.md §3 is normative.
**Affects**: tezzera-widgets (all)

---

### D095 — Widget Consolidation: one box
**Status**: LOCKED
**Decision**: `Container` is the single box widget. `ColoredBox`, `SizedBox`, `Padding`, `Center` are removed (migration table in API_DESIGN.md §5). `Card` survives only as a themed Container preset. The element-based widget structs in `tezzera-layout` are removed; that crate keeps only layout math. New-widget bar: must draw or lay out something new — presets are named constructors, not widgets.
**Reason**: Six widgets did one widget's job; learning curve scales with rules × widgets.
**Affects**: tezzera-widgets, tezzera-layout, tezzera-examples

---

### D096 — Widget styling = builder chain
**Status**: LOCKED
**Decision**: Builder-chain styling (`Text::new("hi").size(20.0).weight(Bold)`) is the primary API. Style-struct arguments are rejected (Rust lacks named/optional args → `..Default::default()` noise at every call site). Reusable styles come later as a single additive `.style(TextStyle)` method bridging to tezzera-style — deferred to style-system integration.
**Affects**: tezzera-widgets, tezzera-style

---

### D097 — Canonical scroll + navigation APIs
**Status**: LOCKED
**Decision**: `ScrollView::new(child, atom)` is live by default; static mode renamed `ScrollView::fixed(child, offset)`; `Column::scrollable(atom)`/`Row::scrollable(atom)` as planned sugar (Expanded is ignored on an unbounded scroll axis). `ScreenNav<R>` is the one routing API; `Navigator`/`Route`/history/guards and nav-anim's Navigator become internal machinery, removed from prelude. `AppBar::back_button(&nav)` replaces the manual can_pop/leading boilerplate.
**Affects**: tezzera-widgets, tezzera-nav, tezzera-nav-anim

---

### D098 — Two-concept model + taxonomy by defaults
**Status**: LOCKED
**Decision**: Users learn exactly two concepts: `Component` (reactive — *what* to show) and `Widget` (primitive protocol — *how* to size/draw/behave). `Element` and the render tree are internal. The Leaf/SingleChild/MultiChild taxonomy is NOT three traits (blanket `impl Widget` for multiple taxonomy traits violates Rust coherence); it is one `Widget` trait with a `children() -> Children` accessor (`None`/`One`/`Many`) and smart defaults keyed off it — the taxonomy is which defaults you keep. Full spec: `.steering/WIDGET_PROTOCOL.md`.
**Affects**: tezzera-widgets, tezzera-core, docs

---

### D099 — Authoring contexts: framework-owned child geometry + declarative semantics
**Status**: LOCKED
**Decision**: `LayoutCx` gains `layout_child(i, constraints)` (framework-memoized on the render tree) and `position_child(i, point)` (stored); `PaintCx` gains `paint_child(i)` reading stored positions. Per-widget measure caches are deleted; measure/paint drift becomes unrepresentable; per-child picture caching and damage rects (Phase 20 Steps 1/5) get their tree from this. `semantics(&self, cx)` declares role/label/actions onto the widget's render-tree node (single-owner, D091) — activates the dormant SemanticNode/Role types (D035/D064).
**Affects**: tezzera-widgets, tezzera, tezzera-a11y

---

### D100 — CustomPaint is a recorded Leaf (amends D034)
**Status**: LOCKED — supersedes D034's "full SkiaCanvas access" wording
**Decision**: `CustomPaint::new(|cx, size| ...).repaint_when(atom)` — a Leaf widget whose closure records DrawCommands. No direct pixel/canvas access at paint time (would bypass the retained pipeline — the D091 vanishing-state bug class). Pixel-level needs use `DrawCommand::BlitRgba`. Hit testing via the standard protocol.
**Affects**: tezzera-render, tezzera-widgets

---

### D101 — Default scroll controllers on the render tree
**Status**: LOCKED
**Decision**: Every scrollable widget scrolls by default with zero wiring: its render-tree node lazily owns a `ScrollController` (persistent per-node state, NOT cleared on repaint — like Flutter's implicit ScrollPosition). `PaintCtx` carries the owning `ComponentId`; node-created controllers subscribe it so writes dirty the component (the no-subscriber trap proven by the b37d9e0 bug). APIs: `ScrollView::new(child)` / `::horizontal(child)` / `Column::scrollable()` / `Row::scrollable()` / `ListView::builder(count, extent, f)` take no scroll state; `.controller(ctrl)` / `ScrollView::controlled(child, ctrl)` opt into programmatic control; raw scroll atoms are removed from the public API. `ScrollView::fixed(child)` remains the inert snapshot mode.
**Reason**: "Create an atom in build() and thread it down" was boilerplate on every scrollable and a footgun (forgotten atom = broken scroll). The OOP 'scrollables always have a controller' model, translated to Rust as per-node retained state (D091's home) + composition.
**Affects**: tezzera-widgets, tezzera-scroll, tezzera, tezzera-examples

---

## DEFERRED DECISIONS

```
D-DEF-001  TEZZERA Studio design          → Phase 4
D-DEF-002  Wide color / HDR              → Phase 3
D-DEF-003  2D bidirectional scroll       → Phase 3
D-DEF-004  Plugin registry governance    → Phase 4
D-DEF-005  OS version review at v1.0     → v1.0
D-DEF-006  RustRover plugin              → Phase 3b
D-DEF-007  Package manager              → Phase 4
D-DEF-008  Server-side rendering        → not planned
D-DEF-009  React Native interop         → not planned
D-DEF-010  Embedded/no-std             → Phase 5
```
