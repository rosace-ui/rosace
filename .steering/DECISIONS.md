# ROSACE — DECISIONS
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
**Question**: How does ROSACE track if a component is the same instance across rebuilds?
**Decision**: Position in tree by default. Optional `.key(value)` when order can change.
**Reason**: Position default keeps simple cases simple. Keys available for dynamic lists. Compiler warns when dynamic lists have no keys.
**Affects**: rosace-core

---

### D002 — Component Lifecycle
**Status**: LOCKED
**Question**: Do components have lifecycle hooks?
**Decision**: Yes. Three hooks: on_mount, on_update, on_unmount. All tree-driven.
- on_mount → fires once when added to tree, return fn for cleanup
- on_update → fires when own props change, receives previous props
- on_unmount → fires once when removed from tree
**Reason**: Real apps always need mount/unmount for connections, timers, resources. Reactivity to atoms is separate and automatic.
**Affects**: rosace-core
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
**Affects**: rosace-core, rosace-macros
**Multi-child API**: ChildContainer trait on all multi-child widgets
**Order guarantee**: Children render in exact addition order

---

### D004 — Error Boundaries
**Status**: LOCKED
**Question**: What happens when a component panics?
**Decision**: Two-layer system:
- Layer 1: RosaceResult for expected failures, propagate with ?
- Layer 2: ErrorBoundary for unexpected panics, shows fallback
**Reason**: Expected and unexpected failures need different handling.
**Affects**: rosace-core
**Rules**:
- Errors bubble up to nearest ErrorBoundary
- App-level fallback as final safety net
- Dev mode: full overlay with stack trace
- Production: clean fallback, silent logging
- ErrorBoundary cannot catch its own errors
- Async errors must use RosaceResult

---

### D005 — Lazy Components
**Status**: LOCKED
**Question**: Should components load code only when needed?
**Decision**: Route components lazy by default. Non-route eager by default.
- #[lazy] to opt-in non-route components
- #[eager] to opt-out route components
- Loading state required for all lazy components
**Reason**: Large apps need code splitting. Routes are natural split points.
**Affects**: rosace-core, rosace-macros, rosace-cli
**Dev mode**: All eager, loading instant

---

## STATE SYSTEM

### D006 — State Primitive
**Status**: LOCKED
**Question**: What is the core state primitive?
**Decision**: Atom<T> — a reactive value. When changed, subscribers rebuild.
**Reason**: Simplest possible primitive. Everything else builds on top.
**Affects**: rosace-state
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
**Affects**: rosace-state
**Rules**:
- Local atoms cannot escape their component
- Scoped atoms outside provider = compile error
- Global atoms overused = lint warning
- Provider can be nested, inner wins

---

### D008 — Atom Persistence
**Status**: LOCKED — `permanent` tier IMPLEMENTED 2026-07-15 by D114/D121 (Phase 31) as `ctx.state_permanent(key, default)`; `reload`/`session` are documented no-ops by construction until D102 hot-reload exists; `encrypted` gated on a future D106 secure-storage capability (no plaintext fallback). See D121 for the hook-model re-homing.
**Question**: Does atom state survive hot reload, backgrounding, restart?
**Decision**: Opt-in per atom. Three levels:
- #[persist(reload)] — survives hot reload
- #[persist(session)] — survives backgrounding
- #[persist(permanent)] — survives restart
- #[no_persist] — explicitly blocked
- #[persist(permanent, encrypted)] — secure storage
**Reason**: Not all state should persist. Developer decides.
**Affects**: rosace-state, rosace-platform
**Rules**:
- Only provided and global atoms can persist
- Type must impl Serialize + Deserialize
- Type change → graceful reset, never crash
- Migration support for permanent atoms

---

### D009 — Async Atoms
**Status**: LOCKED
**Question**: How does ROSACE handle async operations?
**Decision**: use_async family with five states: Idle, Loading, Success, Error, Refreshing
- use_async → auto fetch on mount
- use_async_lazy → manual trigger
- use_async_when → conditional
- use_async_all! → parallel
**Reason**: Async is everywhere. Must be first class.
**Affects**: rosace-state
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
**Affects**: rosace-state

---

### D011 — Smart Refresh Engine
**Status**: LOCKED
**Question**: How does ROSACE minimize rebuilds?
**Decision**: Find dirty roots, prune descendants, rebuild minimum set.
Algorithm:
1. Collect dirty components from atom changes
2. Prune descendants (parent dirty = skip children)
3. Rebuild from roots only
4. Single layout pass
5. Single paint pass
**Reason**: Parent rebuild covers children. No double work.
**Affects**: rosace-state, rosace-core
**Tree index**: DFS timestamps, O(1) ancestor lookup

---

### D012 — External State
**Status**: LOCKED
**Question**: How does ROSACE connect to external sources?
**Decision**: Stream<T> as universal bridge. Typed adapters on top.
Built-in: use_websocket, use_query, use_file_watch, use_sensor, use_network_status, use_app_lifecycle
**Reason**: Everything external is a stream of values.
**Affects**: rosace-state, rosace-platform
**Rule**: All connections auto-cleaned on unmount

---

## LAYOUT ENGINE

### D013 — Layout Engine Name
**Status**: LOCKED
**Decision**: Flexure
**Affects**: rosace-layout

---

### D014 — Constraint Model
**Status**: LOCKED
**Decision**: Constraints with AxisBound: Bounded(f32) | Unbounded | Shrink
Three-pass layout: Measure (top-down), Place (bottom-up), Paint
**Affects**: rosace-layout

---

### D015 — Fractional Sizing
**Status**: LOCKED
**Decision**: Modifier primary (.width(Width::fraction(0.5))). FractionallySizedBox for complex cases.
Fraction is of AVAILABLE space, not screen size. Respects parent constraints.
**Affects**: rosace-layout

---

### D016 — Intrinsic Sizing
**Status**: LOCKED
**Decision**: Explicit opt-in only. IntrinsicHeight, IntrinsicWidth, IntrinsicSize widgets.
Zero cost when not used. Built into Dialog, Tooltip, BottomSheet.
Dev warning when used inside ScrollView.
**Affects**: rosace-layout

---

### D017 — Baseline Alignment
**Status**: LOCKED
**Decision**: Opt-in per Row. Row.align(Alignment::Baseline) or per-child .align_self(Alignment::Baseline).
Default is top alignment.
**Affects**: rosace-layout

---

### D018 — Overlay System
**Status**: LOCKED
**Decision**: Six layers: 0=Content, 1=Navigation, 2=Modal barrier, 3=Modals, 4=Overlays, 5=DevTools
Overlay::show(), Modal::show() APIs. Auto-reposition if off screen.
**Affects**: rosace-layout, rosace-render

---

### D019 — Text Layout
**Status**: LOCKED
**Decision**: cosmic-text foundation, HarfBuzz shaping, fontdue rasterization, Skia rendering.
BiDi automatic. Font fallback chain. Glyph cache (GPU atlas). Layout cache.
Desktop: subpixel. Mobile: grayscale.
**Affects**: rosace-layout, rosace-render

---

### D020 — RTL Support
**Status**: LOCKED
**Decision**: Day 1. Automatic mirroring on RTL locale.
Logical sides (.padding_start/end) auto-mirror. Physical (.padding_left/right) never mirror.
Icons: .mirror_in_rtl(bool). Force LTR: Directionality::ltr().
**Affects**: rosace-layout

---

## SCROLL

### D021 — Bidirectional Scroll
**Status**: DEFERRED (Phase 3)
**Decision**: Phase 1+2 = 1D only. API reserved: ScrollView2D::new()
**Affects**: rosace-scroll

---

### D022 — Sticky Headers
**Status**: LOCKED
**Decision**: Built into VirtualList, day 1. .sticky_headers(true) default.
**Affects**: rosace-scroll

---

### D023 — Pull to Refresh
**Status**: LOCKED
**Decision**: Built into ScrollView. .pull_to_refresh(|| async { }).
Platform feel per target. Desktop: not shown.
**Affects**: rosace-scroll

---

### D024 — Infinite Scroll
**Status**: LOCKED
**Decision**: .on_end_reached() + .end_threshold(n) on VirtualList.
PaginatedState pattern built-in.
**Affects**: rosace-scroll, rosace-state

---

### D025 — Scroll Restoration
**Status**: LOCKED
**Decision**: Automatic per route. .restore_position(false) to opt out.
App restart = position reset. Session only.
**Affects**: rosace-scroll, rosace-nav

---

## NAVIGATION

### D026 — Route Definition
**Status**: LOCKED
**Decision**: #[routes] enum with #[route("/path")] attributes. Type-safe. Auto deep link.
**Affects**: rosace-nav, rosace-macros

---

### D027 — Nested Navigation
**Status**: LOCKED
**Decision**: Full nested navigation, unlimited depth. Each navigator independent history.
Tab switch: each tab remembers its stack.
**Affects**: rosace-nav

---

### D028 — Navigation Guards
**Status**: LOCKED
**Decision**: Async guards via use_before_leave(). Global guards via Navigator::guard().
NavigationDecision: Allow | Block | RedirectTo(route)
**Affects**: rosace-nav

---

### D029 — Back Button
**Status**: LOCKED
**Decision**: use_back_handler() per screen. Default: pop if history, else exit.
BackHandlerResult: Pop | Block | Custom(fn)
**Affects**: rosace-nav, rosace-platform

---

### D030 — Keep Alive
**Status**: LOCKED
**Decision**: Opt-in per tab. keep_alive: true. Memory budget with LRU eviction.
KeepAlive widget for non-tab use.
**Affects**: rosace-nav, rosace-core

---

### D031 — Web URL Sync
**Status**: LOCKED
**Decision**: Automatic. Browser back/forward = Navigator pop/push. Query params supported.
Hash routing option: rsc build --web-routing=hash
**Affects**: rosace-nav, rosace-platform

---

## RENDERING

### D032 — Renderer
**Status**: LOCKED
**Decision**: **D032**: Renderer backend — **tiny-skia** (pure Rust, CPU) for MVP. Swap to skia-safe (C++ Skia, GPU) at v1.0. Isolated in `rosace-render/src/canvas.rs` (~100 lines to swap). Rationale: skia-safe requires C++ toolchain (30-60 min build), breaks wasm32 target, needs Emscripten. tiny-skia builds in seconds and is WASM-compatible.
**Affects**: rosace-render

---

### D033 — Image Handling
**Status**: LOCKED
**Decision**: Always decode on background thread. Three-level cache: memory → disk → network.
Formats: PNG, JPEG, WebP, AVIF, GIF, SVG, APNG.
Memory: LRU 50MB. Disk: LRU 200MB.
**Affects**: rosace-render

---

### D034 — Custom Painters
**Status**: LOCKED
**Decision**: CustomPaint widget with full SkiaCanvas access. Hit tester required. repaint_when for efficiency.
**Affects**: rosace-render

---

### D035 — Accessibility
**Status**: LOCKED
**Decision**: Semantic tree built always. Platform bridges:
iOS=UIAccessibility, Android=AccessibilityNodeInfo, Web=ARIA, Desktop=OS APIs
FocusScope for focus management and trapping.
**Affects**: rosace-render, rosace-platform

---

### D036 — HDR / Wide Color
**Status**: DEFERRED (Phase 3)
**Decision**: sRGB for Phase 1+2. API reserved: .color_space(ColorSpace::DisplayP3)
**Affects**: rosace-render

---

## UI CUSTOMIZATION

### D037 — Customization Levels
**Status**: LOCKED
**Decision**: Five levels:
1. Theme tokens (#[derive(RosaceTheme)])
2. Component styling (modifier chain)
3. Component override (WidgetOverride trait)
4. Custom RenderObject (RenderObject trait)
5. Custom render pipeline (RosaceRenderer trait)
**Affects**: rosace-render, rosace-theme, rosace-core

---

### D038 — Theme System
**Status**: LOCKED
**Decision**: #[derive(RosaceTheme)] — exhaustive, typed. Partial theme = compile error.
All tokens required. Switching theme triggers full re-render.
**Affects**: rosace-theme

---

## OBSERVABILITY

### D039 — Tracing System
**Status**: LOCKED
**Decision**: RosaceTrace enum, TracingBus, zero cost in production.
All systems emit traces. No system merges without traces.
Subscribers: RingBuffer, DevTools, File, Console, IDE.
Protocol: serde, versioned, language-agnostic.
**Affects**: rosace-trace, ALL crates

---

### D040 — Dev Tools Transport
**Status**: LOCKED
**Decision**: Native = shared memory + Unix socket. WASM = WebSocket.
Same protocol both ways. Dev tools = separate process.
MessagePack serialization.
**Affects**: rosace-trace, rosace-devtools

---

### D041 — Hot Reload Limits
**Status**: LOCKED
**Decision**:
Can reload: build() logic, styles, handlers, atom defaults, strings
Needs restart: new deps, atom type change, new files, FFI changes, macro changes
On limit: auto full rebuild, clear message, no silent failure
**Affects**: rosace-devtools, rosace-cli

---

## PLATFORM

### D042 — App Lifecycle
**Status**: LOCKED — IMPLEMENTED 2026-07-14 by D110/Phase 29 Step 1.
**Decision**: GlobalAtom<LifecycleState> + use_app_lifecycle() hook.
States: Active, Inactive, Background, Suspended.
**Affects**: rosace-platform — superseded by D110's resolution: the real
home is `rosace-core/src/app_lifecycle.rs` (rosace-platform is
unreachable from component code; see D110/PHASE_29.md Step 1).

---

### D043 — Permissions
**Status**: LOCKED
**Decision**: Unified async API. Permission::camera().rationale("...").request().await
PermissionStatus: Granted | Denied | PermanentlyDenied
**Affects**: rosace-platform

---

### D044 — Localization
**Status**: LOCKED
**Decision**: Day 1. TOML format. use_locale() hook. LOCALE.set() triggers full re-render + RTL.
**Affects**: rosace-theme, rosace-layout

---

### D045 — Haptics
**Status**: LOCKED
**Decision**: Semantic API. Haptic::light/medium/heavy/success/warning/error/selection()
Desktop/WASM = silent no-op.
**Affects**: rosace-platform

---

### D046 — Safe Areas
**Status**: LOCKED
**Decision**: Edge to edge by default. Scaffold handles automatically.
Padding::safe_area() for manual. .ignore_safe_area(true) for full bleed.
**Affects**: rosace-platform, rosace-layout

---

### D047 — Minimum OS Versions
**Status**: LOCKED
**Decision**: iOS 16+, Android API 24+, macOS 12+, Windows 10 1903+, Ubuntu 20.04+
Web: Chrome 90+, Firefox 90+, Safari 15+
**Affects**: rosace-platform

---

## FFI

### D048 — FFI Bridges
**Status**: LOCKED
**Decision**: #[rosace_ffi(c|swift|kotlin|js)] macros. Safe wrappers auto-generated.
All return RosaceResult. catch_unwind at every boundary.
**Affects**: rosace-ffi

---

### D049 — Synchronous Bridge
**Status**: LOCKED
**Decision**: sync_bridge::call<T>() for zero-serialization sync calls. SharedMemory for hot path.
**Affects**: rosace-ffi

---

### D050 — FFI Memory Ownership
**Status**: LOCKED
**Decision**: Rust allocates → Rust frees. C allocates → C frees via ForeignBox.
Ownership transfer explicit. Never cross ownership silently.
**Affects**: rosace-ffi

---

## CONCURRENCY

### D051 — Concurrency Model
**Status**: LOCKED
**Decision**: Single UI thread + Tokio async runtime + Rayon worker pool.
Atoms only written from UI thread. Workers communicate via channels.
**Affects**: rosace-core, rosace-state

---

## CLI

### D052 — CLI Name
**Status**: LOCKED
**Decision**: rsc
**Commands**: rsc dev, rsc build, rsc test, rsc analyze, rsc snapshot

---

### D104 — Two project/packaging styles: managed + bare
**Status**: PLANNED (build AFTER each platform runs an MVP — user priority: platforms working first)
**Decision**: `rsc` offers two project styles, like the Expo↔React-Native / Flutter split:
- **Managed (Expo-like, default)** — platform harnesses are hidden/generated on demand; the developer stays in Rust and runs one command (`rsc run ios|android|web|desktop`, `rsc dev --target …`). `rsc` owns the generated iOS/Android/web scaffolding in a cache/gen dir; the app repo stays clean.
- **Bare / eject (Flutter-like)** — `rsc eject` (or `rsc add-platform ios`) materializes real, editable native project folders (`ios/`, `android/`, `web/`) in the repo so developers can drop to Xcode / Android Studio / native config for platform-specific work. Once ejected, `rsc` drives those folders instead of regenerating.
**Foundation**: the per-platform build+run+harness machinery (targets, NDK/cargo-ndk, Xcode project, Gradle project, wasm-bindgen glue) is the same underneath both styles — managed hides it in gen/, bare exposes it in-repo. Decide the harness generator (cargo-mobile2 vs hand-rolled) as part of platform bring-up.
**Reason**: Beginners want zero-config one-command runs (managed); serious apps eventually need native escape hatches (bare). Supporting both, with a clean eject, is the proven model.
**Affects**: `rosace-cli`, project templates
**Relates to**: D102/D103 (hot-reload transports per platform), D051/D052 (CLI)

---

## TESTING

### D053 — Golden Files
**Status**: LOCKED
**Decision**: Per-platform golden files. tests/goldens/desktop|mobile|web/
Threshold: 0%=pass, <1%=warn, >1%=fail. Configurable per test.
**Affects**: rosace-test

---

### D054 — VSync Frame Scheduling
**Status**: LOCKED
**Decision**: `ControlFlow::Wait` + `EventLoopProxy<FrameRequest>`. `rosace-state`
holds a global `OnceLock<Box<dyn Fn() + Send + Sync>>` wakeup fn. `Atom::set()`
calls `request_frame()` which sets an `AtomicBool` and invokes the wakeup fn.
Platform registers the proxy at startup. `AboutToWait` + `user_event` both call
`window.request_redraw()` when the flag is set. App idles at 0% CPU.
**Affects**: rosace-state, rosace-platform

---

### D055 — Key Mechanism
**Status**: LOCKED
**Decision**: `Key(u64)` newtype. `impl From<&str>` and `impl From<u64>` via
FNV hash. `Element::key: Option<Key>`. Reconciler matches keyed siblings by key
before falling back to position-based matching. No cross-tree key uniqueness
requirement — keys are local to their parent's child list.
**Affects**: rosace-core, rosace (reconciler)

---

### D056 — LayoutCtx
**Status**: LOCKED
**Decision**: `Widget::layout` changes from `(constraints: Constraints) -> Size`
to `(ctx: &LayoutCtx) -> Size` where `LayoutCtx { constraints, font, theme }`.
Font access in layout allows accurate glyph-metric-based text measurement.
`LayoutCtx::with_constraints(c)` creates a child context with tighter constraints.
**Affects**: rosace-widgets

---

### D059 — Animation VSync Integration
**Status**: LOCKED
**Decision**: `rosace-animate` owns a global frame-delta clock:
`static FRAME_DT: AtomicU32` (f32 bits stored as u32). Platform writes the
real elapsed time via `rosace_animate::set_frame_dt(dt)` at the start of
every `RedrawRequested` event, before the render pass. All animation hooks
(`use_spring`, `use_animation`) read `frame_dt()` — never hardcode a timestep.
`dt` is clamped to `[0.001, 0.1]` seconds to survive tab-out / system sleep.
Platform adds `rosace-animate` as a dependency. No registry, no callbacks —
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
**Affects**: rosace-animate, rosace-platform

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
**Affects**: rosace-widgets

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
**Affects**: rosace-widgets, rosace (App::launch render loop)

---

### D060 — Focus System
**Status**: LOCKED
**Decision**: `FocusManager` (already in `rosace-a11y`) is extended to be
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
**Affects**: rosace-a11y, rosace-widgets, rosace (render loop)

---

### D061 — Navigation Route Stack
**Status**: LOCKED
**Decision**: `Navigator` in `rosace-nav` manages a `Vec<Route>`. Only the
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
via `rosace_state::clear_component()`), unfreezes and re-activates the route below.

Routes are **not** overlay entries. They replace the screen. Overlays sit above
the active route's render output. The navigator is orthogonal to the overlay stack.

A frozen route's atoms retain their values. Scroll positions, text inputs, and
all component state survive navigation round-trips. State is only cleared on
explicit pop (not on freeze).
**Reason**: Back-navigation should feel instant — no rebuild, no lost state.
Frozen routes cost memory but zero CPU. Clear separation from overlays prevents
the two systems from coupling.
**Affects**: rosace-nav, rosace (render loop)

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
**Affects**: rosace-widgets

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
**Affects**: rosace-a11y, rosace-widgets (Phase 14)

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
**Affects**: rosace-widgets, rosace-a11y, rosace (render loop)

---

### D065 — Persistent RenderNode Tree
**Status**: LOCKED
**Question**: How do we avoid full-tree layout + paint every frame?
**Decision**: Each native widget node in the tree is backed by a `RenderNode` that persists across frames. It caches `(last_constraints, cached_size, cached_picture, cached_rect, paint_dirty)`. On each frame, the reconciler diffs the new element tree against the existing RenderNode tree. Clean nodes (unchanged constraints + not paint_dirty) skip layout and reuse their cached Picture. Dirty nodes re-layout + re-paint and update their cache.
**Reason**: Full-tree re-layout + re-paint on every atom change wastes CPU. Caching at the widget granularity gives surgical updates without requiring immutable widget props (we can always mark dirty when unsure).
**Affects**: `rosace` (umbrella — reconciler + render loop), `rosace-render` (Picture must be Clone)

---

### D066 — Reconciler Algorithm
**Status**: LOCKED
**Question**: How does the reconciler match new elements to existing RenderNodes?
**Decision**: DFS by position within sibling list. For each position: if `new.tag == old.tag` AND keys agree (both absent OR both present with same value) → stable node, inherit cache; else → replace (new node, paint_dirty=true). Keyed children within the same parent are matched by key first, then unkeyed by position. A mismatch always creates a fresh RenderNode (forces re-layout + re-paint).
**Reason**: Type+position matching is O(n) DFS and handles the common case (stable tree). Key matching handles reordered lists without losing state.
**Affects**: `rosace` (umbrella — reconciler)

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
**Affects**: `rosace` (umbrella — render loop)

---

### D068 — O(depth) Hit Testing
**Status**: LOCKED
**Question**: How do we replace the flat linear hit-target scan?
**Decision**: Walk the RenderNode tree depth-first, visiting children before parent (post-order). For each node, check `cached_rect.contains(pointer)` and presence of `hit_handlers`. The first matching node wins. Overlay entries are checked first (top-to-bottom in insertion order) before the main tree.
**Reason**: A DFS walk is O(depth × branching_factor) rather than O(n). For typical widget trees (depth ≈ 20, branching ≈ 4) this is ~80 checks vs hundreds. Deepest-child-first mirrors visual stacking — the frontmost widget wins.
**Affects**: `rosace` (umbrella — hit test)

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
**Affects**: `rosace` (umbrella), `rosace-a11y`, `rosace-widgets`

---

### D070 — Navigation Route Stack Wiring
**Status**: LOCKED
**Question**: How does `rosace-nav` (Navigator, RouteStack) integrate with the App render loop?
**Decision**: `Navigator` is a root-level component that holds a `Vec<Route>` in app state. Each `Route` wraps a `Box<dyn Component>`. `Navigator::build()` renders only the top route. Frozen routes are held in memory but not rebuilt.

`push_route(component)` → creates a new Route entry, pushes to Vec, triggers rebuild.
`pop_route()` → drops top route, fires `on_unmount`, clears atom state via `clear_component()`, triggers rebuild.

Navigator stores the route stack in an `Atom<Vec<Arc<RouteEntry>>>`. Changes to this atom trigger a frame. Routes below the top are not walked by `walk_element` (frozen = invisible to layout, paint, and hit test).

Integration: `rosace-nav` already has the `stack.rs` stub. Phase 14 fills it in and adds `Navigator` as a first-class component in the prelude.
**Reason**: D061 spec is fully described; this decision locks the wiring to the render loop. Route freezing happens at the element-walk level — non-top routes are simply not walked.
**Affects**: `rosace-nav`, `rosace` (umbrella), `rosace::prelude`

---

### D071 — RepaintBoundary Widget
**Status**: LOCKED
**Question**: How should isolated PictureLayer caching be exposed to users?
**Decision**: `RepaintBoundary::new(child)` — a wrapper widget that maintains its own isolated `PictureRecorder`. On each paint pass, if the child's `paint_dirty` flag is false AND the boundary's cached Picture exists, the boundary replays only its own cached Picture into the parent recorder — zero widget.paint() calls inside the boundary.

`RepaintBoundary` forces a subtree boundary in the RenderNode tree. Any `Atom` write that touches a widget inside a `RepaintBoundary` only invalidates that boundary's Picture, not sibling boundaries.

Implementation: `RepaintBoundary` is a `NativeElement` with tag `"RepaintBoundary"`. Its `RenderNode` stores `own_picture: Option<Arc<Picture>>` in addition to the normal fields. In `walk_element`, when the current native element has tag `"RepaintBoundary"` and `!paint_dirty`, it replays `own_picture` directly.
**Reason**: Phase 13 caches Pictures per native widget position. RepaintBoundary formalizes the concept — a child subtree with its own isolated Picture whose invalidation is independent of siblings.
**Affects**: `rosace-widgets`, `rosace` (umbrella)

---

### D072 — GPU Backend Choice
**Status**: LOCKED
**Question**: Which GPU API should the compositor target?
**Decision**: **wgpu** (not raw Vulkan/Metal/DX12). wgpu selects the best native backend per OS at runtime (Metal on macOS, Vulkan/DX12 on Windows/Linux). Pure-Rust API, no C++ toolchain required. D032 is unaffected — tiny-skia remains the CPU rasterizer; wgpu is the display backend only. The swap is isolated to `rosace-compositor` + `rosace-platform`.
**Reason**: GPU blit via wgpu enables 120fps and future multi-layer GPU compositing without a full CPU readback per frame.
**Affects**: `rosace-compositor` (new crate), `rosace-platform`

---

### D073 — GPU Texture Pixel Format
**Status**: LOCKED
**Question**: What pixel format is used for the CPU→GPU upload?
**Decision**: Upload as `Rgba8Unorm`; the WGSL shader reads it directly. tiny-skia produces RGBA8. The wgpu surface format is queried from the adapter at init time and the compositor matches it — no manual format detection needed.
**Reason**: `Rgba8Unorm` is universally supported and matches tiny-skia's byte order directly.
**Affects**: `rosace-compositor`

---

### D074 — Compositor Architecture
**Status**: LOCKED
**Question**: Where does wgpu initialization live and how does it integrate with rosace-platform?
**Decision**: Standalone `rosace-compositor` crate exports `GpuPresenter`. `rosace-platform` depends on it and initializes `GpuPresenter` in `AppState::resumed()`. If wgpu init fails, `presenter = None` and the softbuffer fallback path activates silently. No feature flag — the GPU path is always attempted.
**Reason**: Keeps wgpu entirely out of the widget/render crates. Softbuffer fallback prevents crashes on CI/headless environments.
**Affects**: `rosace-compositor` (new), `rosace-platform`

---

### D075 — Compositor Shader
**Status**: LOCKED
**Question**: What is the compositor's render pipeline?
**Decision**: Minimal WGSL fullscreen-quad shader. Vertex shader generates 6 vertices from `vertex_index` (two triangles, no vertex buffer). Fragment shader samples the uploaded frame texture with nearest-neighbour filtering (pixels are already at physical resolution — no upscaling needed). No mipmaps, no sRGB correction (tiny-skia already handles gamma).
**Reason**: Minimum viable GPU blit. No vertex buffers, no index buffers, no uniform buffers. A single bind group with texture + sampler is all that's needed.
**Affects**: `rosace-compositor`

---

### D076 — Layer Compositing Model
**Status**: LOCKED
**Question**: How should multiple render layers be composited?
**Decision**: Each logical layer (base, overlay) is a separate `SkiaCanvas`. Each canvas produces its own RGBA pixel buffer. `GpuPresenter::present_layers(&[CompositorLayer])` uploads N textures and composites them bottom-to-top via `SRC_ALPHA` over `ONE_MINUS_SRC_ALPHA` in two sequential render passes.
**Reason**: Isolates base/overlay rendering so overlay changes (dialog show/hide) do not force a base layer CPU re-render. Foundation for per-layer opacity and transform in Phase 17.
**Affects**: `rosace-compositor`, `rosace/src/lib.rs`, `rosace-render`

---

### D077 — CompositorLayer Struct
**Status**: LOCKED
**Question**: What is the interface between the render loop and the GPU compositor for multi-layer presentation?
**Decision**: `pub struct CompositorLayer<'a> { pixels: &'a [u8], width: u32, height: u32, opacity: f32 }`. `GpuPresenter::present_layers(&[CompositorLayer])` composites them. The old `present()` is kept as a shim for backward compatibility.
**Reason**: Minimal struct — only what's needed for Phase 16. `opacity` is per-layer, applied as a scalar to the source alpha before blending.
**Affects**: `rosace-compositor`

---

### D078 — Overlay Canvas Clear
**Status**: LOCKED
**Question**: How is the overlay canvas initialized each frame?
**Decision**: `SkiaCanvas::clear_transparent()` fills the pixmap with RGBA(0,0,0,0) before each overlay paint pass. Transparent pixels in the overlay texture pass through to the base layer via the blend equation.
**Reason**: Ensures overlay content from the previous frame does not persist when overlays are closed or repositioned.
**Affects**: `rosace-render`, `rosace/src/lib.rs`

---

### D079 — Multi-Layer WGSL Shader
**Status**: LOCKED
**Question**: How does the compositor shader blend N layers?
**Decision**: Two sequential render passes on the same surface target. Pass 1: blit base texture with `REPLACE` blend (opaque). Pass 2: blit overlay texture with `SRC_ALPHA` / `ONE_MINUS_SRC_ALPHA` blend (alpha-over). The overlay pipeline uses `blend: Some(wgpu::BlendState::ALPHA_BLENDING)`. Both passes use the same fullscreen-quad vertex shader.
**Reason**: Two-pass avoids binding limitations and works on all wgpu backends. Fragment output is `base_color * (1 − overlay.a) + overlay_color * overlay.a` — the standard Porter-Duff over operation.
**Affects**: `rosace-compositor`

---

### D080 — TransformLayer Model
**Status**: LOCKED
**Question**: How does TransformLayer capture content and apply GPU-side scroll?
**Decision**: `TransformLayer<W>` wraps a child widget. `layout()` reports the viewport size. `paint()` shifts the child origin by `-scroll_y` (and `-scroll_x`) so content scrolls within the viewport. `CompositorLayer.offset` carries the UV-space scroll offset to the GPU; the shader returns transparent for out-of-range UV. Phase 17 uses CPU shift; Phase 18 adds full GPU-texture-per-scroll.
**Reason**: Establishes the widget API and GPU offset pipeline. Phase 18 replaces the CPU shift with a frozen-texture + uniform-only update path.
**Affects**: `rosace-widgets`, `rosace-compositor`

---

### D081 — Transform Uniform Buffer
**Status**: LOCKED
**Question**: How is the scroll offset passed to the WGSL shader?
**Decision**: A `wgpu::Buffer` with `UNIFORM | COPY_DST` usage holds `[f32; 4]` = `[offset_x, offset_y, 0.0, 0.0]`. The shader reads `@group(0) @binding(2) var<uniform> u_layer: LayerUniforms`. UV is shifted by `u_layer.offset`; out-of-range returns `vec4(0)` (transparent). Buffer is created via `create_buffer_init` each frame in Phase 17; Phase 18 will reuse a persistent buffer and `queue.write_buffer`.
**Reason**: Minimal change to bind group layout — add one uniform binding. No vertex buffer changes. All existing layers pass `(0.0, 0.0)` offset with zero overhead.
**Affects**: `rosace-compositor`

---

### D082 — TransformLayer Size Limit
**Status**: LOCKED
**Decision**: Phase 17 caps at `MAX_TRANSFORM_DIM = 4096` physical pixels. Content exceeding this falls back to CPU clip scroll (unchanged behaviour). Cap is checked at capture time; a debug warning is emitted.
**Affects**: `rosace-widgets`

---

### D083 — ScrollView Integration
**Status**: DEFERRED → Phase 18
**Decision**: Phase 17 provides `TransformLayer<W>` as a first-class widget. Phase 18 integrates it into `ScrollView` transparently. Users can use `TransformLayer` directly in Phase 17.
**Affects**: `rosace-widgets`

---

### D084 — ScrollView::live reactive constructor
**Status**: SUPERSEDED by D101 — `ScrollView::live(child, atom)` no longer exists. D101 replaced the atom-passing model with implicit per-node scroll state: every scrollable owns a `ScrollController` automatically (zero wiring), and the constructor is plain `ScrollView::new(child)` — no atom parameter. Kept here for history only; later docs/decisions that say "`ScrollView::live`" are using it as informal shorthand for "the default interactive scroll view," which is `ScrollView::new` in the actual API.
**Decision** (historical, as originally locked): `ScrollView::live(child, atom: Atom<f32>)` is a second constructor that stores the atom. In `paint()`, if `live_offset` is `Some`, the atom value overrides the static `offset` field. The owning component subscribed to the atom via `ctx.state()`; when the atom changes the component rebuilds and `paint` reads the new offset. The static `ScrollView::new` + `.offset(n)` path is unchanged for snapshot scenarios.
**Reason**: Reactive scrolling without gesture infrastructure. A button click writes the atom; the rebuild renders the new offset.
**Affects**: `rosace-widgets`

---

### D085 — N-layer compositor
**Status**: LOCKED
**Decision**: `present_layers` iterates an arbitrary `&[CompositorLayer]` slice — no hard cap on layer count. Each layer gets its own texture upload + render pass. Performance is O(N) render passes. Phase 19 will batch into a texture atlas.
**Affects**: `rosace-compositor`

---

### D086 — TransformLayer render-tree discovery deferred to Phase 19
**Status**: DEFERRED
**Decision**: Phase 18 does not add `PaintCtx.transform_layers`. TransformLayer uses the CPU-shift model from Phase 17. Full frozen-texture per layer (separate canvas capture, persistent GPU texture, uniform-only update) is Phase 19.
**Reason**: Adding `PaintCtx.transform_layers` requires platform changes to allocate canvases before the render walk. Phase 18 ships the reactive ScrollView win without that complexity.
**Affects**: `rosace-widgets`, `rosace-platform`

---

### D087 — TransformLayerEntry in PaintCtx
**Status**: LOCKED
**Decision**: `PaintCtx` carries `transform_entries: Rc<RefCell<Vec<TransformLayerEntry>>>`. A `TransformLayerEntry` holds: `picture: Picture` (recorded child), `child_size: Size`, `viewport_rect: Rect`, `scroll_x: f32`, `scroll_y: f32`. `TransformLayer::paint()` records child into a sub-`PictureRecorder`, finishes it, and pushes the entry. It does NOT emit to the main recorder. The `transform_entries` vec is shared (Rc-cloned) through `child()` like `hit_targets`.
**Affects**: `rosace-widgets`, `rosace-render`

---

### D088 — Platform TransformLayer replay (D088)
**Status**: LOCKED
**Decision**: After the main paint pass, `rosace/src/lib.rs` iterates `transform_entries`. For each entry, it translates all DrawCommands by `(viewport.origin - scroll_offset)` using the new `DrawCommand::offset(dx, dy)` method, finishes a temporary PictureRecorder, and replays it onto the base canvas. This gives correct scroll positioning without a separate GPU layer per TransformLayer (that's Phase 20).
**Affects**: `rosace`, `rosace-render`

---

### D089 — GPU texture caching
**Status**: LANDED (Phase 20 Step 6)
**Decision**: Full "zero re-upload on scroll" (persistent wgpu Texture keyed by layer, reused across frames) is Phase 20. Phase 19 re-plays the Picture each frame into a new pixel buffer. The architecture is correct; the caching layer is an optimization.
**Implementation**: `GpuPresenter` holds `cached_layers: Vec<CachedLayer>` (persistent texture + bind group + uniform buffer per slot). `CompositorLayer::dirty` drives it: clean layers reuse their texture (no `write_texture`), offset changes are a `write_buffer`, and a frame where every layer is clean and unmoved skips the present entirely. Dirtiness flows `SkiaCanvas::frame_dirty` (set by the frame loop on repaint) → `take_frame_dirty` in the platform → `CompositorLayer::tracked`. Verified: idle/hover frames upload nothing.
**Affects**: `rosace-compositor`, `rosace-render`, `rosace-platform`, `rosace`

---

### D090 — ScrollView integration with TransformLayer
**Status**: COMPLETE (Phase 20)
**Decision**: Phase 19 provides `TransformLayer<W>` as a direct-use widget. Phase 20 integrates it into `ScrollView::new` transparently.
**Implementation (foundation)**: The compositor now supports *placed* layers — `CompositorLayer::placed(pixels, w, h, dest, src_offset, dirty)` positions a quad at a screen-space `dest` rect (physical px) and samples a content-sized texture at `src_offset` (the shader maps `uv_min + corner*uv_span`, out-of-range UV → transparent for viewport/content clipping). The frame loop renders each `TransformLayerEntry` once into its own `SkiaCanvas` and publishes it via the `rosace-platform::scroll_layer` thread-local registry; the platform composites `base + scroll layers + overlay`, retaining the scroll set across clean frames. Composes with D089 (a clean scroll layer skips re-upload; an offset-only change is a uniform write). Verified via app_demo "GPU Scroll Layer".
**Zero-repaint scroll (LANDED, commit c0baffc)**: a placed layer's scroll offset lives in a non-reactive channel `rosace_state::scroll_offset` keyed by render-tree node id. Updating it (`scroll_offset_by`) requests a present-only frame but dirties NO component — so the frame skips build/paint and the platform reads the channel at present as the layer's UV `src_offset`. `TransformLayer` registers a wheel scroll target feeding the channel; the content texture is reused (D089 skips the upload) and only the offset uniform changes. Verified: 92 consecutive scroll frames `needs_paint=false` + `present 2 layers (0 dirty)`. Meets the exit criterion "scroll produces no CPU paint."
**Hit-testing through the offset (LANDED, commit 4b7e159)**: the dispatch walk (`hit_test`/`hover_test`/`long_press` in `render_tree.rs`) maps screen→content coords when descending into a transform node's children (`child_coords`: subtract viewport origin, add `rosace_state::scroll_offset(node_id)`) and clips to the viewport. GPU-composited scroll content is now interactive. Unit-tested.
**ScrollView::gpu (LANDED, commit cc1a243)**: `ScrollView` gained an opt-in GPU-layer path (`::gpu` / `.gpu_layer()`) — records content into its own picture at `(0,0)`, attaches a transform entry, wheel→channel, scrollbar from the channel offset. `::new`/fixed/controlled keep the base-canvas path (zero regression). The full placed-layer scroll mechanism is now usable through `ScrollView`.
**Transparent default + drag fix (LANDED, commit 144d062)**: `ScrollView::new` now auto-composites as a GPU layer via `should_auto_gpu` — enabled once content overflows the viewport on the scroll axis and fits within `MAX_TL_DIM` (4096px, a real constant); otherwise stays on the base path automatically. Also fixed a real bug found in the process: positional-drag (`hits_at`) callbacks inside a transform were remapped to content coordinates only at the initial hit — every subsequent drag-continuation `MouseMove` (streamed straight to the stored callback, no re-hit-test) received raw screen coordinates instead. Fixed by wrapping the callback at the transform-hosting node so it re-applies the remap on every invocation, not just the first. Unit-tested.
**MAX_TL_DIM resolved (commit 5d3500b)**: decided NOT to build GPU-layer re-render windowing for content beyond 4096px. `ListView::builder` already solves "content too large for one texture" for the case that matters (long lists) via real virtualization — no texture-size limit possible regardless of count, since off-screen content is never materialized. The remaining case (one large non-virtualized widget past 4096px) is already handled correctly by the existing base-path fallback. Documented in both widgets' doc comments.
**Affects**: `rosace-compositor`, `rosace-platform`, `rosace`, `rosace-widgets`

---

### D091 — RenderTree owns all per-node retained state
**Status**: LOCKED
**Decision**: The persistent render tree (`RenderNode`) is the single owner of everything a widget produces that must outlive one frame: layout cache, cached Picture, hit regions, scroll regions, focus nodes, overlay attachments, and transform layers. `paint()` becomes side-effect free with respect to the frame: it records commands and *declares* regions/attachments onto its own RenderNode. The frame pipeline then derives the display list, hit-test order, focus cycle, overlay stack, and compositor layers from the tree — nothing is re-emitted per frame through `Rc<RefCell<Vec>>` side channels or thread-locals.
**Reason**: Three independent bugs came from the same disease: state produced only during `paint()` dies on cache-hit frames (hit handlers → fixed a1e91b8; TransformLayerEntries → D088 cache; overlay entries → cached_overlay_entries). Each got its own bolt-on cache. D091 makes the bug class unrepresentable and is the foundation for damage-rect repainting and real RepaintBoundary caching. The existing keyed reconciler (`rosace/src/reconcile.rs`) becomes the actual tree-update mechanism (it is currently dead code — `walk_element` inlines its own tag matching).
**Affects**: `rosace`, `rosace-widgets`, `rosace-a11y`

---

### D092 — Tree-walk hit testing with structural z-order
**Status**: LOCKED
**Decision**: Input dispatch walks the RenderTree back-to-front (overlay roots first, then main root; within a node, children in reverse paint order) instead of scanning a flat `Vec<HitTarget>` with insert-at-0 ordering tricks. A node can consume, pass through, or transform events (scroll offset translation). Scrims become ordinary nodes that consume misses — replacing the four-strip hit-rect workaround. Z-order is structural, not an artifact of registration order.
**Affects**: `rosace`, `rosace-widgets`, `rosace-gesture`

---

### D093 — Constructor Law
**Status**: LOCKED
**Decision**: `Widget::new()` takes exactly the required content — content leaves take their content (`Text::new(str)`), required-child wrappers take the child (`Card::new(child)`), optional-child and multi-child widgets take nothing (`Container::new()`, `Column::new()`). Everything optional is a builder method. Never two required positional args of the same type. Named constructors are shortcuts, never replacements for `new()`. Full spec: `.steering/API_DESIGN.md` §1.
**Affects**: rosace-widgets (all), rosace-examples

---

### D094 — Property Vocabulary
**Status**: LOCKED
**Decision**: One builder name per concept across all widgets: `.background()` = surface fill (never `.color()`/`.bg()`), `.color()` = content/foreground only, `.border(color, width)`, `.radius()`, `.elevation()`, `.padding(EdgeInsets)`, `.width/.height/.size`, `.spacing()`, `.align(Alignment)`, `.on_press()`, `.on_change()`, `.disabled()`. Table in API_DESIGN.md §3 is normative.
**Affects**: rosace-widgets (all)

---

### D095 — Widget Consolidation: one box
**Status**: LOCKED
**Decision**: `Container` is the single box widget. `ColoredBox`, `SizedBox`, `Padding`, `Center` are removed (migration table in API_DESIGN.md §5). `Card` survives only as a themed Container preset. The element-based widget structs in `rosace-layout` are removed; that crate keeps only layout math. New-widget bar: must draw or lay out something new — presets are named constructors, not widgets.
**Reason**: Six widgets did one widget's job; learning curve scales with rules × widgets.
**Affects**: rosace-widgets, rosace-layout, rosace-examples

---

### D096 — Widget styling = builder chain
**Status**: LOCKED
**Decision**: Builder-chain styling (`Text::new("hi").size(20.0).weight(Bold)`) is the primary API. Style-struct arguments are rejected (Rust lacks named/optional args → `..Default::default()` noise at every call site). Reusable styles come later as a single additive `.style(TextStyle)` method bridging to rosace-style — deferred to style-system integration.
**Affects**: rosace-widgets, rosace-style

---

### D097 — Canonical scroll + navigation APIs
**Status**: LOCKED
**Decision**: `ScrollView::new(child, atom)` is live by default; static mode renamed `ScrollView::fixed(child, offset)`; `Column::scrollable(atom)`/`Row::scrollable(atom)` as planned sugar (Expanded is ignored on an unbounded scroll axis). `ScreenNav<R>` is the one routing API; `Navigator`/`Route`/history/guards and nav-anim's Navigator become internal machinery, removed from prelude. `AppBar::back_button(&nav)` replaces the manual can_pop/leading boilerplate.
**Affects**: rosace-widgets, rosace-nav, rosace-nav-anim

---

### D098 — Two-concept model + taxonomy by defaults
**Status**: LOCKED
**Decision**: Users learn exactly two concepts: `Component` (reactive — *what* to show) and `Widget` (primitive protocol — *how* to size/draw/behave). `Element` and the render tree are internal. The Leaf/SingleChild/MultiChild taxonomy is NOT three traits (blanket `impl Widget` for multiple taxonomy traits violates Rust coherence); it is one `Widget` trait with a `children() -> Children` accessor (`None`/`One`/`Many`) and smart defaults keyed off it — the taxonomy is which defaults you keep. Full spec: `.steering/WIDGET_PROTOCOL.md`.
**Affects**: rosace-widgets, rosace-core, docs

---

### D099 — Authoring contexts: framework-owned child geometry + declarative semantics
**Status**: LOCKED
**Decision**: `LayoutCx` gains `layout_child(i, constraints)` (framework-memoized on the render tree) and `position_child(i, point)` (stored); `PaintCx` gains `paint_child(i)` reading stored positions. Per-widget measure caches are deleted; measure/paint drift becomes unrepresentable; per-child picture caching and damage rects (Phase 20 Steps 1/5) get their tree from this. `semantics(&self, cx)` declares role/label/actions onto the widget's render-tree node (single-owner, D091) — activates the dormant SemanticNode/Role types (D035/D064).
**Affects**: rosace-widgets, rosace, rosace-a11y

---

### D100 — CustomPaint is a recorded Leaf (amends D034)
**Status**: LOCKED — supersedes D034's "full SkiaCanvas access" wording
**Decision**: `CustomPaint::new(|cx, size| ...).repaint_when(atom)` — a Leaf widget whose closure records DrawCommands. No direct pixel/canvas access at paint time (would bypass the retained pipeline — the D091 vanishing-state bug class). Pixel-level needs use `DrawCommand::BlitRgba`. Hit testing via the standard protocol.
**Affects**: rosace-render, rosace-widgets

---

### D101 — Default scroll controllers on the render tree
**Status**: LOCKED
**Decision**: Every scrollable widget scrolls by default with zero wiring: its render-tree node lazily owns a `ScrollController` (persistent per-node state, NOT cleared on repaint — like Flutter's implicit ScrollPosition). `PaintCtx` carries the owning `ComponentId`; node-created controllers subscribe it so writes dirty the component (the no-subscriber trap proven by the b37d9e0 bug). APIs: `ScrollView::new(child)` / `::horizontal(child)` / `Column::scrollable()` / `Row::scrollable()` / `ListView::builder(count, extent, f)` take no scroll state; `.controller(ctrl)` / `ScrollView::controlled(child, ctrl)` opt into programmatic control; raw scroll atoms are removed from the public API. `ScrollView::fixed(child)` remains the inert snapshot mode.
**Reason**: "Create an atom in build() and thread it down" was boilerplate on every scrollable and a footgun (forgotten atom = broken scroll). The OOP 'scrollables always have a controller' model, translated to Rust as per-node retained state (D091's home) + composition.
**Affects**: rosace-widgets, rosace-scroll, rosace, rosace-examples

---

### D102 — Hot-reload architecture: stable host + reloadable UI modules
**Status**: PLANNED (full design in `.steering/HOT_RELOAD.md`)
**Problem**: `rsc dev` today only re-runs `cargo build` on change — the running process is never updated (the app is one monolithic static binary, so there is no seam to swap). Hot reload does not work.
**Three layered tiers (revised — read first)**: (0) **Hot restart with state preservation** — always available on every platform (floor). (1) **Template/data hot-reload** — the UNIVERSAL primary path: ships *data not code*, so it reloads structure/style edits instantly on EVERY target incl. iOS device + web (which cannot load new code). Requires the declarative `view!` layer (**D103**). (2) **Native dylib module swap** — an ACCELERATOR for *logic* changes on platforms that permit `dlopen` (desktop, Android dev, iOS simulator); this is the host/module architecture below. Priority: build Tier 0 + Tier 1 first (universal), Tier 2 second (accelerator, not a prerequisite). Full design + capability matrix in `.steering/HOT_RELOAD.md`.
**Decision (Tier 2 — host/module split)**: Split the dev process into a **stable HOST** (winit loop, renderer, reconciler/frame loop, render tree, atom **state store**, reload supervisor) and **reloadable UI MODULE dylibs** (`Component::build()`, handlers, styles — one dylib per UI crate). Both host and modules dynamically link `rosace` compiled **once** as `crate-type=["dylib"]`, so there is a **single instance** of the state-store statics (state survives reload and flows between modules — the make-or-break detail). Modules expose one versioned `extern "C"` entrypoint (`__tzr_module_vN`, generated by `#[rsc::module]`); the host loads them via `libloading`. Reload is strictly ordered to avoid dangling closures: **load new → global rebuild (fresh closures) → swap → drop old tree (frees old `Arc`s) → only then unload old lib**; each tree generation refcounts the `Library` that produced it. All of this is **dev-only, gated by the `rsc-hot` cargo feature**; `rsc build`/`package` static-link one monolithic binary (dylibs never ship). `rsc dev` becomes the supervisor: watch → map file→crate (`cargo metadata`) → `cargo build -p <crate>` → signal reload over IPC; reload only the changed module.
**Failure/limits (extends D041)**: entrypoint call + first post-reload frame run in `catch_unwind`; on failure **revert** to the last-good module ("kept previous version"); if unrecoverable or the change hits a D041 "needs restart" limit (new deps, atom *type* change, new files, FFI/macro changes) → **hot restart**: serialize `#[persist]` atoms (D008) → `exec` fresh process → rehydrate.
**Web**: wasm has no stable dynamic linking, so module load/unload does NOT port. Same UX via **Tier 1 = hot restart with state preservation** (rebuild wasm → WebSocket `reload` → serialize atoms to sessionStorage → re-instantiate → rehydrate). Tier 2 (per-module wasm via dynamic `import()`) and Tier 3 (RSX/template hot-reload with no recompile) are stretch goals.
**Mobile**: splits by whether the OS permits runtime `dlopen`. **Android** (dev) and **iOS Simulator** support it → full module hot-swap ports (Android: cross-compile `.so` → `adb push` to `codeCacheDir` → `dlopen`, mind W^X/SELinux; Simulator: `dyld` doesn't enforce device signing). **iOS real device** forbids `dlopen` of unsigned dylibs (code-signing + sandbox; no AOT JIT) → dylib swap is impossible; fall back to **RSX data hot-reload for markup/style** (Tier 3, no new code) and **full rebuild+re-sign+redeploy+rehydrate for logic** (Tier 1). All mobile targets need a dev transport + control channel (adb; `devicectl`/`ios-deploy` + socket). Matrix in `.steering/HOT_RELOAD.md`.
**Feasibility**: native module hot-swap is a proven pattern (hot-lib-reloader, Bevy `dynamic_linking`, Dioxus desktop). The three make-or-break problems — statics duplication, dangling closures, no stable ABI — have known fixes (shared dylib singleton, ordered swap, narrow `extern "C"` dev-only). If native swap proves un-robust, fall back to hot-restart-with-preservation everywhere (still far better than today).
**Rollout**: (1) runtime split + shared-dylib singleton, (2) native single-module hot-swap + ordered protocol, (3) multi-module incremental, (4) hot-restart + `#[persist]` rehydrate, (5) web Tier 1, (6) stretch (web Tier 2/3, source-location atom keys).
**Reason**: A great edit→see loop is table stakes for a UI framework. The host/module split is the only way to swap live code in Rust; making the state store a shared-dylib singleton is what lets state survive; the ordered swap protocol is what makes it not crash.
**Affects**: `rosace-cli`, `rosace-hot-reload`, `rosace-devtools`, `rosace-platform`, `rosace-state`, `rosace`, `rosace-macros`
**Depends on / relates to**: D008 (atom persistence levels), D041 (hot-reload limits), D103 (declarative view layer — Tier 1 dependency)

---

### D103 — Declarative `view!` layer for universal template hot-reload
**Status**: PLANNED (enables D102 Tier 1; design in `.steering/HOT_RELOAD.md`)
**Problem**: ROSACE is builder-API only (`Column::new().child(..)`). A chain of method calls cannot be mechanically diffed into a static template + dynamic slots, so it cannot support template/data hot-reload — the ONLY mechanism that reloads on iOS device and web (ships data, not code). Without it, hot reload is dylib-only and therefore worthless on the platforms that most need it.
**Decision**: Add a declarative `view!` macro (Dioxus template model) as the hot-reload authoring surface; the builder API remains the low-level escape hatch. `view!` emits BOTH (a) normal Element-building code (release path, zero overhead) and (b) under `rsc-hot`, a **template descriptor** — the static widget skeleton with indexed **dynamic slots** (the `{expr}` holes), keyed by `location!()`. A runtime **interpreter** rebuilds a subtree from a descriptor via a **widget registry** (element name → factory + attribute setters), re-binding dynamic slots to the already-compiled closures/values by index (never runs new logic → works on wasm + iOS device). A dev watcher re-parses the changed `view!`, diffs the descriptor by location key, and pushes deltas over the control channel; the interpreter swaps the subtree with no recompile.
**Boundary**: template edits may add/remove/reorder/wrap static elements, change literal text/attrs/styles, and move an existing dynamic slot. They may NOT add a new dynamic slot, change a handler body, or add a hook (new compiled code) — detected by a change in the slot signature → escalate to D102 Tier 2 (dylib) or Tier 0 (restart). Covers ~70–80% of everyday edits instantly, on every platform.
**Reason**: Data-driven template reload is the only path that bypasses both the iOS `dlopen` ban and wasm's lack of dynamic linking, because it transports a UI data tree rather than machine code. Making it the primary tier is what makes hot reload universally valuable.
**Affects**: `rosace-macros`, `rosace-widgets`, `rosace`, `rosace-cli`, `rosace-hot-reload`
**Relates to**: D102 (hot-reload architecture — Tier 1), D098–D100 (Widget protocol — the registry/Element model this builds on)

---

### D105 — Platform-adaptive theming: ONE widget set, theme is the only platform authority
**Status**: PLANNED (full plan in `.steering/PHASE_23.md`)
**Problem**: Desktop/iOS/Android chrome genuinely differs (macOS traffic-light inset, iOS centered title + edge-back, Android left title + elevation, Cupertino vs Material switch shapes, scroll physics, touch density). Today ROSACE has one widget set with ad-hoc props (`AppBar.show_traffic_lights`) and **no platform awareness at all** — `ThemeData` carries only global tokens (colors/typography/spacing/radius/animation). We do NOT want two widget libraries (Material + Cupertino) — that doubles maintenance and fights the one-concept ethos.
**Decision**: Keep **ONE widget set**; make the **theme the sole source of platform look** (SwiftUI/Flutter-`ThemeExtension` model, minus the dual-widget-set cost). Four parts:
1. **Platform-keyed theme bundle + fallback.** `Themes::new(fallback).platform(Platform::Ios, ios).platform(Platform::Android, android)` handed to `App::themes(..)`. The **framework resolves the active theme ONCE at startup** from the detected running platform (`themes.get(platform).unwrap_or(fallback)`); widgets read only `ctx.theme` and **never branch on platform** (nothing per-platform to maintain in widget code). The active platform can be forced to preview another platform's look.
2. **Per-widget Style structs in `ThemeData`** (`AppBarStyle { title_align, show_traffic_lights, height, elevation, .. }`, `ButtonStyle`, `SwitchStyle`, …). This is what lets a theme change *structure*, not just color — so "Material theme" and "Cupertino theme" are two different `ThemeData` values with identical widget code. Per-instance widget props still override the theme style.
3. **`ThemeExtension` type-map** (`ThemeData` gains a `HashMap<TypeId, Box<dyn Any>>`; `theme.with_ext(MyStyle{..})` / `ctx.theme.ext::<MyStyle>()`). New/custom/third-party widgets add their own theming WITHOUT editing core `ThemeData` — the "new-widget customization is addable" requirement.
4. **Built-in `material()` / `cupertino()` themes** so `rsc new` can wire an iOS+Android app out of the box (D104).
**Reason**: Platform look varies structurally, but branching in widgets (or shipping a second widget set) is unmaintainable. Pushing all platform variance into data (theme) keeps widgets platform-agnostic, gives maximum per-platform per-widget customization, and stays extensible. Resolving the theme once (framework, not per-widget) keeps widgets dumb readers.
**Rejected**: dual widget set (Flutter Material+Cupertino) — 2× library to build/maintain, rejected explicitly by the user.
**Affects**: `rosace-theme`, `rosace-widgets`, `rosace`, `rosace-platform`, `rosace-cli`
**Relates to**: D104 (packaging — themes ship per platform), the theme-global animation model (D-anim), Widget protocol D098–D100

---

### D106 — Mobile needs a real native host project; winit cannot own the iOS app
**Status**: IN PROGRESS — Steps 1-2 landed 2026-07-08: the native-bridge FFI boundary (`rosace-ffi`) and a real `rsc new`-generated `.xcodeproj` + Swift `AppDelegate`/`SceneDelegate`/`EngineViewController` host, both verified with real `xcodebuild` builds and iOS Simulator runs (see `.steering/PHASE_24.md`). Steps 3-5 (Android Gradle/JNI generation, `rsc run` toolchain integration, capability proof) remain. Full plan in `.steering/PHASE_24.md`.
**Problem**: This session's iOS/Android bring-up used a "hand-rolled minimal harness" (a prior decision, made when platform bring-up itself was the goal) — `rsc new`'s iOS output is an `Info.plist` next to a raw executable, no `.xcodeproj`. That was enough to prove the rendering engine runs on iOS/wasm, but it is NOT a viable end-state: a shippable app needs entitlements, capabilities, push notifications, deep links, App Store icons/launch screens, and permission prompts — none of which are reachable without a real, editable native project. Verified directly in winit 0.30.13's source that this is a structural blocker, not a configuration gap: `EventLoop::run_app` on iOS calls `UIApplicationMain` itself and generates its own implicit `AppDelegate` — there is no supported way to embed winit inside a host `AppDelegate`/`SceneDelegate` we control, so `application(_:didFinishLaunchingWithOptions:)`, `application(_:open:options:)` (deep links), push-notification registration, and `BGTaskScheduler` are all unreachable as long as winit owns the iOS lifecycle. Android is structurally better — winit's Android backend runs on the `android-activity` crate, whose own docs say a `NativeActivity`/`GameActivity` subclass in Kotlin is the supported way to reach platform features — but we still ship no Gradle project, manifest, or Activity today.
**Decision**: This is the harness-generator choice D104 deferred ("Decide the harness generator... as part of platform bring-up"), now resolved: **generate real native host projects, not bare executables.**
- **iOS**: `rsc new` generates a real `.xcodeproj` (own `AppDelegate`/`SceneDelegate` in Swift, Info.plist, entitlements, asset catalog — all Xcode-editable). The Rust engine compiles to a **staticlib**, linked into the Xcode target; a thin Swift/ObjC bridge owns `UIApplicationMain`/`AppDelegate` for real and creates the `UIView`/`CAMetalLayer` that the Rust engine renders into via a small C FFI (init/resize/input-event/frame-tick calls) — winit's iOS backend is NOT used for the shipped app (desktop/web keep it; it works well there). This directly gives the user Xcode-native control over permissions, push, deep links, and App Store metadata.
- **Android**: `rsc new` generates a real Gradle project (`build.gradle`, `AndroidManifest.xml`, a `MainActivity.kt` subclassing `GameActivity`/`NativeActivity`). The Rust engine compiles to a **cdylib (.so)**, called via JNI from the Activity, rendering into a `SurfaceView`. Winit can still be used here (android-activity subclassing is supported), narrowing the gap from iOS.
- `rsc run --target ios|android` shifts from our own hand-rolled `codesign`/`simctl` bundling to invoking the real toolchains (`xcodebuild`, `./gradlew`) against the generated project — the generated project is the source of truth, `rsc` drives it rather than replaces it.
**Reason**: A cross-platform UI engine that cannot reach platform permissions, push notifications, or deep links is not production-viable on mobile; the earlier "prove it renders" MVP was the right first step but was never meant to be the final architecture. Real, editable native projects are also how Flutter's `ios/`/`android/` folders work — proven precedent for exactly this hybrid model.
**Scope note (added 2026-07-08, not yet designed — flagging so it isn't lost)**: this FFI boundary is also the right home for a future **platform capabilities API** — sensors, camera, biometrics, push-permission prompts, display insets/orientation, "am I mobile or desktop." These are NOT a separate architecture to invent: `rosace-platform` today is purely windowing/input (verified in the `CRATE_CONTRACTS.md` audit — the capability surface an earlier planning pass imagined for it, e.g. `Permission`/`Haptics`/`Biometrics`/`use_sensor()`, was never built), and winit cannot reach any OS capability API on iOS for the same structural reason it can't own the AppDelegate. Once the native host + FFI bridge exist (Step 1's `tzr_engine_init`/`resize`/`input`/`frame` boundary), capability calls are just more messages over that same channel — "read the accelerometer" or "request camera permission" is no different in kind from "deliver an input event." Desktop/web capability access (direct OS/browser APIs) doesn't need this detour and can be wired independently, sooner. Display insets/orientation specifically build on the already-shipped `rosace_core::SafeArea`; "what platform am I on" is D105's `Platform` enum. Revisit this note when Phase 24 actually starts.
**Rejected**: continuing to extend the winit-owns-everything hand-rolled harness for iOS — verified structurally impossible to reach AppDelegate-level features (`UIApplicationMain` ownership), not worth patching further.
**Affects**: `rosace-cli` (new/run generators), a new thin native-bridge crate (Rust staticlib/cdylib + FFI surface), `rosace-platform` (iOS event loop moves out of winit's ownership; Android may keep winit; eventually gains the capabilities surface per the scope note above)
**Relates to**: D104 (packaging styles — this resolves the deferred harness-generator choice), D102/D103 (hot-reload transports — the native-bridge boundary changes how a dylib/template payload reaches the running iOS app), D105 (Platform enum — "what platform am I on"), the shipped `rosace_core::SafeArea` (display insets — capability surface builds on this, doesn't replace it)

---

### D107 — Web SEO/accessibility: a semantic-tree-driven DOM shadow, NOT a second widget renderer
**Status**: PLANNED (full plan in `.steering/PHASE_25.md`)
**Problem**: The web target renders to `<canvas>` (same GPU/softbuffer pipeline as native). A canvas is opaque to search engines — no text, no structure, nothing to index, same failure mode as a screenshot — and a canvas-only first paint hurts Core Web Vitals (nothing visible until the wasm bundle loads/inits), which search ranking penalizes directly.
**Rejected option — compile all widgets to real HTML/CSS**: this is effectively a second, parallel widget renderer (every widget's paint logic reimplemented against DOM/CSS instead of canvas draw commands), maintained forever in lockstep with the canvas one. This is the exact shape of cost D105 already rejected for platform-adaptive theming (one API, two implementations to keep in sync) — just moved from the widget layer to the render-backend layer. Concrete precedent: Flutter tried this (its "HTML renderer" alongside CanvasKit) and has been deprecating the HTML renderer specifically because of this maintenance/consistency cost.
**Decision**: Canvas remains the ONLY visual renderer, on every platform (the core "pixel-identical everywhere" value of ROSACE is preserved). On web, additionally emit a semantic HTML shadow built from the semantic/accessibility tree ROSACE already has: `RenderTree::collect_semantics()` (D099, `rosace-widgets/src/tree/render_tree.rs`) already derives a nested `rosace_core::SemanticNode { role, label, children }` tree from widgets' declared `Semantics` entries, in paint order, respecting render-tree structure — this is exactly the source the shadow needs. Map each `SemanticNode` to a real HTML element (`Role::Text`→`<p>`/`<span>`, `Role::Button`→`<button>`, headings/links/lists once the role set covers them — see Phase 25 Step 1).
**Two delivery mechanisms, preferred order**:
1. **Build-time, via Declarative Shadow DOM** (`<template shadowrootmode="open">` — a real, now widely-shipped web-platform feature requiring no JS to construct the shadow root). For any route whose content is knowable at `rsc build --target web` time, run the semantic-tree→HTML mapping AT BUILD TIME and bake the result directly into the page's HTML response. Crawlers that don't execute JS still see full real content, and it's visible before wasm even downloads (also helps Core Web Vitals for that first-paint window) — strictly better than constructing the shadow only after hydration.
2. **Runtime JS-driven shadow (fallback)** — the original mechanism, updating the shadow tree as app state changes after hydration, for content the build step couldn't know about (dynamic/user-driven state). Both mechanisms use the same `SemanticNode`→HTML mapping; (1) is a build-time export of it, (2) is the live update path.
Also export a per-route `llms.txt`/plain-text summary from the same semantic tree — the emerging convention for AI/LLM crawlers (Perplexity, GPT search, etc.), essentially free once (1) exists.
**Explicitly separate, NOT decided here**: full dynamic server-side rendering of app LOGIC (not just the semantic/text tree) — running arbitrary component state server-side per-request — is a materially bigger feature and is NOT what (1) does (which only needs the semantic tree, already buildable offline for static content). Only pursue full dynamic SSR if a concrete use case needs per-request personalized markup; most app-like UIs don't.
**Reason**: Reuses infrastructure already built for screen-reader accessibility (D099) instead of building a second render backend — a search crawler and a screen reader want the same thing (real text + structure, not pixels), so one semantic tree serves both needs, and serving it at build time (rather than only via runtime JS) reaches JS-skipping crawlers and improves first-paint, without needing a server runtime. Keeps the "one widget set, one visual renderer" property intact.
**Affects**: `rosace-widgets` (semantics coverage must become comprehensive, not sparse — every widget that carries user-facing text needs a `Semantics` entry), `rosace-a11y` (its richer `Role` enum — includes `Link`/`Heading`/`List`/`ListItem`/`Tab` — should likely absorb/replace `rosace_core::semantic_node::Role`'s narrower set, since real HTML semantics need heading levels and links), `rosace-cli` (`rsc build --target web` gains the static semantic-HTML + `llms.txt` export step), `rosace-platform` (web-only: runtime shadow-DOM sync for post-hydration state changes)
**Relates to**: D099 (accessibility tree this reuses), D105 (the same "don't build two parallel implementations" reasoning, applied to rendering instead of theming)

---

### D108 — Pervasive default animation + an animation-authoring framework (raised 2026-07-08, scoped 2026-07-09 as Phase 26)
**Status**: SCOPED, IN PROGRESS — see `.steering/PHASE_26.md`. First real scope covers press/tap feedback, real momentum scroll, default-on nav transitions, and image load-in fades — mostly wiring `rosace-scroll`'s and `rosace-nav-anim`'s existing-but-orphaned engines into the real widget paths, not building new physics/transition systems from scratch (confirmed via a fresh audit, not assumed). The animation-authoring-framework/library half of the original vision remains explicitly deferred past this first phase.
**Existing foundation (shipped, never formally written down until now — a documentation gap in the same spirit as `CRATE_CONTRACTS.md`'s)**: ROSACE already has a **theme-global, not per-widget** animation model. `ThemeData.animation: AnimationConfig { enabled, duration_ms }` (default on, 160ms) is a single switch (`set_animations(bool)`) that governs every animated widget at once. `PaintCtx::animate_to(target, ms) -> f32` eases a per-render-tree-node persistent scalar — snaps instantly if the theme disables animation, otherwise exponentially eases and keeps requesting frames until settled. `Switch`/`Checkbox`/`Radio`/`SegmentedControl` already animate through this mechanism, automatically respecting the global toggle. `rosace-animate` (see `CRATE_CONTRACTS.md`) is the crate backing it — `use_animation`/`use_spring` let any widget drive a per-frame value through `Context`.
**The vision (not yet designed)**: extend "theme-pinned, automatic" animation far beyond the four widgets that have it today — smooth scroll (momentum/deceleration, not just instant offset jumps), navigation/screen-transition animation (`rosace-nav-anim` exists but isn't wired to be "just on" by default), press/tap feedback (ripple/fade), image load-in fades, list-item enter/exit — all "under the hood," no per-app wiring, governed by the same theme-global switch so an app can still turn it all off at once. On top of that: ROSACE should offer an **abundant, ready-to-use library of custom animations** and a **real framework for authoring new ones** (beyond today's low-level `Tween`/`Spring`/`Keyframe` primitives) as an explicit platform strength/differentiator, not an afterthought.
**Why not scoped yet**: this spans many widgets and touches `rosace-nav-anim`, `rosace-scroll`, `rosace-animate`, and probably `rosace-widgets` broadly — a real phase-sized effort, not a single decision. Recorded now, at the user's explicit request, specifically so it survives to when it's actually picked up rather than being re-discovered from scratch.
**Affects (when scoped)**: `rosace-animate`, `rosace-nav-anim`, `rosace-scroll`, `rosace-widgets`, `rosace-theme` (the `AnimationConfig` surface likely grows — e.g. per-category durations/curves, not just one global duration).
**Relates to**: the existing (until-now-undocumented) theme-global animation model this extends; D105 (same "theme is the single dial" philosophy, applied to motion instead of color/platform look).

---

### D109 — Move core rendering off tiny-skia (CPU) onto wgpu GPU shaders, via a new `rosace-shader` crate (raised 2026-07-10, rewritten same day, scoped as Phase 27)
**Status**: SCOPED, NOT STARTED — see `.steering/PHASE_27.md`.
**Decision**: ROSACE's rendering moves from CPU rasterization (`tiny-skia`/`TinySkiaCanvas`) to GPU-native drawing via wgpu, for BOTH built-in shapes and custom effects, using one mechanism. `FillRect`/`FillRRect`/`FillCircle`/`FillGradient`/`FillArc`/`DrawShadow`/`StrokeRect`/`StrokeRRect` each become a built-in registered `wgpu::RenderPipeline` (WGSL fragment shader draws the shape directly on GPU — SDF-style, no CPU tessellation step) instead of a CPU `tiny-skia` call. Text moves to a cached GPU glyph atlas — glyphon-style *mechanism*, built on ROSACE's real text stack, NOT by adopting glyphon's dependencies: `rosace-render`'s fontdue-backed `FontCache` already rasterizes each distinct glyph once into a CPU-side cache today; the atlas moves that cache's *storage* into a GPU texture so already-seen glyphs render as GPU instanced-quad samples instead of today's per-frame CPU blit of cached coverage bitmaps into the pixel buffer (`cosmic-text`/`etagere` are what glyphon itself uses upstream, cited as prior art only — importing them would silently replace `FontCache`, a separate decision nobody has made; corrected 2026-07-10 from a first draft that named `cosmic-text` as the rasterizer). A `ShaderPaint` widget (own type, not a `CustomPaint` mode-switch) lets app/widget authors register additional custom pipelines through the exact same registry — this was D109's original, narrower scope (raised first) and now falls out of the core-rendering mechanism for free rather than being a separate escape hatch. All pipeline registration is eager (compiled at registration time, not lazy-on-first-paint) — see Reason.
**API surface**: `DrawCommand` gains `ShaderFill { pipeline_id: PipelineId, rect: Rect, uniforms: Vec<u8> }` (threaded through `offset`/`morph` like every other variant, so Hero transitions and damage-rect translation work on GPU-drawn regions for free); built-in shape variants keep their existing call sites (`cx.fill_rect(...)` etc.) unchanged — only their *implementation* moves from a `tiny-skia` call to a registered built-in pipeline, so no widget author code changes for existing shapes. Uniform bytes are produced by `#[derive(ShaderUniforms)]` (`rosace-macros`) generating compile-time-layout-checked `to_bytes()`, never hand-packed. `PipelineId → wgpu::RenderPipeline` compilation/storage stays inside `rosace-compositor` (the only crate holding a live `wgpu::Device`) — and because `rosace-compositor` is Layer 0 with a hard "zero rosace-* deps" contract, it CANNOT import `rosace-shader`'s types: `GpuPresenter`'s registration API takes only primitive/std types (`u64` pipeline id, `&str` WGSL source, a compositor-owned blend enum), and `rosace-platform` (already the compositor's only consumer) converts from the typed `ShaderSpec` at the boundary (resolution added 2026-07-10 — the first draft implied the compositor would accept `rosace-shader` types, which would have violated its own contract); `ShaderSpec`/`PipelineId`/`ShaderUniforms` live in new Layer 5 crate `rosace-shader`, the future home for a built-in shader/effect library (blur, noise, gradients) — mirrors `tessera-components`'s role, but for shaders instead of widgets.
**Reason**: Comparative review of `tessera-ui` (real Rust UI crate) found its renderer has NO CPU shape rasterizer at all — every primitive is a registered wgpu `DrawablePipeline`; text uses `glyphon` (cache-once-reuse-forever glyph atlas). Confirmed the same pattern is now standard across the field, not just Tessera: Flutter replaced Skia's CPU-adjacent path with **Impeller** (default on iOS/Android as of 2026) specifically because JIT shader compilation + CPU tessellation caused jank and battery drain; Jetpack Compose uses full Skia's **GPU backend** (Ganesh via OpenGL/Vulkan/Metal), never its CPU/software backend; SwiftUI is GPU-composited (Core Animation/Metal) throughout, no CPU rasterization step at all. ROSACE's `tiny-skia` is a pure-Rust reimplementation of *only* Skia's CPU/software backend — it never had a GPU mode — and D108's pervasive default animation makes this the wrong tradeoff: animated widgets are dirty by design every frame for the animation's duration, so every animated frame pays a real CPU rasterization cost that damage-rect/frame-skip cannot help with (those only help idle apps). This also resolves `D-DEF-004` (plugin registry governance, previously unscoped) — `rosace-shader`'s registry is its first concrete instance.
**Lesson explicitly carried in from Impeller's own history**: Impeller initially shipped with runtime (JIT) shader compilation and had to fix first-render/first-animation stutter by moving to ahead-of-time compilation. D109 avoids repeating that mistake: pipelines compile at `register_shader`/startup time, never lazily on first paint.
**Scope discipline**: explicitly NOT MVP-shortcut work — confirmed by the user 2026-07-10 ("we need to ship a framework in future, none of this are mvp"; "this is not an mvp... i might often say need an mvp but that doesnt mean it is not an mvp"). Raw-byte uniforms and a `CustomPaint`-closure mode-switch were both considered and rejected in favor of typed/dedicated-widget APIs. `tiny-skia` is not deleted in this phase — see `PHASE_27.md`'s migration-order steps; it is only fully removable once GPU parity for every shape + text is proven. Backdrop-blur/glassmorphism (needs a two-pass compositor render) remains a named, deliberately deferred follow-up. Two more hard constraints found by code review 2026-07-10: (1) **web/wasm never touches the GPU today** — `rosace-platform/src/web.rs` presents via 2D-canvas `putImageData` and never constructs a `GpuPresenter`, so this phase's pipelines are desktop/mobile-only until a wgpu-on-wasm (WebGPU) presenter is scoped as its own phase, and `tiny-skia` therefore cannot be fully deleted even after every Phase 27 step lands — web keeps the CPU path. (2) Desktop's **softbuffer CPU fallback** (`app.rs`, taken when `GpuPresenter::new` returns `None`) would render no shapes at all once shapes are GPU-only, unless tiny-skia is explicitly kept as that fallback's renderer — Phase 27 decides this in-phase (see its design-constraints section), not by discovery.
**Affects**: `rosace-render` (new DrawCommand variant, shape-drawing implementation moves), `rosace-compositor` (registry, eager pipeline compilation, new `CompositorLayer::Shader` variant in `present_layers`), `rosace-macros` (derive macro), `rosace-widgets` (new `ShaderPaint` widget; existing widgets unchanged at the call-site level), `rosace-render`'s `FontCache` (glyph-atlas caching — NOT `rosace-shaping`, which has zero render-path call sites, Known Issue #12), new crate `rosace-shader`.
**Relates to**: D091 (RenderTree single-owner state — pipeline compilation keyed stably, not per-frame), D100 (CustomPaint's no-raw-pixel-access contract, preserved not extended — `ShaderPaint` is a distinct type), D-DEF-004 (now scoped by this decision).

---

### D110 — App lifecycle + push notifications over the existing native-host FFI bridge (raised + scoped 2026-07-10 as Phase 29)
**Status**: Step 1 (lifecycle) LANDED + live-verified on the iOS Simulator 2026-07-14 — `LifecycleState`'s home resolved to `rosace-core/src/app_lifecycle.rs` (the `ime_hint.rs` bridge precedent; D042's "Affects: rosace-platform" was unreachable from component code), four `RSC_EVENT_LIFECYCLE_*` kinds over the bridge, `Engine::input` applies lifecycle immediately (background-safe atom write) since the display link is paused exactly when `Background` arrives. See `.steering/PHASE_29.md` Step 1 for full detail. Step 2 (push) LANDED + live-verified same day: `PUSH_PERMISSION`/`PUSH_TOKEN`/`PUSH_MESSAGE` atoms (camera's three-piece shape + token + latest-wins/seq foreground delivery), four `rsc_push_*` C-ABI fns, iOS template wired end-to-end (`UNUserNotificationCenterDelegate` on AppDelegate, frame-tick polling) — real permission prompt + user grant + `simctl push` foreground delivery all proven on a fresh unpatched `rsc new` scaffold. Named account-blocked deferrals: real APNs token/network push (Apple Developer team), Android FCM (Firebase project). Phase 29 complete with those deferrals.
**Decision**: Add `LifecycleState` (D042: Active/Inactive/Background/Suspended) and push-notification registration/delivery as two more capabilities crossing the D106/Phase 24 native-host FFI bridge, following the exact three-piece shape Phase 24 Step 5 already proved with camera permission (request queue + result/state atom + host-side native call) — not a new architecture. iOS: real `AppDelegate`/`SceneDelegate` methods (`applicationDidBecomeActive`, `applicationDidEnterBackground`, `didRegisterForRemoteNotificationsWithDeviceToken`, `didReceiveRemoteNotification`) call into Rust over new FFI entry points. Android: `MainActivity.kt`'s `onResume`/`onPause`/`onStop` + `FirebaseMessagingService` do the same via JNI.
**Reason**: Checked the actual code, not the plan-on-paper: `D042`'s `GlobalAtom<LifecycleState>`/`use_app_lifecycle()` has zero real implementation anywhere in the workspace (confirmed by grep — decision recorded, never built). `rosace-ffi/src/event.rs`'s FFI event kinds are only `MouseMove/Down/Up`, `KeyDown/Up`, `Text`, `WindowResized`, `Scroll` — no lifecycle event crosses the boundary at all. Push notifications were only ever named as an *example candidate* for Phase 24 Step 5's single proof; camera permission got built instead, so push notifications remain unbuilt. A framework being positioned to ship real apps needs both — background/resume state to pause expensive work (animation, network polling) and push notifications are baseline mobile-app requirements, not optional polish.
**Sequencing**: after Phase 27 (GPU rendering) and Phase 28 (TextInput/IME) — see those phases' own docs; this is independent of both technically (touches `rosace-ffi`/native hosts, not rendering or text), but the queue stays GPU → TextInput/IME → this, per user sequencing decisions made the same day.
**Affects**: `rosace-ffi` (new FFI event kinds + capability module mirroring `capability.rs`'s camera shape), native host templates (`rsc new`'s generated `AppDelegate.swift`/`SceneDelegate.swift`/`MainActivity.kt`), `rosace-core` or a new home for the actual `LifecycleState` atom (D042 never picked one — needs resolving as part of this phase, not assumed).
**Relates to**: D042 (the never-implemented lifecycle decision this finally builds), D106/Phase 24 (the FFI bridge and camera-permission pattern this reuses verbatim).

---

### D111 — Correction to D108: default animation is a curated widget list, not universal (raised + fixed 2026-07-10)
**Status**: LANDED.
**Decision**: D108's "pervasive default animation" vision is corrected — default (theme-governed, zero-per-app-code) animation applies ONLY to a curated, explicit set: press/tap feedback, real momentum scroll, default-on nav transitions, and the toggle-state widgets (`Switch`/`Checkbox`/`Radio`/`SegmentedControl`). It does NOT apply, by default, to per-item content inside recycled/virtualized containers (`ListView` rows and anything painted through them) — that case is removed. `Image`'s automatic load-in fade (D108/Phase 26 Step 4) is reverted: `Image::paint` now always blits at `opacity: 1.0` immediately, no `seed_anim_if_unset`/`animate_to` call. `DrawCommand::BlitRgba`'s `opacity` field is kept (real per-call parameter, still used deliberately by Hero/shared-element transitions, which DO have stable per-widget identity via explicit tags) — only `Image`'s automatic default use of it is removed.
**Reason**: User-reported ("if the image animate in scroll view it is bad") and confirmed in code: `ListView` (`rosace-widgets/src/tree/list_view.rs`) is virtualized — only viewport-visible rows are built/painted each frame, each via `ctx.child(row_rect)` → `RenderTree::slot()` (`render_tree.rs:167`). `slot()` allocates render-tree nodes **positionally** — the *n*-th `child()` call under a parent this frame gets child-slot *n*, with no awareness of which data index that call represents. As the visible window scrolls, a given slot's underlying `RenderNode` (and its persisted `anim` field, D091) gets reassigned across frames to whatever row currently occupies that screen position — NOT to the same image. Consequence: a newly-revealed image can land in a slot already settled at `anim = 1.0` and never fade in, or land in a slot mid-animation from a *different* image's fade and pop in at that image's stale partial opacity — a real visible glitch, not a preference issue. More generally, this confirms the user's broader correction: "not every widget needs default animation" — recycled/virtualized content is exactly the case where per-node animated state is structurally unsafe under the current positional slot allocator, regardless of which specific animation is attempted.
**Fix landed**: `Image::paint` (`rosace-widgets/src/tree/image.rs`) no longer calls `seed_anim_if_unset`/`animate_to`; blits at `opacity: 1.0` unconditionally. `rosace/src/engine.rs`'s two Phase-26-Step-4 tests (`real_decoded_image_fades_in_from_zero_...`, `image_fade_is_instant_when_animations_are_disabled`) replaced with two tests confirming `Image` drives no per-node animated scalar at all, with and without the global animation toggle. Full `cargo build --workspace` and `cargo test --workspace --no-fail-fast` clean (0 failures) after the change.
**NOT fixed by this decision (see Known Issue #11 in `CRATE_CONTRACTS.md`)**: the underlying `RenderTree::slot()` positional-allocation bug itself is NOT fixed — only its one known trigger (the image fade default) is removed. `ListView` rows still have no stable identity across scroll frames; ANY future per-row retained state (hover, focus, a future per-row animation, per-row scroll-linked effects) would hit the identical bug. Fixing `slot()` itself (e.g. keying `ListView` children by data index instead of call order) is a real, separate, larger change to core render-tree allocation — out of scope for this correction, flagged as a known issue for its own future decision/phase rather than bundled in here under time pressure.
**Affects**: `rosace-widgets` (`image.rs`), `rosace` (`engine.rs` tests), `.steering/PHASE_26.md` (Step 4's description now needs a landed-then-reverted note).
**Relates to**: D108 (the vision this narrows), D091 (the per-node retained-state model whose positional-allocation gap this exposes), Known Issue #11 (the unfixed root cause).

---

### D112 — Real text editing, real OS IME, and wiring `rosace-forms` — TextInput stops being decorative (raised + scoped 2026-07-10 as Phase 28)
**Status**: LANDED 2026-07-12, ALL 8 STEPS (renumbered/expanded under D116 mid-phase) — real keyboard editing, `TextArea`, real desktop OS IME, SpanSource/CursorStyle, context menu/selection handles, and `rosace-forms` wired for real via `.field()`/`.filters()`. Two disclosed deferrals: Step 6's mobile native-host IME and Step 7's magnifier loupe — see `.steering/PHASE_28.md` for full detail on both.
**Decision**: `TextInput` gains real keyboard-driven editing (cursor, selection, insert/delete, standard shortcuts, clipboard via the existing `rosace-clipboard`), a new multi-line `TextArea` widget, real OS IME composition (replacing `rosace-ime`'s `NoopIme` — winit's `WindowEvent::Ime` on desktop, `UITextInput`/`InputConnection` via the D106 FFI bridge on mobile), and `rosace-forms`'s `Field`/`Validator`/`Form` wired to both — validation state and submit flow, not just a standalone unused crate.
**Reason**: Checked the actual code (not the plan): `TextInput` (`rosace-widgets/src/tree/text_input.rs`) is paint-only — `focused` is a plain bool field, and `rosace/src/engine.rs`'s `KeyDown` handling only drives Tab focus-cycling, never inserts/deletes a character. You cannot type into it in a running app today. `rosace-ime` (546 lines: `ImeHandler`/`NoopIme`/`ImeComposition`/`ImeEvent`/`ImeState`) is a data-model stub only — `PHASE_8.md` explicitly deferred real OS IME integration to v1.0, and grepping the workspace found no winit `Ime` event handling anywhere in `rosace-platform`. `rosace-forms` (503 lines: `field.rs`/`validator.rs`/`form.rs`) has zero references from `rosace-widgets` or anywhere else — the same "built, never wired" pattern already found in `ScrollView`/`Navigator`/`ImageCache`. The user's original priority was "TextInput/**Forms**" — this phase is the real scope of that request, not just editing.
**Affects**: `rosace-widgets` (`text_input.rs` rewrite, new `text_area.rs`), `rosace-ime` (real `ImeHandler` impl replacing `NoopIme`), `rosace-platform` (winit `Ime` event plumbing), `rosace-ffi` (mobile IME capability, same shape as camera/lifecycle), `rosace-forms` (wired, not rewritten — its `Field`/`Validator` API is reused as-is unless a real integration blocker surfaces).
**Relates to**: D106 (the FFI bridge mobile IME reuses), D091 (per-node retained state — cursor/selection position lives on the render-tree node, same discipline as everything else), Known Issue #11 (any list-of-TextInputs must NOT repeat the positional-slot identity bug — flagged explicitly in Phase 28's steps).

---

### D113 — Networking: sync HTTP/WebSocket crates, not hand-rolled TLS (raised + scoped 2026-07-10 as Phase 30)
**Status**: Step 1 (`ureq` HTTP client) LANDED + live-verified 2026-07-14 — `HttpClient` in `rosace-net/src/client.rs` (blocking + `fetch()` thread/mpsc handle), https-rejecting `http.rs` deleted, `ImageLoader` rebuilt on the shared client, wasm resolved as the documented named-gap (compiles, returns `Err`, never panics — including a thread-free wasm `fetch`). Live proof: `http_demo` rendered real JSON from `https://httpbin.org/json`. Step 2 (use_query) LANDED same day — no-polling worker-writes-atom design, on_unmount alive-flag cleanup with a connection-actually-closed regression test. Steps 3-4 LANDED 2026-07-15: WsClient on tungstenite 0.26 (hand-rolled RFC 6455 deleted, wss works, API unchanged) + use_websocket (local-echo-server-verified cleanup, live wss://ws.postman-echo.com proof) and use_network_status (attempt-based prober + set_network_status D106 host seam, live Wi-Fi-toggle proof). PHASE 30 COMPLETE — web backends and mobile-host connectivity halves are named deferrals in PHASE_30.md. See `.steering/PHASE_30.md`.
**Decision**: Reverses the Phase 1/6/7-era "no reqwest, no tungstenite, hand-roll HTTP over raw TcpStream" stance for anything touching TLS. `rosace-net` (today: image loading only, zero HTTP deps) gains a general HTTP client built on `ureq` (synchronous, `rustls`-based, no async runtime — respects the framework's existing no-tokio constraint while not reimplementing TLS). `rosace-ws` (today: hand-rolled RFC 6455 handshake over raw `TcpStream`, per its own doc comment) moves to the `tungstenite` crate (the synchronous crate, not `tokio-tungstenite`) for the same reason. `D012`'s decided-but-never-built hooks (`use_query`, `use_websocket`, `use_network_status`) get real implementations on top of these.
**Reason**: Hand-rolling TLS is a security liability, not a quality signal — reimplementing HTTPS from scratch is exactly the kind of "looks like it avoids a dependency, actually just reimplements a hard, security-critical problem worse" shortcut this project has explicitly rejected elsewhere (see [[feedback_no_mvp_shortcuts]] equivalent, D109's scope-discipline note). Checked the actual code (corrected 2026-07-10 — the reality is starker than "hand-rolled TLS"): `rosace-net` has zero HTTP/TLS dependencies AND its `parse_url` **rejects `https://` URLs outright** (unit test `parse_url_rejects_https`, `http.rs`) — there is no TLS at all today, hand-rolled or otherwise, so a remote image on an https URL simply cannot load, and virtually every real 2026 endpoint is https-only; `rosace-ws`'s handshake IS genuinely hand-rolled per its own doc comment. The realistic options were hand-rolling TLS (the liability above) or adopting `rustls` via `ureq` — this decision picks the latter. **Wasm caveat**: `ureq`/`tungstenite` are `std::net`-based and non-functional on `wasm32-unknown-unknown` (as is the `std::thread` pattern `rosace-net` already uses); since the whole SDK compiles for wasm today, both must be target-gated, with web `fetch()`/WebSocket backends or an explicit documented web gap decided at Phase 30 Step 1 — not discovered as a broken wasm build. `ureq`/`tungstenite` were chosen specifically because both are synchronous — they don't pull in `tokio`/`async-std`, preserving the "no async runtime dependency" architecture `rosace-net`'s original design already committed to; only the "reinvent TLS/the handshake by hand" part is reversed.
**Affects**: `rosace-net` (real HTTP client, generalized beyond images), `rosace-ws` (swap hand-rolled handshake for `tungstenite`), `rosace-state` (the `use_query`/`use_websocket`/`use_network_status` hooks D012 decided but never built).
**Relates to**: D012 (the hooks this finally implements), [[feedback_no_mvp_shortcuts]] (why hand-rolled TLS is the wrong kind of "avoid a dependency").

---

### D114 — Persistence: real `#[persist]` tiers backed by `rusqlite`, secure tier deferred to the FFI bridge (raised + scoped 2026-07-10 as Phase 31)
**Status**: SCOPED, NOT STARTED — see `.steering/PHASE_31.md`.
**Decision**: `D008`'s `#[persist(reload/session/permanent/encrypted)]` gets a real implementation. `reload` and `session` tiers are in-process (survive hot-reload/backgrounding without hitting disk — ties into D102's hot-reload rehydration, which already serializes `#[persist]` atoms conceptually). `permanent` writes to a real embedded `rusqlite` (SQLite) database — synchronous, no async runtime, matching D113's networking crate-choice reasoning. `encrypted` (secure storage) is explicitly NOT solved by a Rust crate — it's deferred to the platform Keychain (iOS)/Keystore (Android), reachable only through the D106 FFI bridge, and is scoped as an addition to Phase 29's capability list (same three-piece shape as camera/lifecycle/push), not duplicated here.
**Reason**: Grepped `rosace-state`/`rosace-macros` for `persist`/`#[persist]` — zero implementation anywhere, despite `D008` being LOCKED since early planning. A framework being positioned to ship real apps needs state to survive a restart (login sessions, cached data, prefs) — right now nothing does. `rusqlite` chosen over a hand-rolled file format for the same reason `ureq`/`tungstenite` were chosen in D113: don't reinvent a solved, correctness-critical problem (data file format durability/corruption resistance) when a mature, synchronous, dependency-light crate exists. Real client apps essentially never need direct SQL access to a remote database (that's server-side, behind an HTTP API from D113) — local embedded SQLite covers the actual client-side need. **Wasm caveat** (added 2026-07-10): `rusqlite` links C SQLite and does not build on `wasm32-unknown-unknown` — the `rusqlite` dependency must be target-gated so the SDK keeps compiling for wasm, and the web story for the `permanent` tier (localStorage/IndexedDB) is explicitly named-deferred, decided at Phase 31 Step 1, not silently broken.
**Affects**: `rosace-state` (real `#[persist]` macro backing), `rosace-macros` (the attribute), new dependency on `rusqlite` (likely a new thin crate, `rosace-storage`, rather than bloating `rosace-state` with a SQL dependency — decide in Phase 31 Step 1).
**Relates to**: D008 (the decision this finally builds), D113 (same reasoning for crate choice), D110/Phase 29 (secure-storage capability lives there, not here).

---

### D115 — Widget expansion + extensible icons + color-glyph/rich-text support (raised + scoped 2026-07-10 as Phase 32)
**Status**: SCOPED, NOT STARTED — see `.steering/PHASE_32.md`.
**Decision**: Add the widgets a typical real app needs that don't exist today: `FloatingActionButton`, `BottomNavigationBar`, `DatePicker`/`TimePicker`, `Carousel`/`PageView`, `Stepper`, `SearchBar`, `RatingBar`, `DataTable`, `Snackbar`. Replace `Icon`'s closed 27-shape hardcoded enum (`icon.rs`) with an extensible system (icon font or SVG-path-based) a third party can add to without editing core `rosace-widgets` code. Wire `rosace-text`'s already-built-but-orphaned `RichText`/`TextSpan` (multi-style spans) into the `Text` widget — today it only calls `word_wrap` for single-style paragraphs. Add color-glyph (emoji) rendering, designed as part of Phase 27 Step 4's glyph atlas work (not bolted on after) since emoji need bitmap/COLR glyph handling distinct from the atlas's default monochrome-vector-glyph path.
**Reason**: User: "we need more widgets honestly" — confirmed against the actual 55-file widget set (`rosace-widgets/src/tree/`): core controls/layout/overlays are solid, but the above list is entirely missing (grepped, zero hits). `Icon` is a closed enum, not extensible — a real blocker for third-party widget authors (D-DEF-004/D109's plugin-registry spirit applies to icons too, not just shaders). `rosace-text`'s `RichText`/`TextSpan` orphaned-crate finding matches the exact pattern already found in `rosace-forms` (D112), `ScrollView`/`Navigator`/`ImageCache` — decided/built infrastructure that never got wired to the widget apps actually use.
**Sequencing note**: widget authoring is "most easy when we have the best render object available" (user's own framing) — new widgets built against Phase 27's GPU-native `DrawCommand` set (once it lands) avoid being built twice. This phase is sequenced after Phase 27 for that reason, independent of Phases 28-31 otherwise.
**Affects**: `rosace-widgets` (new widget files, `icon.rs` rewrite, `text.rs` rich-text wiring), `rosace-text` (consumed for real, not just `word_wrap`), `rosace-render`/`rosace-shader` (color-glyph rendering, ties to Phase 27 Step 4).
**Relates to**: D109/Phase 27 Step 4 (the glyph atlas emoji support builds on), `WIDGET_AUTHORING_GUIDE.md` (the contract new widgets and the icon system follow).

---

### D116 — The text-editing architecture: one layered core under every editable surface (raised + scoped 2026-07-12, expands D112/Phase 28)
**Status**: Layers 1/2/4 (document, edit core, behavior) LANDED 2026-07-12 as Phase 28 Step 2 — `Transaction`/`Selection`/`Command`/`EditController` all real and tested. Layer 3 (`TextLayoutSnapshot`) LANDED 2026-07-12 as Phase 28 Step 3 — real click-to-glyph placement, mouse drag selection, double/triple-click, all dispatched through the same headless `FrameEngine` integration tests. `TextArea` LANDED 2026-07-12 as Phase 28 Step 4 — multi-line wrap, Enter-newline, goal-column Up/Down, virtualized-paint scrolling, built entirely on the unchanged Step 2/3 core (only `TextLayoutSnapshot`'s pre-existing multi-line shape was exercised, nothing added to it). Layer 5 (render/styling) LANDED 2026-07-12 as Phase 28 Step 5 — `SpanSource`/`style_runs` (the markdown/syntax-highlighting seam, incremental via the new `TextEditState.last_edit_range`) and `CursorStyle`/`CursorShape` (Bar/Block/Underline/Custom, theme-default via `ThemeData::ext`, D105) shared by both widgets via a common `paint_caret` fn. `markdown_editor_demo` capstone added. Real OS IME (desktop half) LANDED 2026-07-12 as Phase 28 Step 6 — `rosace-platform` translates real `WindowEvent::Ime` into `InputEvent::Ime(rosace_ime::ImeEvent)`; `text_edit.rs`'s "provisional transaction" model (`ime_set_preedit`/`ime_commit`) makes a whole CJK composition undo in one hop; a new `GlobalAtom` bridge at `rosace-core` (`ime_cursor_area`/`keyboard_type`) crosses the rosace-widgets↔rosace-platform layer gap without widening `run_layered`'s closure signature. Known Issue #15 (`TZR_KEY_DELETE/_HOME/_END`) fixed. Mobile native-host IME (`UITextInput`/`InputConnection`) deferred to a real device session, matching the camera-capability precedent. Device-adaptive selection UX LANDED 2026-07-12 as Phase 28 Step 7 — `MouseButton::Right` routing (previously dropped silently) opens a real desktop context menu (Cut/Copy/Paste/Select All) via the existing `push_overlay`/`Menu` machinery, driving the exact same `text_edit`/`rosace_clipboard` calls the keyboard shortcuts use; long-press-to-select-word + draggable selection handles added, reusing `LineLayout::x_at`/`position_at`. A real pre-existing race (a background long-press timer never reliably cancelled by anything but the next mouse gesture) was found and fixed at the root via eager cancellation on any keyboard event, not papered over with a longer timeout. Magnifier loupe explicitly deferred (named in the step's own scope as a Phase 27 shader job). `rosace-forms` wiring LANDED 2026-07-12 as Phase 28 Step 8, the phase's final step — `FormField` redesigned atom-backed (`Clone` shares live state, matching `EditController`/`ScrollController`'s convention) so `TextInput::field(f)`/`Form::field(f)`/a submit button's own clone all stay in sync with zero manual wiring; `Form::submit()` and `Button::disabled_if()` added (didn't exist before); `InputFilter` applies at `commit_text_edit`, the one funnel every edit source reaches, deliberately separate from `Validator` (filters reject characters, validators judge complete values). Phase 28 is now fully complete. See `.steering/PHASE_28.md` for full detail.
**Decision**: ROSACE gets ONE text-editing core with hard seams between five layers, such that a login field, a search box, a chat composer, a markdown editor with live highlighting, and (someday) a code-editor-class widget are all CONSUMERS of the same core — never parallel reimplementations. The user's explicit framing: this is not about making `TextArea` a code editor; it's about making the foundation capable enough that anything text-shaped can be built on top without touching the core again.

**The five layers and their seams**:
1. **Document**: a plain `String` today, ALWAYS mutated through layer 2's transactions — never ad-hoc string surgery. Because nothing above layer 2 touches storage directly, swapping in a rope for large documents later is a storage-only change (named trigger: real apps editing >~1MB documents; until then a rope is speculative complexity).
2. **Edit core**: every change is a `Transaction` — a set of `(range, replacement)` edits applied atomically against a `Selection`. `Selection` is `Vec<SelectionRange { anchor, head, affinity }>` from day one — single-caret is the one-element case, so multi-cursor (a code-editor feature) is a UI feature later, not a data-model rewrite. Transactions are invertible → **undo/redo** is a per-field stack of inverse transactions (persistent node state, D091). Transactions are also the future seam for IME preedit (a provisional transaction), programmatic editing, and — if ever wanted — collaborative editing (OT/CRDT operate on exactly this shape). Cursor/word boundaries are **grapheme-cluster** correct via the `unicode-segmentation` crate — approved here by the same reasoning as D113's `ureq`/`rustls`: UAX #29 segmentation is a solved, correctness-critical spec; hand-rolling it is the hand-rolled-TLS mistake wearing a different hat. (Step 1's char-index ops are the compatible substrate; boundaries tighten to graphemes in Step 2.)
3. **Layout seam — `TextLayoutSnapshot`**: the keystone that dissolves the `FontCache`-is-`!Sync` wall Step 1 documented. During paint (where `FontCache` IS available), the widget computes plain-data geometry — per-line boxes, per-boundary caret x positions — and stores it on its render-tree node. Engine dispatch (click, drag, touch handles, IME cursor-area reporting) then answers point→position and position→point queries against that snapshot with zero font access. This one structure unlocks click-to-glyph caret placement, mouse drag selection, double-click word / triple-click line, touch selection handles, the IME candidate-window rect, and scroll-caret-into-view. (This is Flutter's `TextPainter`/`RenderEditable` relationship, named.)
4. **Behavior**: a `Command` enum (`MoveLeft`, `MoveWordLeft`, `DeleteWordBack`, `SelectAll`, `Undo`, …) between key events and edit-core calls, with a default keymap table. Apps/widgets can rebind or extend (a future code editor adds commands like `AddCursorBelow` without touching dispatch); platform conventions (Cmd vs Ctrl) live in the keymap, not scattered `if`s.
5. **Render**: paint order per field = background **decorations** (selection highlight, search matches, IME preedit underline — range-keyed, engine- or app-supplied) → **text runs** styled by an optional app-supplied **`SpanSource`** hook (`fn(&str, changed_range) -> Vec<Span { range, style }>`, invalidated incrementally by transaction ranges — this is the entire markdown/syntax-highlighting story: the app brings the tokenizer, the widget just paints spans) → **caret(s)** styled by **`CursorStyle`** — we paint the caret ourselves (it is already just a `fill_rect`), so width, color, corner radius, blink rate, shape (`Bar`/`Block`/`Underline`) and a fully custom painter (any `DrawCommand`s — an icon, even a Phase 27 shader) are all ours to offer, with theme-level defaults and per-field overrides.

**App-facing API** (the layer app authors actually see): `TextInput`/`TextArea` stay CONTROLLED components (`.value()`/`.on_change()`, unchanged). Added: `.controller()` returning a per-field **`EditController`** (D101 `ScrollController` precedent — node-persistent, zero wiring) exposing programmatic `replace_range`/`insert_at_cursor`/`get_selection`/`set_selection`/`select_all`/`undo`/`redo` — this is what makes a markdown toolbar's Bold button (`wrap selection in **`) a 5-line app closure. Plus `.spans(SpanSource)`, `.cursor_style(CursorStyle)`, `.filters(...)` (max-length/character-class input filters), and mobile keyboard-type hints (email/numeric/URL — hints to the OS keyboard, not distinct widget types).
**Also planned as real steps (previously unscoped gaps, surfaced 2026-07-12)**: mouse drag selection; double-click word / triple-click line; click-to-glyph caret placement (replacing Step 1's caret-to-end simplification); a context menu (right-click desktop / long-press mobile) with Cut/Copy/Paste/Select All via the existing overlay system; touch selection handles + magnifier for mobile alongside the FFI IME work.
**Deliberately NOT in this decision (named, with owners)**: editable rich text / WYSIWYG (spans are read-only styling over a plain string; contenteditable's history is the cautionary tale); a code-editor-class widget (gutters, folding, multi-cursor UI, virtualized huge-file rendering — a future phase built ON these seams; the multi-cursor data model and Command seam land now precisely so that phase needs no core rewrite); BiDi caret movement in mixed RTL/LTR text (v1.0, with D014's real shaping — the snapshot's line/position model must not preclude it, and does not); rope storage (behind the transaction seam, trigger named above); spellcheck, autocomplete, drag-and-drop text (already named deferrals).
**Reason**: The 2026-07-12 review of Phase 28's original plan found it produced a working field but with dead-end seams: no transaction log (retrofitting undo/IME/collab later means rewriting every edit path), single-cursor baked into the state shape, no click→glyph story past the `!Sync` wall, no styling hook (so markdown highlighting would have forked `TextArea`), and no home for the touch/context-menu UX users assume. Every mature text stack studied (CodeMirror 6, Flutter's editable text, TextKit 2, xi-editor's post-mortem) converges on exactly these seams; designing them in now costs one extra step — retrofitting them costs a rewrite per feature.
**Affects**: `rosace-widgets` (`text_edit` grows transactions/selection/commands/snapshot/spans/cursor-style; `TextInput`/`TextArea`), `rosace` (engine dispatch through the Command layer), `rosace-a11y` (unchanged — focus model landed in Step 1), `rosace-platform`/`rosace-ffi` (IME + touch + keyboard-hint events, Known Issue #15), `rosace-forms` (Step 8), new approved dep `unicode-segmentation`.
**Relates to**: D112 (the phase this restructures), D091 (undo stack/snapshot are node-persistent state), D101 (`EditController` follows `ScrollController`), D113 (dependency-approval reasoning reused for `unicode-segmentation`), D014 (BiDi/shaping deferred together), Phase 32/D115 (read-only `RichText` remains its job; a future code-editor phase builds on this).

---

### D117 — `FontCache::system_ui()` was silently loading the wrong face out of every macOS `.ttc` collection, making `FontWeight::Bold` invisible (found + fixed 2026-07-12, live-app regression from Phase 28)

**Bug**: user-reported via a live screenshot of `markdown_editor_demo` — headings and `**bold**` markdown spans showed no visible weight difference from body text, even though the whole D116 span/weight pipeline (`Span::weight` → `style_runs` → `DrawCommand::DrawText{weight}` → `layout_glyphs` → `FontCache::resolve`) was independently verified correct top to bottom, and a probe confirmed every candidate font file parses fine via `fontdue`. The real bug was one level lower: `fontdue::Font::from_bytes` defaults to `collection_index: 0`, and macOS ships every UI family as a `.ttc` collection where index 0 is NOT the Regular face — on `Avenir Next.ttc`, index 0 is **Avenir Next Bold** (Regular is index 7, found via `ttf_parser::fonts_in_collection` + the name table). So `system_ui()`'s "regular" font was already Avenir Next Bold, and "bold" fell back to the unrelated standalone `Arial Bold.ttf` — a thinner-looking family than the (actually-bold) "regular" — making the two visually indistinguishable, sometimes reversed (confirmed empirically: bold glyph bitmaps measured NARROWER than "regular" for the same character).
**Fix**: `FontCache::system_ui()` now reads each collection's name table (new direct `ttf-parser` dependency, already present transitively via `fontdue`) and scores every face for "closest to true Regular" / "closest to true Bold" (`weight_score`), picking the best-matching index for each — preferring a same-family bold face found inside the SAME collection file over the old unconditional `BOLD_PATHS` fallback, which now only fires when the chosen family genuinely has no bold member of its own (e.g. plain `Arial.ttf`, which needs the separate `Arial Bold.ttf`). Verified via a scratch probe (`glyph_weighted` for regular vs bold, both now correctly same-family and bold consistently wider) and a live before/after screenshot of `markdown_editor_demo` showing real, visible weight contrast.
**Lesson**: this is a second instance of the same discipline as [[feedback_verify_dont_assume]] — the entire logical pipeline reading correct on paper (and every unit test passing, because `FontCache::embedded()` used by all headless tests has no bold face at all and silently no-ops the whole question) hid a real bug that only a live screenshot surfaced. Headless `FrameEngine` tests cannot catch a `system_ui()`-specific font-loading bug by construction.
**Affects**: `rosace-render` (`font.rs`'s `system_ui()`, new `ttf-parser` direct dependency).
**Relates to**: D116 (the span/weight pipeline this sits underneath, which was already correct), [[feedback_verify_dont_assume]].

---

### D118 — Window resize on macOS was async-only (`request_redraw()`), causing live-resize ghosting/stale-frame stretching; now draws synchronously on every `Resized` tick (found + fixed 2026-07-12)

**Bug**: user-reported live (screenshots + a real screen-recording burst while dragging) — during an active window-resize drag, `markdown_editor_demo` showed ghosted/duplicated panel content, uninitialized-looking pixels bleeding through at the growing edge, and toolbar text visibly detached from its own button background for a frame. Root cause: `rosace-platform/src/app.rs`'s `WindowEvent::Resized` handler reconfigured the wgpu surface (`presenter.resize()`) then only called `window.request_redraw()` — which merely *schedules* a `RedrawRequested` for the next winit event-loop turn. On macOS, a live window-drag runs inside AppKit's own nested tracking runloop; until the app actually submits a new GPU frame, the OS compositor just stretches the LAST presented frame to cover the window's current (already-changed) size. `request_redraw()`'s scheduled callback can lag many resize ticks behind that nested loop, so what the user saw during the drag was largely the previous frame being stretched/exposed, not a genuinely broken layout or paint (both were re-verified correct once a real frame landed).
**Also fixed in the same pass** (found while reproducing the above): `rosace-compositor`'s `GpuPresenter::present_frame` called `self.surface.get_current_texture()` deep in the middle of the function, AFTER already committing that frame's `cached_glyph_batches`/`cached_images`/`cached_backdrops` (the D089 skip-present bookkeeping) — and silently `return`ed on any acquire failure (routine during a live resize: `Outdated`/`Lost`). That meant a frame could be fully prepared, its state marked "presented," but never actually drawn — and the skip-present fast path would then treat the NEXT frame as unchanged too, leaving content (observed: text glyphs specifically) missing until something else forced a diff.
**Fix, part 1 (necessary but NOT sufficient — the user re-reproduced after these landed)**: (1) `app.rs`'s `Resized` handler now calls a new `AppState::redraw()` method — the `RedrawRequested` body, extracted verbatim — synchronously, in addition to still requesting a follow-up async redraw. (2) `present_frame` now acquires the surface texture (with an `Outdated`/`Lost` → reconfigure-and-retry-once path, the idiomatic wgpu pattern) BEFORE committing any skip-present cache state; a failed acquire now bails out before touching `cached_*`, so a dropped frame is just a dropped frame — the next `present_frame` call sees the real diff and draws it, instead of the state believing a draw happened that didn't.
**Fix, part 2 (the actual root cause of the user's symptom — "text doesn't move with resize, backgrounds do")**: `GpuPresenter::resize()` reconfigures the surface but left every D089 skip/reuse cache intact, and those caches bake in TWO assumptions a reconfigure breaks: (a) *"the swapchain already shows the last frame"* — false after reconfigure (contents undefined), so a content-identical next frame (window grown at the bottom-right, content anchored top-left — nothing changed in px) passed every `*_unchanged` comparison and SKIPPED the present entirely, showing garbage/blank ("sometimes a white panel"); and (b) **`sync_cached_quads` only rewrites a quad's placement uniform when its RECT changes** — but `shader_quad_uniform(rect, sw, sh)` bakes the SURFACE SIZE into the px→NDC mapping, so after a resize every quad whose pixel rect was unchanged rendered scaled/shifted through the old mapping, while the glyph pipeline (whose globals buffer IS keyed on surface size and rewritten) rendered at correct positions — shape quads (button fills, panel backgrounds) and text glyphs visibly disagreeing with each other in a single frame, which is exactly what the user's screenshots showed. Fixed by making `resize()` clear `cached_quads`/`cached_layers`/`cached_glyph_batches`/`cached_images`/`cached_backdrops` (compare-metadata + surface-size-dependent uniforms; the image TEXTURE cache and glyph atlas are untouched, so the cost is one full redraw, not re-uploads). Offscreen scroll layers needed nothing: their placed uniform was already recomputed against the live surface size every draw.
**Diagnostic path worth remembering**: the mixed-generation frame (stale quads + fresh glyphs) was only isolatable because quads and glyphs take different uniform paths — temporary per-frame glyph-batch logging (`RUST_LOG=debug`) proved the ENGINE was emitting correct new-layout items every frame, which pinned the staleness inside the compositor's caches rather than the paint/layout pipeline.
**Verified**: user re-tested live resize-dragging after part 2 and confirmed fixed ("yes it worked") — text now tracks its backgrounds through grow/shrink drags.
**Affects**: `rosace-platform` (`app.rs`: `WindowEvent::Resized`, new `AppState::redraw()`), `rosace-compositor` (`present_frame`'s surface-acquisition ordering; `resize()`'s cache invalidation).
**Relates to**: D109 (the GPU-shapes/compositor architecture this operates inside), D089 (the skip-present optimization whose hidden surface-size assumptions this exposed), [[feedback_verify_dont_assume]] (this whole bug was only found via live capture, never from reading the resize pipeline's code — which looked structurally correct on paper).

---

### D119 — `TextArea` had no visible scroll-position indicator (found + fixed 2026-07-12)

**Bug**: user-reported live — `TextArea` scrolls (via the same `ScrollController`/wheel pipeline `ScrollView`/`ListView` use, D101) but painted no thumb, so a user with content taller than the viewport had no visual cue there was more to scroll to, or where they were in it.
**Fix**: ported `ScrollView`'s existing vertical-thumb convention (D108-era code, `scroll_view.rs`) into `text_area.rs` verbatim in spirit — drawn AFTER `PopClip` (so the thumb itself isn't clipped by the text viewport), re-reading the scroll offset fresh at draw time rather than the value captured earlier in `paint()` (same "don't lag a frame behind scroll-into-view" reasoning `ScrollView` already documents). New `TextArea::no_scrollbar()`/`::scrollbar_color()` builder methods, same names/shape as `ScrollView`'s.
**Verified**: live screenshot of `markdown_editor_demo` (extended with enough lines to overflow the viewport) shows a real thumb tracking the visible-content ratio.
**Follow-up bug found immediately after, same day (clipped last line)**: with a working scrollbar, actually scrolling to the bottom exposed a D116-Step-4-era geometry bug — `max_scroll` was computed from bare `content_h` (`n_lines * line_h`) while lines are DRAWN at `PAD + i*line_h - scroll_y`, so at max scroll the last line's bottom sat exactly `PAD` (10px) past the clip, permanently half-cut. Fixed by making the scrollable extent `content_h + PAD*2` everywhere it's used (max_scroll, the controller's `content_size`, the scrollbar's ratio/position, the caret scroll-into-view bottom branch — which now clears the bottom pad too — and `first_visible`, which now subtracts PAD so a line poking into the top padding band still paints). Pinned by a headless regression test (`scrolled_to_the_bottom_the_last_line_is_fully_inside_the_viewport`) asserting both the exact clamped offset and the geometric truth (last line bottom ≤ viewport height).
**Also raised in the same report, deliberately NOT pulled forward**: the user asked for rendered markdown (markers hidden) — that's Phase 32 Step 3's `RichText`/`TextSpan` wiring; the user chose to keep the queue order (Phase 29 next). The concrete use-case is now logged in `PHASE_32.md` Step 3.
**Second follow-up bug, same day (scroll-into-view fought the wheel)**: user-reported — "when the cursor is below half the scroll is not working, when it is bottom no scrolling." Root cause: the D116-Step-4 caret scroll-into-view ran on EVERY focused paint, and a focused `TextArea` repaints every frame (caret blink `request_animation()`), so any wheel-scroll away from the caret was snapped back within one frame — a caret on the bottom line made upward scrolling impossible, and a mid-document caret clamped the reachable scroll range to a viewport-sized window around itself. Real editors chase the caret only when it MOVES. Fixed with a new `TextEditState::scrolled_cursor: Option<usize>` view-state field (the position the view last chased): the widget compares `cursor()` against it each paint, chases + records only on a real change (typing, arrows, click). Because `PaintCtx::text_edit()` returns a clone (engine-owns-mutation convention), a deliberately-scoped `PaintCtx::set_scrolled_cursor()` is the ONE sanctioned widget-side write into `text_edit` — documented as view state alongside `scroll_x`, and carried through the three field-by-field `TextEditState` constructors (`apply_and_record`, `undo`, `redo`). Pinned by `wheel_scrolling_away_from_the_caret_is_not_snapped_back_by_scroll_into_view`: wheel-to-top with caret at bottom must STICK across a blink frame, and one typed char must chase back to the bottom.
**Affects**: `rosace-widgets` (`text_area.rs`: two new fields + builder methods, scrollbar paint block, scroll-extent geometry), `rosace` (`engine.rs`: regression test).
**Relates to**: D101 (`ScrollController`, the shared scroll-state primitive), D116 (`TextArea` itself), D115/Phase 32 Step 3 (the rendered-preview ask this surfaced).

---

### D120 — The C ABI drops the Tezzera-era `tzr` prefix for `rsc` (user-decided 2026-07-14, mid-Phase-29)

**Status**: LANDED 2026-07-14.
**Decision**: Every FFI-boundary name renames `tzr` → `rsc` in all three casings: `tzr_engine_*` symbols → `rsc_engine_*`, `TZR_EVENT_*`/`TZR_KEY_*`/`TZR_KEYBOARD_*`/`TZR_BUTTON_*` constants → `RSC_*`, `TzrEngine`/`TzrInputEvent`/`TzrInputEventFfi` types → `Rsc*`, and `include/tzr_engine.h` → `include/rsc_engine.h`. Internal stragglers (`read_tzr_toml_bundle_id`, cli test temp-dir prefixes) rename too — no mixed prefix survives outside historical decision text.
**Reason**: The user asked "are we still using tzr instead of rsc?" during Phase 29 Step 1 — audit confirmed the entire C ABI still carried the old Tezzera prefix with NO decision backing it (nothing in NAMING.md/DECISIONS.md — legacy, not choice), while the CLI (`rsc`), config (`rsc.toml`), and crates (`rosace-*`) had all moved on. Renamed NOW because it's the cheapest it will ever be: pre-1.0, every native host is an `rsc new`-generated template (regenerate to migrate), and each new capability (the lifecycle events landed the same day) would otherwise bake more `TZR_` into app-side Swift/Kotlin.
**Migration**: previously scaffolded apps regenerate their host projects with `rsc new` (or hand-rename the same three casings). No compat aliases kept — pre-1.0, sole consumer is the generated templates.
**Historical text is NOT rewritten**: decision entries D106–D119 and phase docs keep their `TZR_*` references — they accurately describe what landed at the time; this entry is the rename record.
**Affects**: `rosace-ffi` (header + all symbol/type/constant names), `rosace-cli` (generated Swift/Kotlin/Rust templates, internal fn names), `rosace-examples` (demo_app ffi, stubs), doc comments across `rosace-core`/`rosace-platform`/`rosace`.
**Relates to**: D106 (the FFI bridge being renamed), D110 (the phase this interrupted — its lifecycle constants shipped under `RSC_` from the first commit that reaches a generated app).

---

### D121 — `#[persist]` re-homed onto the hook model: `ctx.state_permanent(key, default)` (decided 2026-07-15, Phase 31 Step 2)

**Status**: LANDED with Phase 31 Step 2.
**Decision**: D008's persistence tiers are implemented on the API apps actually use — the `ctx.state(..)` hook model — not the `#[state]` field-attribute world D008 assumed (that macro predates the hook model and has zero real call sites; building `#[persist]` on it would be the exact built-but-never-wired antipattern the roadmap audit keeps finding). Concretely: `Context::state_permanent(key, default)` = a `ctx.state` whose first initialization reads `key` from the installed persist backend and whose every later `set` writes through (via `Atom`'s single `on_change` slot, now claimed by persistence — it was test-only before). A `PersistBackend` trait + global install slot lives in `rosace-core` (`set_persist_backend`), so core/state stay SQLite-free; `rosace-storage` implements the trait and `App::launch` installs it pointed at the platform app-data dir. Values implement a small `PersistValue` (to/from bytes) trait — impls for the primitives + `String` + `Vec<u8>`; a serde-based general impl is a named deferral (new dependency = its own decision, not smuggled in here). The `reload`/`session` tiers are documented no-ops today by construction (atoms already live for the whole process; hot reload doesn't exist yet — D102), NOT silently claimed implemented. `#[persist(...)]` attribute syntax remains reserved as future sugar if the field-macro world returns.
**Affects**: `rosace-core` (persist.rs, `Context::state_permanent`), `rosace-state` (`Atom::set_on_change` made `pub`), `rosace-storage` (backend impl), `rosace` (`App::launch` installs the backend).
**Relates to**: D008 (the tiers), D114/Phase 31 (the phase), D102 (hot reload, which the `reload` tier waits on).

---

## DEFERRED DECISIONS

```
D-DEF-001  ROSACE Studio design          → Phase 4
D-DEF-002  Wide color / HDR              → Phase 3
D-DEF-003  2D bidirectional scroll       → Phase 3
D-DEF-004  Plugin registry governance    → SCOPED as D109/Phase 27 (rosace-shader)
D-DEF-005  OS version review at v1.0     → v1.0
D-DEF-006  RustRover plugin              → Phase 3b
D-DEF-007  Package manager              → Phase 4
D-DEF-008  Server-side rendering        → not planned
D-DEF-009  React Native interop         → not planned
D-DEF-010  Embedded/no-std             → Phase 5
D-DEF-011  Web GPU presenter            → own phase after 27; wgpu-on-wasm
                                          (WebGPU) replaces web.rs's
                                          putImageData path; GATES the
                                          final tiny-skia removal (see
                                          PHASE_27.md Out of Scope)
D-DEF-012  Backdrop blur / glassmorphism → IMPLEMENTED 2026-07-11:
                                          DrawCommand::BackdropBlur →
                                          scene-texture indirection +
                                          half-res separable Gaussian +
                                          glass pass (see PHASE_27.md
                                          checklist; glass_demo verified)
```
