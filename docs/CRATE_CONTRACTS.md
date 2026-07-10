# ROSACE — CRATE CONTRACTS
> Each crate has one job. It does that job and nothing else.
> Dependencies only flow downward.
> A crate never reaches into another crate's internals.

---

## DEPENDENCY HIERARCHY

```
rosace-macros          (no rosace deps)
rosace-trace           (no rosace deps)
rosace-core            (← trace, macros)
rosace-state           (← core, trace)
rosace-layout          (← core, state, trace)
rosace-render          (← core, layout, trace)
rosace-animate         (← core, state, render, trace)
rosace-scroll          (← core, state, layout, render, trace)
rosace-nav             (← core, state, render, animate, trace)
rosace-platform        (← core, state, trace)
rosace-ffi             (← core, trace)
rosace-theme           (← core, state, layout, trace)
rosace-widgets         (← all above)
rosace-test            (← all above)
rosace-devtools        (← trace, core, state)
rosace-cli             (← all above)
```

**Rule**: If crate A is above crate B in this hierarchy,
A cannot depend on B. Ever.

---

## CRATE CONTRACTS

---

### rosace-macros
**Job**: Provide all proc macros. Nothing else.
**Exports**:
- #[component] — define a component
- #[state] — local atom declaration
- #[derived] — derived atom
- #[persist(...)] — atom persistence
- #[global_state] — global atom
- #[scoped_state] — scoped atom
- #[async_state] — async atom
- #[routes] — route enum
- #[route("...")] — route attribute
- #[lazy] / #[eager] — loading strategy
- #[rosace_ffi(...)] — FFI bridge
- #[rosace_test] — test macro
- #[rosace_snapshot] — snapshot test
- atom!() — atom constructor macro
- trace!() — trace emission macro
- batch!() — batch macro

**Must NOT**:
- Contain any runtime logic
- Import any rosace-* crate
- Contain UI logic of any kind

---

### rosace-trace
**Job**: Define RosaceTrace event type, TracingBus, and all subscribers.
**Exports**:
- RosaceTrace enum
- TracingBus struct
- TraceSubscriber trait
- RingBufferSubscriber
- ConsoleSubscriber
- FileSubscriber
- DevToolsSubscriber (transport only)
- TraceProtocol (serialization)
- TRACING_BUS global

**Must NOT**:
- Contain any UI logic
- Import any rosace-* crate (except macros)
- Do any rendering or layout

**Rule**: Every other crate depends on this.
It must be lean, fast, and stable.

---

### rosace-core
**Job**: Define the component model, element tree, and lifecycle.
**Exports**:
- RosaceComponent trait
- Element type
- RenderObject trait
- SemanticNode type
- Context struct
- ChildContainer trait
- ErrorBoundary widget
- RosaceResult, RosaceError
- Lifecycle hooks: on_mount, on_update, on_unmount
- Key type
- ComponentId, AtomId types
- RosaceApp builder

**Must NOT**:
- Know about specific widgets (Button, Text etc.)
- Implement any layout algorithms
- Touch Skia or rendering
- Know about navigation routes

---

### rosace-state
**Job**: Implement the atom system, reactivity, and refresh engine.
**Exports**:
- Atom<T> struct
- GlobalAtom<T> struct
- AtomProvider widget
- use_atom() hook
- use_async() family
- use_stream()
- batch() / batch_async()
- RefreshEngine
- Priority enum
- AsyncState<T> enum
- Derived atom support

**Must NOT**:
- Know about layout or rendering
- Import rosace-layout or rosace-render
- Contain any widget implementations

---

### rosace-layout
**Job**: Implement the Flexure constraint layout engine and text layout.
**Exports**:
- Constraints struct
- AxisBound enum
- LayoutResult struct
- Flexure engine
- All layout widgets: Column, Row, Stack, Grid, Flex, Wrap,
  Spacer, SizedBox, AspectRatio, Expanded, FractionallySizedBox
- IntrinsicHeight, IntrinsicWidth, IntrinsicSize
- Width / Height sizing enums
- Alignment, Baseline alignment
- Overlay system (layers 0-5)
- Directionality
- Text layout via cosmic-text + HarfBuzz + fontdue
- RTL support

**Must NOT**:
- Call Skia directly
- Know about navigation
- Know about animation
- Import rosace-render

---

### rosace-render
**Job**: Bridge between layout engine and Skia. Manage GPU layers, dirty regions, frame rendering.
**Exports**:
- SkiaCanvas wrapper
- RenderPipeline
- LayerCompositor
- DirtyRegionTracker
- CustomPaint widget
- Image handling (Image::network, Image::asset etc.)
- ImageCache (memory + disk)
- SemanticTree builder
- Platform accessibility bridges
- RosaceRenderer trait (custom pipelines)

**Must NOT**:
- Implement layout algorithms
- Know about navigation
- Know about animation state

---

### rosace-animate
**Job**: Implement all animation systems.
**Exports**:
- Animation (timeline)
- AnimatedContainer
- Implicit animation (.animate() modifier)
- SpringAnimation
- DragAnimation
- Curve enum
- Transition types
- SharedElement transition

**Must NOT**:
- Touch Skia directly (use rosace-render)
- Know about navigation routes
- Implement layout

---

### rosace-scroll
**Job**: Implement scrolling, virtualization, and gesture arbitration.
**Exports**:
- ScrollView widget
- VirtualList widget
- ScrollController
- ScrollPhysics
- GestureArbitrator
- PullToRefresh
- StickyHeader support
- ScrollAnchor
- KeyboardAvoidBehavior

**Must NOT**:
- Implement navigation
- Touch Skia directly
- Implement layout algorithms

---

### rosace-nav
**Job**: Implement routing, navigation, and transitions.
**Exports**:
- Navigator
- StackNavigator
- TabNavigator
- DrawerNavigator
- AppRoute traits
- NavigationDecision enum
- use_before_leave() hook
- use_back_handler() hook
- use_route() hook
- NavigationGuard
- Transition types
- KeepAlive widget
- Deep link handling
- Web URL sync

**Must NOT**:
- Implement animations from scratch (use rosace-animate)
- Know about specific screen content
- Implement scroll behavior

---

### rosace-platform
**Job**: Provide platform-specific APIs in a unified interface.
**Exports**:
- Platform struct
- Permission API
- App lifecycle (LifecycleState atom)
- Haptics API
- Safe area insets
- File picker, camera, share, clipboard
- Biometrics
- Notifications
- PlatformChannel
- use_network_status()
- use_file_watch()
- use_sensor()
- use_app_lifecycle()
- Localization (LOCALE atom, use_locale())

**Must NOT**:
- Implement UI widgets
- Know about navigation routes
- Implement rendering

---

### rosace-ffi
**Job**: Provide all FFI bridges and synchronous platform bridge.
**Exports**:
- C FFI macros and safe wrappers
- Swift FFI bridge
- Kotlin FFI bridge
- JS/WASM FFI bridge
- sync_bridge module
- SharedMemory
- ForeignBox
- KYRA_CHANNEL (FFI to UI thread channel)
- catch_unwind wrappers

**Must NOT**:
- Contain UI logic
- Import rosace-widgets
- Implement platform APIs (use rosace-platform)

---

### rosace-theme
**Job**: Implement the theme system, design tokens, and localization files.
**Exports**:
- RosaceTheme derive macro support
- ThemeData struct
- TextStyle struct
- SpacingScale
- ColorScheme
- BorderRadius types
- Shadow types
- Locale system
- Translation file loading (TOML)

**Must NOT**:
- Implement rendering
- Know about specific widgets
- Implement layout

---

### rosace-widgets
**Job**: Implement the official widget library.
**Exports**: All built-in widgets:
- Text, RichText, SelectableText, TextInput
- Button, IconButton, FloatingActionButton
- Image
- Icon
- Column, Row, Stack (re-exported from layout)
- Scaffold, AppBar, BottomNavigationBar
- Card, Dialog, AlertDialog, BottomSheet
- ListTile, Divider
- Checkbox, Switch, Slider, Radio
- TextField, Form
- CircularProgressIndicator, LinearProgressIndicator
- Tooltip, Snackbar
- Chip, Badge
- Avatar
- Skeleton
- Empty, Spacer (re-exported)
- AdaptiveButton (platform-adaptive)

**Must NOT**:
- Implement any framework internals
- Bypass the RenderObject system
- Import rosace-core internals directly

---

### rosace-test
**Job**: Provide testing utilities and snapshot testing.
**Exports**:
- render!() macro
- render_constrained!() macro
- assert_text!() macro
- assert_size!() macro
- tap!() macro
- snapshot!() macro
- Golden file comparison
- Per-platform test runners

**Must NOT**:
- Ship in production builds
- Import production-only crates

---

### rosace-devtools
**Job**: Implement the dev tools server and hot reload system.
**Exports**:
- DevToolsServer
- HotReloadWatcher
- TimeTravel
- DevToolsTransport (WebSocket + shared memory)
- Frame profiler
- Component tree serializer

**Must NOT**:
- Ship in production builds (#[cfg(debug_assertions)] everything)
- Import production-only crates in release

---

### rosace-cli (rsc)
**Job**: Implement the rsc command-line tool.
**Commands**:
- rsc dev [--trace=...] [--profile] [--time-travel]
- rsc build --target [web|desktop|ios|android|all]
- rsc build --web-routing=[hash|history]
- rsc test [--update-goldens] [--platform=...]
- rsc analyze
- rsc snapshot --update

**Must NOT**:
- Contain framework logic
- Be imported by other crates

---

## VIOLATION POLICY

If any crate violates its contract:
1. Do not merge
2. Redesign the boundary
3. Update this document if the contract needs adjusting
4. Never just add the import and move on
