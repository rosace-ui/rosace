# ROSACE — NAMING CONVENTIONS
> Names are decided once. Never changed after Phase 1 ships.
> Renaming is expensive. Get it right now.

---

## FRAMEWORK IDENTITY

```
Framework name:   ROSACE (all caps in prose, TitleCase in code)
Pronounced:       TEZ-er-a
CLI binary:       rsc
Crate prefix:     rosace-
Rust namespace:   rosace::
Error prefix:     T (T001, T002...)
Docs domain:      rosace.dev
Error docs:       rosace.dev/errors/T{number}
```

---

## RUST NAMING RULES

### Types (PascalCase)
```rust
RosaceComponent
RosaceResult
RosaceError
RosaceTrace
RosaceTheme
RosaceRenderer
RosaceApp
AsyncState
AxisBound
RenderObject
SemanticNode
ChildContainer
```

### Traits (PascalCase)
```rust
RosaceComponent   // implement this to make a component
RenderObject       // implement this for custom render
RosaceRenderer    // implement this for custom pipeline
WidgetOverride     // implement this to override a widget
TraceSubscriber    // implement this for custom tracing
```

### Functions and methods (snake_case)
```rust
use_atom()
use_async()
use_async_lazy()
use_async_when()
use_async_all!()
use_stream()
use_locale()
use_route()
use_before_leave()
use_back_handler()
use_network_status()
use_sensor()
use_app_lifecycle()
use_bloc_listener()
batch()
batch_async()
on_mount()
on_update()
on_unmount()
```

### Macros (snake_case)
```rust
atom!()
trace!()
batch!()
render!()
render_constrained!()
assert_text!()
assert_size!()
tap!()
snapshot!()
```

### Constants and statics (SCREAMING_SNAKE_CASE)
```rust
static THEME: GlobalAtom<Theme> = atom!(Theme::Dark);
static USER: GlobalAtom<Option<User>> = atom!(None);
static APP_LIFECYCLE: GlobalAtom<LifecycleState>;
static TRACING_BUS: TracingBus;
static LOCALE: GlobalAtom<Locale>;
```

### Enums (PascalCase variants)
```rust
enum AsyncState<T> {
    Idle,
    Loading,
    Success(T),
    Error(RosaceError),
    Refreshing(T),
}

enum AxisBound {
    Bounded(f32),
    Unbounded,
    Shrink,
}

enum Priority {
    Immediate,
    Normal,
    Background,
}

enum NavigationDecision {
    Allow,
    Block,
    RedirectTo(AppRoute),
}

enum BackHandlerResult {
    Pop,
    Block,
}

enum PermissionStatus {
    Granted,
    Denied,
    PermanentlyDenied,
}

enum LifecycleState {
    Active,
    Inactive,
    Background,
    Suspended,
}
```

---

## WIDGET NAMING

### Built-in widgets (PascalCase, descriptive)
```
Text
RichText
SelectableText
TextInput
Button
IconButton
FloatingActionButton
Image
Icon
Column
Row
Stack
Grid
Flex
Wrap
Spacer
SizedBox
AspectRatio
Expanded
FractionallySizedBox
IntrinsicHeight
IntrinsicWidth
IntrinsicSize
ScrollView
VirtualList
Scaffold
AppBar
BottomNavigationBar
Card
Dialog
AlertDialog
BottomSheet
Tooltip
Snackbar
Chip
Badge
Avatar
Skeleton
Empty
ErrorBoundary
KeepAlive
AtomProvider
CustomPaint
AnimatedContainer
FocusScope
Directionality
Overlay
Modal
Suspense
```

### Widget modifier methods (snake_case, verb or property)
```rust
.child()
.children()
.builder()
.child_if()
.prepend()
.append()
.key()
.padding()
.padding_start()
.padding_end()
.padding_left()
.padding_right()
.margin()
.width()
.height()
.background()
.foreground()
.border()
.border_radius()
.shadow()
.opacity()
.scale()
.rotate()
.clip()
.animate()
.disabled()
.on_press()
.on_long_press()
.on_hover()
.on_change()
.on_submit()
.on_scroll()
.semantic_label()
.semantic_role()
.semantic_hint()
.semantic_hidden()
.mirror_in_rtl()
.ignore_safe_area()
.loading()
.error()
.cache()
.fit()
.placeholder()
.restore_position()
.restoration_key()
.sticky_headers()
.pull_to_refresh()
.keep_alive()
.physics()
.physics()
.keyboard_avoid()
.scroll_anchor()
.on_end_reached()
.end_threshold()
.estimated_item_height()
.overscan()
.max_lines()
.overflow()
.line_break()
.direction()
.align()
.align_self()
.hover_style()
.pressed_style()
.disabled_style()
.transition()
.duration()
.curve()
.repaint_when()
.override_widget()
.provide()
.atom()
.theme()
.plugin()
.locale()
.locales()
.default_locale()
.locale_dir()
.color_space()
.font()
```

---

## FILE NAMING

### Rust source files (snake_case)
```
component.rs
element.rs
render_object.rs
semantic_node.rs
context.rs
child_container.rs
error_boundary.rs
atom.rs
global_atom.rs
atom_provider.rs
refresh_engine.rs
async_atom.rs
tracing_bus.rs
rosace_trace.rs
flexure.rs
constraints.rs
text_layout.rs
overlay.rs
scroll_view.rs
virtual_list.rs
navigator.rs
stack_navigator.rs
tab_navigator.rs
```

### Test files
```
{module}_test.rs      or    tests/{module}.rs
```

### Golden files
```
tests/goldens/{platform}/{widget}_{state}.png

examples:
tests/goldens/desktop/button_default.png
tests/goldens/desktop/button_disabled.png
tests/goldens/mobile/button_default.png
tests/goldens/web/button_default.png
```

---

## ATTRIBUTE NAMING

```rust
#[component]           // define component
#[state]               // local atom
#[derived]             // derived atom
#[global_state]        // global atom
#[scoped_state]        // scoped atom
#[async_state]         // async atom
#[persist(reload)]     // persist level
#[persist(session)]
#[persist(permanent)]
#[persist(permanent, encrypted)]
#[no_persist]          // block persistence
#[lazy]                // lazy component
#[eager]               // eager component
#[lazy(group = "...")]  // lazy group
#[routes]              // route enum
#[route("/path")]      // route attribute
#[query_param("q", field = "query")]
#[rosace_ffi(c)]      // FFI bridge
#[rosace_ffi(swift)]
#[rosace_ffi(kotlin)]
#[rosace_ffi(js)]
#[rosace_export(js)]  // export to JS
#[rosace_test]        // test function
#[rosace_snapshot]    // snapshot test
#[rebuild_budget(max_per_second = 60)]
```

---

## ERROR NAMING

All compiler errors: T{zero-padded-3-digit-number}
```
T001 — Missing required handler
T002 — Impossible layout
T003 — Dynamic list without keys
T004 — Lifecycle hook not at top level
T005 — Scoped atom outside provider
T006 — Non-serializable persisted atom
T007 — Circular derived atoms
T008 — Partial theme
T009 — Intrinsic inside unbounded scroll
T010 — Lazy component missing loading state
```

Format in compiler output:
```
error[T001]: Missing required handler
  --> src/ui/home.rs:14:5
   |
14 |     Button::new("Submit")
   |     ^^^^^^^^^^^^^^^^^^^^^
   |
   = Button requires .on_press() when interactive
   hint: add .on_press(|| { ... }) or use .disabled(true)
   docs: rosace.dev/errors/T001
```

---

## THINGS THAT ARE NEVER RENAMED

Once Phase 1 ships, these names are frozen:
```
rsc (CLI binary)
rosace (namespace)
Atom<T>
RosaceComponent
RosaceTrace
on_mount / on_update / on_unmount
use_atom
use_async
batch
ErrorBoundary
RenderObject
SemanticNode
Flexure (internal engine name)
```
