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
**Status**: LOCKED
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
**Decision**: Skia via skia-safe crate. Pixel-perfect, identical across platforms.
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
**Status**: LOCKED
**Decision**: GlobalAtom<LifecycleState> + use_app_lifecycle() hook.
States: Active, Inactive, Background, Suspended.
**Affects**: rosace-platform

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

## TESTING

### D053 — Golden Files
**Status**: LOCKED
**Decision**: Per-platform golden files. tests/goldens/desktop|mobile|web/
Threshold: 0%=pass, <1%=warn, >1%=fail. Configurable per test.
**Affects**: rosace-test

---

## DEFERRED DECISIONS

```
D-DEF-001  ROSACE Studio design          → Phase 4
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
