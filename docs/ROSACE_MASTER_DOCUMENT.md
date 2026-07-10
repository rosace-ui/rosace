# ROSACE — Master Architecture Document
> Version 0.1 — Pre-code planning document  
> Status: Architecture complete, ready for Phase 1 implementation

---

**Fast by nature. Beautiful by design.**

---

## TABLE OF CONTENTS

1. Vision & Philosophy
2. Core Principles
3. The Complete Stack
4. Component Model
5. State System
6. Layout Engine
7. Scroll & Virtualization
8. Navigation
9. Animation
10. Rendering
11. UI Customization
12. Observability & Tracing
13. Dev Tools
14. Platform Integration
15. FFI Layer
16. Theming
17. Testing
18. Build System
19. Crate Structure
20. Phase Roadmap
21. Open Decisions (deferred)
22. Appendix A — Compiler Error Reference
23. Appendix B — Glossary

---

## 1. VISION & PHILOSOPHY

### Identity
```
Name:        ROSACE
Pronounced:  TEZ-er-a
Meaning:     Mosaic tile — simple alone, beautiful together
Tagline:     Fast by nature. Beautiful by design.
Language:    Rust
CLI:         rsc
```

### One Sentence
> Write once in pure Rust, run everywhere at 120fps, see changes instantly, ship with confidence.

### The Golden Rule
```
Strict underneath.
Invisible on top.
Efficient always.
Correct by construction.
```

### What ROSACE Is
ROSACE is a declarative UI framework built in Rust targeting desktop, web (WASM), iOS, and Android from a single codebase. It draws the best ideas from Flutter, Jetpack Compose, SwiftUI, and React — then improves on all of them using Rust's type system and ownership model.

Like a mosaic — every component is a tessera tile. Simple, precise, and self-contained. Together they form something beautiful.

### What Makes ROSACE Different
- **Compile-time UI correctness** — invalid layouts, missing handlers, type mismatches caught before running
- **Rust ownership as a feature** — state corruption is architecturally impossible
- **Observability by design** — every system is traceable from day one
- **Strict but invisible** — framework enforces rules so users never hit runtime surprises
- **Maximum customization** — from theme tokens to custom render pipelines
- **JIT in dev, AOT in production** — instant feedback during development, maximum performance when shipping

### The Mosaic Metaphor
```
Component  = one tile
App        = the mosaic
Every tile fits perfectly
Together   = a beautiful complete picture

Simple alone.
Magnificent together.
```

---

## 2. CORE PRINCIPLES

### Delivery & Experience
1. Deliver simplicity — one right way per problem
2. Stricter DSL — fewer footguns, more guardrails
3. JIT via WASM hot-swap in dev, native AOT in production
4. Dev tools built-in, not an afterthought
5. Compile errors are UX — actionable, friendly, linkable

### Architecture
6. Flutter-like tree: Component → Element → RenderObject → Semantic
7. Constraint-based layout — no undefined layout behavior
8. Skia rendering — pixel-perfect, identical across all platforms
9. Dirty region tracking — repaint only what changed
10. Layer compositing — GPU-cached static layers
11. Smart refresh engine — rebuild minimum subtree always

### Syntax & DX
12. SwiftUI-like modifiers — chainable, readable, composable
13. Pure Rust first — proc macro sugar on top
14. Zero runtime overhead — macros compile away completely in AOT
15. Compile-time UI correctness — invalid states caught before run

### State
16. Atom primitive — reactive value, automatic subscriber notification
17. Three scopes: local, provided (tree-scoped), global
18. Fine-grained reactivity — only atom subscribers rebuild
19. State time-travel in dev mode
20. No mutation surprises — borrow checker enforced

### Cross Platform
21. One codebase — desktop, web, mobile
22. Consistent pixels — Skia guarantees identical output
23. Platform adapters — native feel where needed
24. Safe area insets — first class, edge to edge by default

### Performance
25. Parallel layout — safe via Rust ownership
26. AOT dead code elimination — smallest possible binary
27. 120fps target — no frame budget compromises
28. Memory safety guaranteed — no GC pauses ever

### Observability
29. Every system emits traces — non-negotiable
30. Zero cost in production — #[cfg(debug_assertions)] gates everything
31. Tracing is architecture, not a feature

### Accessibility
32. Semantic tree built always — not optional
33. Screen readers, keyboard navigation, focus management — core
34. RTL and BiDi — day one, automatic

### Ecosystem
35. Plugin system — trait-based, versioned, official registry
36. Official widget library — batteries included
37. Package registry for community components
38. IDE tooling roadmap — VS Code, RustRover, ROSACE Studio

---

## 3. THE COMPLETE STACK

```
┌─────────────────────────────────────────────────────┐
│                USER CODE (Pure Rust)                │
├─────────────────────────────────────────────────────┤
│             PROC MACRO SUGAR LAYER                  │
├─────────────────────────────────────────────────────┤
│              COMPONENT LAYER                        │
│       Component → Element → RenderObject            │
│              → SemanticNode                         │
├─────────────────────────────────────────────────────┤
│               STATE LAYER                           │
│         Atom + Provider + Tracing                   │
├─────────────────────────────────────────────────────┤
│              LAYOUT ENGINE (Flexure)                │
│       Constraint Solver + Intrinsic + RTL           │
├─────────────────────────────────────────────────────┤
│           SCROLL & GESTURE LAYER                    │
│      Virtualization + Physics + Arbitration         │
├─────────────────────────────────────────────────────┤
│             NAVIGATION LAYER                        │
│        Router + Stack + Guards + Deep Link          │
├─────────────────────────────────────────────────────┤
│             ANIMATION LAYER                         │
│       Timeline + Physics + Transitions              │
├─────────────────────────────────────────────────────┤
│             OBSERVABILITY LAYER                     │
│         RosaceTrace Bus + Subscribers              │
├─────────────────────────────────────────────────────┤
│             RENDERING LAYER                         │
│      Skia + Dirty Regions + Layer Cache             │
├─────────────────────────────────────────────────────┤
│             PLATFORM LAYER                          │
│     Winit + WASM + iOS + Android Adapters           │
├─────────────────────────────────────────────────────┤
│               FFI LAYER                             │
│      C / C++ / Swift / Kotlin / JS Bridges          │
├─────────────────────────────────────────────────────┤
│              HARDWARE / OS                          │
└─────────────────────────────────────────────────────┘
```

---

## 4. COMPONENT MODEL

### The Four-Layer Tree
```
Component        ← what users write
    ↓
Element          ← lightweight immutable description
    ↓
RenderObject     ← layout + paint + hit testing
    ↓
SemanticNode     ← accessibility tree
```

### Component Identity
- **Default**: position in tree determines identity
- **Keys**: `.key(value)` available when list order can change
- Keys must be unique among siblings — compile-time check where possible, runtime warning otherwise
- Compiler warns when dynamic lists have no keys
- Moving a component without a key = destroy and recreate

### Component Definition
```rust
// Pure Rust
struct Counter {
    label: String,
}

impl RosaceComponent for Counter {
    fn build(&self, ctx: &mut Context) -> Element {
        let count = ctx.state(0);

        Column::new()
            .child(Text::new(&self.label))
            .child(Text::new(count.get().to_string()))
            .child(
                Button::new("Increment")
                    .on_press(|| count.update(|n| n + 1))
            )
            .into_element()
    }
}

// Proc macro sugar
#[component]
fn Counter(label: String) -> Element {
    #[state] let count = 0;

    Column {
        Text(label)
        Text(count)
        Button("Increment") {
            .on_press(|| count += 1)
        }
    }
}
```

### Lifecycle
Lifecycle is tree-driven. Added to tree = birth. Removed from tree = death.

```rust
#[component]
fn ChatScreen(room_id: String) -> Element {
    on_mount(|| {
        let ws = WebSocket::connect(&room_id);
        move || ws.close()  // cleanup runs on unmount
    });

    on_update(|prev| {
        if prev.room_id != room_id {
            // reconnect
        }
    });

    on_unmount(|| {
        MESSAGES.set(vec![]);
    });
}
```

**Lifecycle rules:**
- `on_mount` — fires once when added to tree
- `on_update` — fires when own props change, receives previous props
- `on_unmount` — fires once when removed from tree
- All hooks must be at component top level — compiler error if inside condition or loop
- Reactivity to atoms is always automatic, separate from lifecycle

### Component Composition
Three tiers, all supported:

**Tier 1 — Builder (always available)**
```rust
Column::new()
    .child(Text::new("First"))
    .children(vec![...])
    .builder(items.iter(), |item| ItemRow::new(item))
    .child_if(condition.then(|| Widget::new()))
    .prepend(Header::new())
```

**Tier 2 — Named slots**
```rust
Dialog::new()
    .header(Text::new("Title"))
    .body(form_content)
    .footer(action_row)
```

**Tier 3 — Macro sugar**
```rust
Dialog {
    header: { Text("Title") }
    body: { FormContent() }
    footer: { ActionRow() }
}
```

**Multi-child API (ChildContainer trait)**
```rust
.child(element)           // append single
.children(vec![...])      // append many
.builder(iter, |i| ...)   // generate from iterator
.child_if(Option<...>)    // conditional append
.prepend(element)         // insert at front
.append(&mut self, ...)   // mutable append after creation
.append_many(vec![...])   // mutable bulk append
```

Order guarantee: children render in the exact order they are added.

### Error Boundaries
Two-layer system:

**Layer 1 — RosaceResult (expected failures)**
```rust
#[component]
fn UserProfile(id: String) -> RosaceResult<Element> {
    let user = REPO.get(&id)
        .ok_or(RosaceError::not_found("User"))?;
    Ok(ProfileView::new(user))
}
```

**Layer 2 — ErrorBoundary (unexpected panics)**
```rust
ErrorBoundary::new()
    .fallback(|error| {
        ErrorView::new(error).on_retry(|| error.retry())
    })
    .on_error(|e| Analytics::log(e))
    .retry_on(RetryTrigger::Manual)
    .child(RiskyComponent::new())
```

- Errors bubble up to nearest boundary
- App-level fallback as final safety net
- Dev mode: full overlay with stack trace, file, line, hot reload to fix
- Production: clean fallback UI, silent logging

### Lazy Components
```rust
// Route components — lazy by default
// Non-route components — eager by default

#[lazy]
#[component]
fn AdminPanel() -> Element { }

#[eager]
#[route("/home")]
fn HomeScreen() -> Element { }

// At usage site
AdminPanel::new()
    .loading(|| Skeleton::new())
    .error(|e| ErrorMessage::new(e))

// Preloading
AdminPanel::preload();

// Groups
#[lazy(group = "admin")]
fn AdminPanel() -> Element {}
#[lazy(group = "admin")]
fn AdminSettings() -> Element {}
```

---

## 5. STATE SYSTEM

### The Atom Primitive
```
Atom<T> = reactive value
When changed → subscribers notified → rebuild
Nothing more. Nothing less.
```

### Three Scopes

**Local**
```rust
#[component]
fn Counter() -> Element {
    let count = use_atom(0);
    Button::new(count.get().to_string())
        .on_press(|| count.update(|n| n + 1))
}
```

**Provided (tree-scoped)**
```rust
static CART: Atom<Cart> = atom!(Cart::empty());

AtomProvider::new()
    .atom(CART, Cart::new())
    .child(CheckoutFlow::new())

// Inside provider — just works
let cart = use_atom(CART);

// Outside provider — compile error
// "CART not provided in this scope"
```

**Global**
```rust
static THEME: GlobalAtom<Theme> = atom!(Theme::Dark);
static USER:  GlobalAtom<Option<User>> = atom!(None);
// No provider needed — accessible anywhere
```

### Atom API
```rust
x.get()                    // read, auto-subscribe
x.set(value)               // write, notify subscribers
x.update(|v| v + 1)        // atomic read-modify-write

// Derived — computed, cached, auto-invalidated
#[derived]
let doubled = count.get() * 2;

// Async state
#[async_state]
let user_data = fetch_user(id).await;
```

### Atom Persistence
```rust
// Default: no persistence
let count = use_atom(0);

// Survives hot reload
#[persist(reload)]
static FORM: Atom<FormState> = atom!(FormState::empty());

// Survives app backgrounding
#[persist(session)]
static CART: Atom<Cart> = atom!(Cart::empty());

// Survives full restart
#[persist(permanent)]
static AUTH: GlobalAtom<AuthState> = atom!(AuthState::Guest);

// Security
#[no_persist]
static CREDIT_CARD: Atom<Option<CardData>> = atom!(None);

#[persist(permanent, encrypted)]
static AUTH_TOKEN: GlobalAtom<Option<String>> = atom!(None);
```

**Persistence rules:**
- Only provided and global atoms can persist
- Type must impl Serialize + Deserialize — compile error otherwise
- Type change → graceful reset, never crash
- Versioning + migration for permanent atoms

### Async Atoms
```rust
enum AsyncState<T> {
    Idle,
    Loading,
    Success(T),
    Error(RosaceError),
    Refreshing(T),
}

// Auto fetch on mount
let user = use_async(|| api::fetch_user(&id));

// Manual trigger
let results = use_async_lazy(|| api::search(&query));

// Conditional
let seller = use_async_when(
    order.is_success(),
    || api::fetch_seller(&order.data().seller_id)
);

// Parallel
let (user, feed, notifs) = use_async_all!(
    api::fetch_user(),
    api::fetch_feed(),
    api::fetch_notifications(),
);

// With options
let data = use_async(|| api::fetch())
    .cache_key(format!("user:{}", id))
    .cache_duration(Duration::minutes(5))
    .stale_while_revalidate(true)
    .refresh_on_focus(true)
    .refresh_on_reconnect(true)
    .retry(RetryPolicy::exponential(3, Duration::seconds(1)));
```

**Guarantees:**
- Race conditions impossible — latest fetch always wins
- Automatic cancellation on unmount
- No memory leaks

### Smart Refresh Engine
The heart of ROSACE's performance.

```
Step 1 — Collect all dirty components from atom changes
Step 2 — Prune descendants (if parent dirty, skip children)
Step 3 — Rebuild from minimum root set only
Step 4 — Single layout pass
Step 5 — Single paint pass
Step 6 — One frame rendered
```

**Tree index:** DFS timestamps for O(1) ancestor lookup. Incremental updates — never rebuilt from scratch.

**Guarantees:**
- Every component rebuilt at most once per frame
- Parent rebuild covers all children
- Untouched subtrees never rebuilt
- No torn frames — async changes queued for next frame

### Atom Batching
```rust
// Automatic — all sync changes batched
fn handle_refresh(data: DashboardData) {
    feed.set(data.feed);
    notifications.set(data.notifs);
    user.set(data.user);
    // → ONE rebuild
}

// Manual
batch(|| {
    update_profile();
    update_settings();
});

// Async batch
batch_async(|| async {
    let data = api::fetch().await;
    atom_a.set(data.a);
    atom_b.set(data.b);
}).await;

// Priority
atom.set_priority(value, Priority::Immediate);
atom.set(value);                               // Normal default
atom.set_priority(value, Priority::Background);
```

### External State
```rust
// Stream bridge — universal primitive
use_stream(source: impl Stream<Item = T>, atom: &Atom<T>);

// Built-in adapters
let ws = use_websocket("wss://...")
    .on_message(|msg| MESSAGES.update(|m| m.push(msg)))
    .reconnect(ReconnectPolicy::exponential());

let users = use_query(
    db::query("SELECT * FROM users"),
    |rows| rows.map(User::from_row).collect()
);

let config = use_file_watch("config.toml")
    .parse(|s| toml::from_str::<Config>(s))
    .fallback(Config::default());

let location = use_sensor(Sensor::GPS)
    .accuracy(Accuracy::High)
    .interval(Duration::seconds(5));

let network = use_network_status();

use_app_lifecycle(|event| {
    match event {
        LifecycleEvent::Background => pause_sync(),
        LifecycleEvent::Foreground => resume_sync(),
        LifecycleEvent::LowMemory  => clear_cache(),
    }
});
```

All connections automatically cleaned up on unmount.

---

## 6. LAYOUT ENGINE (FLEXURE)

### Constraint Model
```rust
struct Constraints {
    min_width:  f32,
    max_width:  AxisBound,
    min_height: f32,
    max_height: AxisBound,
}

enum AxisBound {
    Bounded(f32),
    Unbounded,
    Shrink,
}
```

### Layout Pass
1. **Measure pass** (top-down) — parent sends constraints to children
2. **Place pass** (bottom-up) — sizes bubble up, parent places children
3. **Paint pass** — walk tree, each RenderObject paints itself

### Sizing API
```rust
.width(Width::fraction(0.5))
.width(Width::fill())
.width(Width::fixed(200.0))
.width(Width::shrink())
.width(Width::min(100.0))
.width(Width::max(400.0))
.width(Width::range(100, 400))
// Same for height
```

### Built-in Layouts
```
Column, Row, Stack, Grid, Flex,
Wrap, Spacer, SizedBox, AspectRatio,
Expanded, FractionallySizedBox
```

### Intrinsic Sizing
```rust
IntrinsicHeight::new().child(Row::new()...)
IntrinsicWidth::new().child(Column::new()...)
IntrinsicSize::new().child(content)
// Built into: Dialog, Tooltip, BottomSheet
// Dev warning: IntrinsicHeight inside ScrollView
```

### Baseline Alignment
```rust
Row::new()
    .align(Alignment::Baseline)
    .child(Text::new("Label"))
    .child(Input::new())
```

### Impossible Layouts — Compile Error
```
error[T002]: Expanded inside unbounded Column
  → Column needs bounded height for Expanded to work
  hint: wrap Column in SizedBox or give explicit height
  docs: rosace.dev/errors/T002
```

### Overlay System
```
Layers (bottom to top):
  0 — App content
  1 — Navigation transitions
  2 — Modal barrier
  3 — Modals (dialogs, sheets)
  4 — Overlays (tooltips, dropdowns)
  5 — Dev tools
```

### RTL Layout
```rust
.padding_start(16)   // left in LTR, right in RTL
.padding_end(16)
.padding_left(16)    // always left, never mirrors
Directionality::ltr().child(CodeBlock::new(code))
Icon::new("arrow_forward").mirror_in_rtl(true)
```

### Text Layout
- **Foundation**: cosmic-text (BiDi, shaping, fallback)
- **Shaping**: HarfBuzz
- **Rasterization**: fontdue (pure Rust)
- **Rendering**: Skia

```rust
Text::new("Hello مرحبا 🌍")
    .max_lines(3)
    .overflow(TextOverflow::Ellipsis)

RichText::new()
    .span("Hello ", TextStyle::body())
    .span("World", TextStyle::bold())

SelectableText::new("Copy me")
TextInput::new().value(text.get()).on_change(|t| text.set(t))
```

---

## 7. SCROLL & VIRTUALIZATION

### Scroll API
```rust
ScrollView::vertical()
    .physics(ScrollPhysics::platform_default())
    .keyboard_avoid(KeyboardAvoid::ScrollToFocused)
    .pull_to_refresh(|| async { feed.refresh().await })
    .restore_position(true)
    .restoration_key("home_feed")
    .child(content)
```

### Virtual List
```rust
VirtualList::new(items)
    .item_builder(|index, item| {
        ItemRow::new(item).key(item.id)
    })
    .estimated_item_height(56.0)
    .overscan(5)
    .sticky_headers(true)
    .section_header(|s| SectionHeader::new(s.title))
    .on_end_reached(|| feed.load_next_page())
    .end_threshold(5)
    .footer(match feed.state() {
        PageState::Loading => LoadingSpinner::new(),
        PageState::Error(e) => RetryButton::new(e),
        PageState::Done    => EndMessage::new(),
        PageState::Idle    => Empty::new(),
    })
```

### Nested Scroll Arbitration
```
Gesture angle < 30°  → horizontal owner wins
Gesture angle > 60°  → vertical owner wins
Between 30°–60°      → first touch contact wins
After 100ms          → axis locked by velocity
```

### Bidirectional Scroll
- Phase 2 — API reserved: `ScrollView2D::new()`

---

## 8. NAVIGATION

### Route Definition
```rust
#[routes]
enum AppRoute {
    #[route("/")]
    Home,

    #[route("/profile/:id")]
    Profile { id: String },

    #[route("/search")]
    #[query_param("q", field = "query")]
    Search { query: String },

    #[route("/*")]
    NotFound,
}
```

### Navigation API
```rust
Navigator::push(AppRoute::Profile { id: "123".into() });
Navigator::pop();
Navigator::pop_to_root();
Navigator::replace(AppRoute::Home);
Navigator::show_modal(AppRoute::Settings);
Navigator::dismiss_modal();
Navigator::preload(AppRoute::Admin);
```

### Navigation Patterns
```rust
TabNavigator::new()
    .tab(Tab {
        route: AppRoute::Home,
        icon: Icon::home(),
        label: "Home",
        keep_alive: true,
        navigator: StackNavigator::new().initial(AppRoute::Home),
    })
```

### Navigation Guards
```rust
use_before_leave(|destination| async {
    if form_is_dirty() {
        let confirmed = Dialog::confirm("Discard changes?").await;
        if confirmed { NavigationDecision::Allow }
        else { NavigationDecision::Block }
    } else {
        NavigationDecision::Allow
    }
});
```

### Back Button
```rust
use_back_handler(|| {
    if payment_in_progress() { BackHandlerResult::Block }
    else { BackHandlerResult::Pop }
});
```

### Web URL Sync
- Automatic — browser URL matches app route
- Back/forward browser buttons work
- Bookmarkable URLs, query parameters supported
- Hash routing: `rsc build --web-routing=hash`

---

## 9. ANIMATION SYSTEM

```rust
// Level 1 — Implicit
Text::new("Hello").animate().color(theme.primary)

// Level 2 — Transition
AnimatedContainer::new()
    .duration(Duration::ms(300))
    .curve(Curve::EaseInOut)
    .width(if expanded { 300.0 } else { 100.0 })

// Level 3 — Timeline
Animation::new()
    .keyframe(0.0, |s| s.opacity(0.0).scale(0.8))
    .keyframe(0.6, |s| s.opacity(1.0).scale(1.05))
    .keyframe(1.0, |s| s.scale(1.0))
    .duration(Duration::ms(400))
    .curve(Curve::Spring { stiffness: 200.0, damping: 20.0 })

// Physics
SpringAnimation::new()
    .target(1.0)
    .stiffness(180.0)
    .damping(12.0)
```

---

## 10. RENDERING

### Pipeline
```
Component Tree → Element Tree → Layout → Paint → Skia → GPU
```

### Image Handling
```rust
Image::network("https://...")
    .size(200, 200)
    .fit(ImageFit::Cover)
    .placeholder(Skeleton::new())
    .error(Icon::new("broken_image"))
    .cache(CachePolicy::NetworkFirst)
```

- Always decoded on background thread
- Formats: PNG, JPEG, WebP, AVIF, GIF, SVG, APNG
- Memory cache: LRU 50MB default
- Disk cache: LRU 200MB default

### Custom Painters
```rust
CustomPaint::new()
    .painter(|canvas: &mut SkiaCanvas, size: Size| {
        canvas.draw_circle(center, 50.0, Paint::new().color(Colors::red));
    })
    .hit_tester(|point, size| point.distance_to(center) < 50.0)
    .repaint_when(repaint_atom)
```

### Accessibility
```rust
Text::new("Alert")
    .semantic_label("Alert: Important notice")
    .semantic_role(Role::Alert)

FocusScope::new()
    .auto_focus(true)
    .trap_focus(true)
    .child(Dialog::new())
```

**Platform bridges:**
- iOS → UIAccessibility
- Android → AccessibilityNodeInfo
- Web → ARIA
- Desktop → OS accessibility APIs

### HDR / Wide Color
- Phase 3 — sRGB for Phase 1 and 2
- API reserved: `RosaceApp::new().color_space(ColorSpace::DisplayP3)`

---

## 11. UI CUSTOMIZATION

Five levels. Users choose how deep they go.

### Level 1 — Theme Tokens
```rust
#[derive(RosaceTheme)]
struct AppTheme {
    primary:         Color,
    secondary:       Color,
    background:      Color,
    surface:         Color,
    error:           Color,
    on_primary:      Color,
    display:         TextStyle,
    headline:        TextStyle,
    body:            TextStyle,
    caption:         TextStyle,
    radius_sm:       f32,
    radius_md:       f32,
    radius_lg:       f32,
    spacing:         SpacingScale,
    duration_fast:   Duration,
    duration_normal: Duration,
    curve_default:   Curve,
}
// Partial theme = compile error

RosaceApp::new()
    .theme(if dark { dark_theme } else { light_theme })
```

### Level 2 — Component Styling
```rust
Button::new("Click")
    .background(Colors::red)
    .foreground(Colors::white)
    .border_radius(BorderRadius::all(8.0))
    .padding(EdgeInsets::symmetric(16.0, 8.0))
    .hover_style(|s| s.background(Colors::darkred))
    .pressed_style(|s| s.scale(0.98))
    .disabled_style(|s| s.opacity(0.5))
```

### Level 3 — Component Override
```rust
RosaceApp::new()
    .override_widget::<Button, MyButton>()

WidgetScope::new()
    .override_widget::<Button, MyButton>()
    .child(MyFeature::new())

struct MyButton;
impl WidgetOverride<Button> for MyButton {
    fn build(props: &ButtonProps) -> Element {
        // completely custom implementation
    }
}
```

### Level 4 — Custom RenderObject
```rust
struct StarRatingRenderObject {
    rating:    f32,
    max_stars: u32,
    color:     Color,
}

impl RenderObject for StarRatingRenderObject {
    fn layout(&mut self, constraints: Constraints) -> Size {
        Size {
            width:  28.0 * self.max_stars as f32,
            height: 24.0,
        }
    }

    fn paint(&self, canvas: &mut SkiaCanvas, size: Size) {
        for i in 0..self.max_stars {
            let filled = i as f32 < self.rating;
            canvas.draw_star(
                Point::new(i as f32 * 28.0 + 12.0, 12.0),
                12.0,
                if filled { self.color } else { Colors::grey }
            );
        }
    }

    fn hit_test(&self, point: Point, size: Size) -> bool {
        point.y >= 0.0 && point.y <= size.height &&
        point.x >= 0.0 && point.x <= size.width
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode::new()
            .label(format!("{} of {} stars", self.rating, self.max_stars))
            .role(Role::Slider)
    }
}
```

### Level 5 — Custom Render Pipeline
```rust
// Game engine inside ROSACE app
GameView::new()
    .renderer(MyGameRenderer::new())
    .size(Size::fill())

// 3D content
SceneView::new()
    .renderer(WgpuRenderer::new())
    .scene(my_3d_scene)

trait RosaceRenderer: Send {
    fn initialize(&mut self, surface: &RenderSurface);
    fn render(&mut self, commands: &[RenderCommand], size: Size);
    fn resize(&mut self, size: Size);
    fn cleanup(&mut self);
}
```

---

## 12. OBSERVABILITY & TRACING

### The Rule
> No system in ROSACE gets merged without its trace emissions.
> Tracing is architecture, not a feature.

### RosaceTrace — Unified Event Language
```rust
enum RosaceTrace {
    ComponentMount    { id: ComponentId, name: &'static str, location: Location },
    ComponentUnmount  { id: ComponentId, name: &'static str },
    ComponentRebuild  { id: ComponentId, cause: RebuildCause, duration: Duration },
    AtomRead          { atom: AtomId, component: ComponentId },
    AtomWrite         { atom: AtomId, old: TraceValue, new: TraceValue,
                        by: ComponentId, location: Location },
    LayoutStart       { component: ComponentId, constraints: Constraints },
    LayoutEnd         { component: ComponentId, size: Size, duration: Duration },
    FrameStart        { frame: u64, timestamp: Instant },
    FrameEnd          { frame: u64, duration: Duration, dropped: bool },
    PaintRegion       { rect: Rect },
    RouteChange       { from: Option<Route>, to: Route, transition: Transition },
    RequestStart      { id: RequestId, url: String, method: Method, component: ComponentId },
    RequestEnd        { id: RequestId, status: u16, duration: Duration,
                        cached: bool, size: usize },
    FfiCall           { fn_name: &'static str, duration: Duration },
    FfiError          { fn_name: &'static str, error: String },
    GestureReceived   { kind: GestureKind, handler: ComponentId },
}
```

### TracingBus — Zero Cost in Production
```rust
// In release: entire bus compiles to nothing
macro_rules! trace {
    ($event:expr) => {
        #[cfg(debug_assertions)]
        TRACING_BUS.emit($event);
    }
}
```

### Subscribers
```
RingBufferSubscriber  → last N events, enables time travel
DevToolsSubscriber    → WebSocket/shared memory to dev tools
FileSubscriber        → crash dumps, post-mortem
ConsoleSubscriber     → terminal output, filterable
IdeSubscriber         → IDE extension (Phase 3)
```

### Time Travel Debugging
```
T-500ms  ATOM  USER.set(None)
T-400ms  ROUTE Home → Login
T-300ms  REQUEST POST /api/refresh  401
T-100ms  REBUILD AppBar
T-0ms    CRASH index out of bounds   ← now you know why
```

### Terminal Filtering
```
rsc dev --trace=state
rsc dev --trace=network
rsc dev --trace=performance
rsc dev --trace=all
rsc dev --trace=component:HomeScreen
```

### Protocol — IDE Ready From Day 1
```rust
#[derive(Serialize, Deserialize)]
enum RosaceTrace { ... }

// Native → Unix socket
// WASM   → WebSocket
// Same protocol — IDE tools in any language
```

---

## 13. DEV TOOLS

### Phase 1 — CLI Output
```
[MOUNT]   HomeScreen        src/screens/home.rs:12
[ATOM]    FEED.set()        64 items
[REBUILD] FeedList          cause: FEED  0.8ms
[FRAME]   #847              2.1ms ✓
[REQUEST] GET /api/feed     200  145ms  cache:miss
[ROUTE]   Home → Profile    slide_left  280ms
```

### Phase 2 — Browser Dev Tools
```
┌──────────────────────────────────────────────────────┐
│ ROSACE DEVTOOLS                          FPS: 120  │
├──────────────┬──────────────────┬───────────────────┤
│ Component    │ Properties       │ State             │
│ Tree         │                  │                   │
│ ▼ App        │ Button           │ count: 3          │
│   ▼ Column   │  label: "Click"  │ theme: Dark       │
│     Text     │  disabled: false │ user: None        │
│   ▶ Button   │  padding: 16     │                   │
│              │                  │ [◀][▶] Time Travel│
├──────────────┴──────────────────┴───────────────────┤
│ Layout: ON │ Repaint: ON │ Semantics: OFF           │
│ Network ──────────────────────────────────────────   │
│ GET /api/feed    200  145ms  ████░░ 2.3kb           │
└──────────────────────────────────────────────────────┘
```

### Dev Tools Transport
- **Native**: shared memory + Unix socket
- **WASM**: WebSocket
- **Protocol**: MessagePack, versioned
- **Separate process**: app crash never kills dev tools

### Hot Reload Limits
```
Can hot reload:
  ✓ Component build() logic
  ✓ Style and layout changes
  ✓ Event handler changes
  ✓ Atom default values
  ✓ Text and strings

Requires full restart:
  ✗ New Cargo.toml dependency
  ✗ Atom type structure changed
  ✗ FFI bindings changed
  ✗ Macro definitions changed
```

### Phase 3 — IDE Extensions
- VS Code extension + RustRover plugin
- Both speak RosaceTrace protocol
- Component tree in sidebar
- Atom values on hover in editor
- Click component → jump to source

### Phase 4 — ROSACE Studio
- Standalone app built with ROSACE itself
- Full visual debugger, time travel UI
- Network timeline, frame profiler
- Team debugging sessions

---

## 14. PLATFORM INTEGRATION

### App Lifecycle
```rust
enum LifecycleState {
    Active, Inactive, Background, Suspended,
}

on_change(APP_LIFECYCLE, |state| {
    match state {
        LifecycleState::Background => pause_sync(),
        LifecycleState::Active     => resume_sync(),
        LifecycleState::Suspended  => save_state(),
        _ => {}
    }
});
```

### Permissions
```rust
let status = Permission::camera()
    .rationale("We need camera to scan QR codes")
    .request().await;

match status {
    PermissionStatus::Granted           => open_camera(),
    PermissionStatus::Denied            => show_manual_steps(),
    PermissionStatus::PermanentlyDenied => open_settings(),
}
```

### Localization
```rust
let t = use_locale();
Text::new(t.feed.items_count(n: count))

RosaceApp::new()
    .locales(vec!["en", "ar", "fr"])
    .default_locale("en")
    .locale_dir("assets/locales/")

LOCALE.set(Locale::Arabic);
// RTL layout mirrors automatically
```

### Haptics
```rust
Haptic::light()    Haptic::success()
Haptic::medium()   Haptic::warning()
Haptic::heavy()    Haptic::error()
Haptic::selection()
// Desktop/WASM → silent no-op
```

### Safe Areas
```rust
Scaffold::new()
    .app_bar(AppBar::new("Title"))
    .body(content)
    .bottom_bar(BottomNav::new())
// Scaffold automatically pads for safe areas

Padding::safe_area().child(content)
Image::asset("hero.jpg").ignore_safe_area(true)
```

### Minimum OS Versions
```
iOS       → 16.0+
Android   → API 24+ (Android 7.0)
macOS     → 12.0+ (Monterey)
Windows   → 10 (build 1903+)
Linux     → Ubuntu 20.04+
Web       → Chrome 90+, Firefox 90+, Safari 15+
```

---

## 15. FFI LAYER

### Bridges
```rust
#[rosace_ffi(c)]
extern "C" { fn native_compute(input: f64) -> f64; }

#[rosace_ffi(swift)]
swift_import!("NativeModule") {
    fn get_device_info() -> DeviceInfo;
}

#[rosace_ffi(kotlin)]
kotlin_import!("com.app.NativeModule") {
    fn get_android_id() -> String;
}

#[rosace_ffi(js)]
js_import! {
    fn window_location() -> String;
}
```

### Synchronous Bridge (JSI-like)
```rust
// Zero serialization — call in render path
let value = sync_bridge::call::<f64>("native_fn", args)?;

// Shared memory for hot path
let shared = SharedMemory::map("rosace_shared", size)?;
```

### Memory Ownership
```rust
let data = ForeignBox::new(
    ptr:  unsafe { c_lib::alloc() },
    drop: |ptr| unsafe { c_lib::free(ptr) },
);
```

### Binary Size
```toml
[features]
ffi-opencv    = ["dep:opencv-sys"]
ffi-sqlite    = ["dep:sqlite3-sys"]
ffi-bluetooth = ["dep:btleplug"]
```

---

## 16. TESTING

```rust
#[rosace_test]
fn test_counter() {
    let tree = render!(Counter { label: "Test" });
    assert_text!(tree, "0");
    tap!(tree, button: "Increment");
    assert_text!(tree, "1");
}

#[rosace_snapshot]
fn test_button_states() {
    snapshot!(Button::new("Normal"));
    snapshot!(Button::new("Disabled").disabled(true));
}
```

Golden files per platform:
```
tests/goldens/
  ├── desktop/
  ├── mobile/
  └── web/
```

---

## 17. BUILD SYSTEM

```
rsc dev                          → dev server, hot reload
rsc dev --trace=all              → full tracing
rsc dev --profile                → deep profiling
rsc dev --time-travel            → time travel debugging
rsc build --target web           → WASM + JS glue
rsc build --target desktop       → native binary
rsc build --target ios           → .ipa
rsc build --target android       → .apk / .aab
rsc build --target all           → everything
rsc build --web-routing=hash     → hash-based routing
rsc test                         → all tests
rsc test --update-goldens        → update snapshots
rsc analyze                      → lint + perf hints
rsc snapshot --update            → update golden files
```

### JIT/AOT Duality
```
Dev mode (JIT-like):
  → WASM hot-swap on change
  → State preserved across reloads
  → All tracing active
  → Dev tools available

Production (AOT):
  → Full native compilation
  → Dead code eliminated
  → All tracing stripped
  → Maximum performance
```

---

## 18. CRATE STRUCTURE

```
rosace/
├── rosace-core/       ← component model, element tree, lifecycle
├── rosace-state/      ← atom system, reactivity, refresh engine
├── rosace-layout/     ← constraint engine (Flexure), RTL
├── rosace-render/     ← Skia integration, layers, dirty regions
├── rosace-scroll/     ← virtualization, physics, arbitration
├── rosace-nav/        ← router, transitions, guards, deep links
├── rosace-animate/    ← animation, physics, springs
├── rosace-platform/   ← platform APIs, permissions, safe areas
├── rosace-ffi/        ← all bridges, sync bridge
├── rosace-theme/      ← design tokens, theming, localization
├── rosace-test/       ← testing utilities, snapshot testing
├── rosace-trace/      ← RosaceTrace bus, subscribers, protocol
├── rosace-devtools/   ← dev tools server, hot reload, time travel
├── rosace-macros/     ← all proc macros
├── rosace-cli/        ← rsc CLI commands
└── rosace-widgets/    ← official widget library
```

---

## 19. PHASE ROADMAP

### Phase 1 — Foundation (3–6 months)
**Goal: Working desktop app in pure Rust**

Deliver:
- rosace-core: Component model, element tree, four-layer architecture
- rosace-state: Atom, three scopes, refresh engine, batching
- rosace-layout: Flexure, Column/Row/Stack/Flex/Grid, text layout
- rosace-render: Skia pipeline, dirty regions, layer compositing
- rosace-trace: Tracing bus, console + file + ring buffer
- rosace-macros: #[component], #[state], core macros
- rosace-cli: rsc dev, rsc build --target desktop

Exit criteria:
- Counter app works
- Constraint layout works
- State updates and re-renders correctly
- Tracing output in terminal
- 60fps on desktop

### Phase 2 — Interaction (4–6 months)
**Goal: Real app buildable, web + desktop**

Deliver:
- rosace-scroll: Virtualization, sticky headers, pull to refresh
- rosace-nav: Router, stack, tab, transitions, guards
- rosace-animate: All three animation levels, physics
- rosace-platform: Desktop platform APIs
- rosace-devtools: Browser dev tools, hot reload via WASM
- rosace-widgets: Core widget library

Exit criteria:
- Full CRUD app buildable
- Navigation with guards works
- 10,000+ item lists performant
- Hot reload working
- Dev tools visible in browser

### Phase 3 — Platform (6–9 months)
**Goal: Ship on all platforms**

Deliver:
- iOS and Android targets
- rosace-ffi: All bridges, sync bridge
- Full platform APIs
- VS Code extension
- 2D scroll
- Wide color / HDR

Exit criteria:
- App on App Store and Play Store
- All platform APIs working
- VS Code extension published

### Phase 4 — Ecosystem (ongoing)
**Goal: Community can build and share**

Deliver:
- Full test framework
- Plugin registry
- Theme marketplace
- Documentation site
- rosace.dev error reference

### Phase 5 — Polish (never done)
**Goal: Production-grade**

Deliver:
- ROSACE Studio
- Performance tuning
- Accessibility audit
- v1.0 release

---

## 20. OPEN DECISIONS (DEFERRED)

```
1.  ROSACE Studio detailed design     → Phase 4
2.  Wide color / HDR specifics         → Phase 3
3.  2D bidirectional scroll            → Phase 3
4.  Plugin registry governance         → Phase 4
5.  Minimum OS version review          → v1.0
6.  RustRover / IntelliJ plugin        → Phase 3b
7.  ROSACE package manager            → Phase 4
8.  Server-side rendering              → not planned, revisit
9.  React Native interop               → not planned
10. Embedded / no-std targets          → Phase 5 consideration
```

---

## APPENDIX A — COMPILER ERROR REFERENCE

All errors follow format `T{number}` and link to `rosace.dev/errors/T{number}`.

```
T001 — Missing required handler
       Button requires .on_press() or .disabled(true)

T002 — Impossible layout
       Expanded inside unbounded axis

T003 — Dynamic list without keys
       List with changing order needs .key()

T004 — Lifecycle hook not at top level
       on_mount() must be at component top level

T005 — Scoped atom outside provider
       CART accessed outside CartProvider subtree

T006 — Non-serializable persisted atom
       Persisted atom type must impl Serialize+Deserialize

T007 — Circular derived atoms
       Derived atom A depends on B which depends on A

T008 — Partial theme
       AppTheme missing required field: error_color

T009 — Intrinsic inside unbounded scroll
       IntrinsicHeight inside ScrollView is expensive

T010 — Lazy component missing loading state
       Lazy component must declare .loading() handler
```

---

## APPENDIX B — GLOSSARY

```
Atom           ROSACE's core state primitive.
               A reactive value. When changed,
               all subscribers are notified.

Component      What users write. The logical unit of UI.

Element        Lightweight immutable description of
               what to render. Created by Component.build().

RenderObject   Handles layout, painting, hit testing.
               Created from Element.

SemanticNode   Accessibility tree node.

Flexure        ROSACE's layout engine. Constraint-based.

RosaceTrace   Unified event type emitted by all systems.

TracingBus     Central hub for RosaceTrace events.
               Zero cost in production.

Atom Scope     Where an atom lives: local, provided, global.

Provider       Makes a scoped atom available to a subtree.

AOT            Ahead-of-time. Production mode.

JIT            Dev mode WASM hot-swap approximates JIT.

Dirty          Component or region needing rebuild/repaint.

Refresh Engine Finds minimum components to rebuild
               after atom changes.

rsc            The ROSACE CLI tool.
               Short for ROSACE.
```

---

*ROSACE Master Document — v0.1*
*Fast by nature. Beautiful by design.*
*Architecture complete. Ready for Phase 1.*
*Every decision documented. Every doubt captured.*
*First line of code: rosace-core/src/component.rs*
