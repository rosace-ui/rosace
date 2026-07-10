# ROSACE — PHASE 2
> Rich UI Layer: Polished, themed, animated apps
> Prerequisite: Phase 1 exit criteria 100% complete
> Target: Developer can build polished, themed, animated apps at 60fps

---

## PHASE 2 GOAL

A developer can build polished, themed, animated apps.
All standard widgets respect a typed theme system.
Animations are physics-based and driven by Atom<f32>.
Scroll views have momentum. Multi-screen demo proves the system.

---

## EXIT CRITERIA
> Phase 2 is NOT done until ALL of these pass.

```
□ rosace-theme crate ships with built-in Light and Dark themes
□ All theme tokens typed — compile error for missing tokens
□ rosace-widgets ships Button, Text, TextInput, Image, Divider, ScrollView
□ All widgets use #[component] macro and respect theme tokens
□ rosace-animate ships Tween, Spring, Keyframe — driven by Atom<f32>
□ Animated counter example runs at 60fps with spring animations
□ rosace-scroll ships ScrollView with momentum scrolling
□ A real multi-screen demo app (rosace-examples) using all Phase 2 features
□ All new crate tests pass
□ No warnings in release build
□ No unsafe without SAFETY comments
```

---

## STEP-BY-STEP PLAN

### STEP 1 — rosace-theme crate
**Build this first. Every widget in Phase 2 depends on it.**

**1a — Core color type**
```rust
/// Linear RGBA, all channels in [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self { ... }
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self { Self::rgba(r, g, b, 1.0) }
    pub fn from_hex(hex: u32) -> Self { ... }
    pub fn with_alpha(self, a: f32) -> Self { ... }
}
```

**1b — ColorScheme**
```rust
pub struct ColorScheme {
    // Brand
    pub primary: Color,
    pub primary_variant: Color,
    pub secondary: Color,
    pub secondary_variant: Color,
    // Surfaces
    pub background: Color,
    pub surface: Color,
    pub surface_variant: Color,
    // Status
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    // On-colors (text/icons on top of each surface)
    pub on_primary: Color,
    pub on_secondary: Color,
    pub on_background: Color,
    pub on_surface: Color,
    pub on_error: Color,
    // Utility
    pub outline: Color,
    pub shadow: Color,
    pub scrim: Color,
}
```

**1c — Typography**
```rust
pub enum FontWeight {
    Thin,       // 100
    Light,      // 300
    Regular,    // 400
    Medium,     // 500
    SemiBold,   // 600
    Bold,       // 700
    ExtraBold,  // 800
    Black,      // 900
}

pub struct TextStyle {
    pub font_family: FontFamily,
    pub size: f32,           // sp (scale-independent pixels)
    pub weight: FontWeight,
    pub line_height: f32,    // multiplier, e.g. 1.4
    pub letter_spacing: f32, // em units
    pub color: Option<Color>, // None → inherit from ColorScheme
}

pub struct Typography {
    pub display_large: TextStyle,
    pub display_medium: TextStyle,
    pub display_small: TextStyle,
    pub headline_large: TextStyle,
    pub headline_medium: TextStyle,
    pub headline_small: TextStyle,
    pub title_large: TextStyle,
    pub title_medium: TextStyle,
    pub title_small: TextStyle,
    pub body_large: TextStyle,
    pub body_medium: TextStyle,
    pub body_small: TextStyle,
    pub label_large: TextStyle,
    pub label_medium: TextStyle,
    pub label_small: TextStyle,
}
```

**1d — Spacing, border radius, shadow tokens**
```rust
pub struct Spacing {
    pub xs: f32,   // 4.0
    pub sm: f32,   // 8.0
    pub md: f32,   // 16.0
    pub lg: f32,   // 24.0
    pub xl: f32,   // 32.0
    pub xxl: f32,  // 48.0
}

pub struct BorderRadius {
    pub none: f32,   // 0.0
    pub xs: f32,     // 2.0
    pub sm: f32,     // 4.0
    pub md: f32,     // 8.0
    pub lg: f32,     // 12.0
    pub xl: f32,     // 16.0
    pub full: f32,   // 9999.0 (pill shape)
}

pub struct Shadow {
    pub color: Color,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread_radius: f32,
}

pub struct Elevation {
    pub none: Vec<Shadow>,
    pub sm: Vec<Shadow>,
    pub md: Vec<Shadow>,
    pub lg: Vec<Shadow>,
    pub xl: Vec<Shadow>,
}
```

**1e — RosaceTheme trait**
```rust
/// Implement this to define a complete theme.
/// All fields must be provided — missing tokens = compile error.
pub trait RosaceTheme: 'static + Send + Sync {
    fn color_scheme(&self) -> &ColorScheme;
    fn typography(&self) -> &Typography;
    fn spacing(&self) -> &Spacing;
    fn border_radius(&self) -> &BorderRadius;
    fn elevation(&self) -> &Elevation;
}
```

**1f — #[derive(RosaceTheme)] proc-macro (add to rosace-macros)**
- Add `RosaceTheme` derive variant to rosace-macros
- Generates `RosaceTheme` impl stubs with compiler-friendly errors for missing fields
- Usage:
```rust
#[derive(RosaceTheme)]
pub struct MyTheme {
    colors: ColorScheme,
    type_: Typography,
    spacing: Spacing,
    radii: BorderRadius,
    elevation: Elevation,
}
```

**1g — Built-in themes**
```rust
pub struct RosaceLightTheme { /* ... */ }
pub struct RosaceDarkTheme  { /* ... */ }

impl RosaceTheme for RosaceLightTheme { /* ... */ }
impl RosaceTheme for RosaceDarkTheme  { /* ... */ }
```

**1h — ThemeProvider widget and use_theme() hook**
```rust
/// Injects a theme into the widget tree. Widgets below can call use_theme().
#[component]
pub fn ThemeProvider(theme: Arc<dyn RosaceTheme>, children: Element) -> Element { ... }

/// In rosace-state — reads the nearest ThemeProvider's theme.
pub fn use_theme(ctx: &mut Context) -> Arc<dyn RosaceTheme> { ... }
```

Tests required:
```rust
#[test]
fn light_theme_compiles_and_provides_tokens() { }

#[test]
fn dark_theme_compiles_and_provides_tokens() { }

#[test]
fn custom_theme_via_derive_macro() { }

#[test]
fn use_theme_returns_nearest_provider() { }

#[test]
fn theme_provider_nesting_prefers_inner() { }
```

**Commit:**
```bash
git commit -m "feat(theme): implement RosaceTheme, ColorScheme, Typography, and built-in Light/Dark themes"
```

---

### STEP 2 — rosace-widgets core set
**Build each widget in this order. Each one must compile and render before moving on.**

All widgets must:
- Use the `#[component]` proc-macro
- Accept theme tokens via `use_theme(ctx)`
- Have doc comments on every public field
- Have at least one test

**2a — Text widget**
```rust
pub enum TextOverflow {
    Clip,
    Ellipsis,
    Fade,
}

#[component]
pub fn Text(
    content: impl Into<String>,
    style: Option<TextStyle>,      // defaults to body_medium from theme
    color: Option<Color>,          // overrides style.color
    overflow: TextOverflow,        // default: Clip
    max_lines: Option<usize>,
    text_align: TextAlign,
) -> Element { ... }
```

**2b — Button widget**
```rust
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Ghost,
    Outlined,
}

#[component]
pub fn Button(
    label: impl Into<String>,
    on_click: impl Fn() + 'static,
    variant: ButtonVariant,        // default: Primary
    disabled: bool,
    loading: bool,
    icon: Option<Element>,         // optional leading icon
) -> Element { ... }
```

Button states: idle, hovered, pressed, disabled, loading.
All state transitions read from the active theme.

**2c — TextInput widget**
```rust
#[component]
pub fn TextInput(
    value: Atom<String>,
    placeholder: impl Into<String>,
    on_change: impl Fn(String) + 'static,
    on_submit: Option<impl Fn(String) + 'static>,
    obscure: bool,                 // password field
    disabled: bool,
    label: Option<String>,
    error: Option<String>,
    max_length: Option<usize>,
) -> Element { ... }
```

**2d — Divider widget**
```rust
pub enum DividerDirection {
    Horizontal,
    Vertical,
}

#[component]
pub fn Divider(
    direction: DividerDirection,
    color: Option<Color>,          // defaults to theme.color_scheme().outline
    thickness: f32,                // default 1.0px
    indent: f32,                   // leading indent
    end_indent: f32,               // trailing indent
) -> Element { ... }
```

**2e — Image widget**
```rust
pub enum ImageFit {
    Contain,
    Cover,
    Fill,
    None,
    ScaleDown,
}

#[component]
pub fn Image(
    src: ImageSource,              // Path, URL, or embedded bytes
    fit: ImageFit,
    width: Option<f32>,
    height: Option<f32>,
    border_radius: Option<f32>,
    placeholder: Option<Element>,  // shown while loading
    error_widget: Option<Element>, // shown on load failure
) -> Element { ... }
```

**2f — Padding and Center utility widgets**
```rust
#[component]
pub fn Padding(
    all: Option<f32>,
    horizontal: Option<f32>,
    vertical: Option<f32>,
    top: Option<f32>,
    right: Option<f32>,
    bottom: Option<f32>,
    left: Option<f32>,
    child: Element,
) -> Element { ... }

#[component]
pub fn Center(child: Element) -> Element { ... }
```

Tests required per widget:
```rust
#[test]
fn text_renders_with_theme_style() { }

#[test]
fn button_calls_on_click_when_pressed() { }

#[test]
fn button_disabled_does_not_fire_on_click() { }

#[test]
fn text_input_updates_atom_on_change() { }

#[test]
fn text_input_fires_on_submit_on_enter() { }

#[test]
fn image_shows_placeholder_while_loading() { }

#[test]
fn padding_applies_correct_insets() { }

#[test]
fn center_centers_child_in_parent() { }
```

**Commit:**
```bash
git commit -m "feat(widgets): add Text, Button, TextInput, Divider, Image, Padding, Center"
```

---

### STEP 3 — rosace-animate crate
**Physics-based animation. All values driven through Atom<f32>.**

**3a — Lerp trait**
```rust
pub trait Lerp: Clone + 'static {
    fn lerp(from: &Self, to: &Self, t: f32) -> Self;
}

// Implement for: f32, f64, Color, Point, Size, Rect, [f32; N]
```

**3b — Easing functions**
```rust
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    Custom(fn(f32) -> f32),    // user-supplied curve
}

impl Easing {
    pub fn apply(&self, t: f32) -> f32 { ... }
}
```

**3c — Tween<T: Lerp>**
```rust
pub struct Tween<T: Lerp> {
    pub from: T,
    pub to: T,
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
    pub repeat: RepeatMode,       // Once, Loop, PingPong, Count(u32)
}

impl<T: Lerp> Tween<T> {
    pub fn new(from: T, to: T, duration: Duration) -> Self { ... }
    pub fn with_easing(self, easing: Easing) -> Self { ... }
    pub fn with_delay(self, delay: Duration) -> Self { ... }
    pub fn with_repeat(self, repeat: RepeatMode) -> Self { ... }
}
```

**3d — Spring**
```rust
pub struct Spring {
    pub stiffness: f32,    // e.g. 300.0
    pub damping: f32,      // e.g. 30.0
    pub mass: f32,         // e.g. 1.0
    pub initial_velocity: f32,
}

impl Spring {
    /// Common presets
    pub fn gentle() -> Self    { Self { stiffness: 120.0, damping: 14.0, mass: 1.0, initial_velocity: 0.0 } }
    pub fn bouncy() -> Self    { Self { stiffness: 300.0, damping: 10.0, mass: 1.0, initial_velocity: 0.0 } }
    pub fn stiff() -> Self     { Self { stiffness: 400.0, damping: 28.0, mass: 1.0, initial_velocity: 0.0 } }
    pub fn slow() -> Self      { Self { stiffness:  80.0, damping: 20.0, mass: 1.0, initial_velocity: 0.0 } }

    /// Advance simulation by dt seconds. Returns (position, velocity).
    pub fn step(&self, position: f32, velocity: f32, target: f32, dt: f32) -> (f32, f32) { ... }

    /// Returns true when the spring has settled within epsilon of target.
    pub fn is_settled(&self, position: f32, velocity: f32, target: f32, epsilon: f32) -> bool { ... }
}
```

**3e — Keyframe animation**
```rust
pub struct Keyframe<T: Lerp> {
    /// t in [0.0, 1.0]
    pub stops: Vec<(f32, T)>,
    pub easing: Easing,
    pub duration: Duration,
    pub repeat: RepeatMode,
}
```

**3f — AnimationController**
```rust
pub struct AnimationController {
    state: Atom<AnimationState>,
}

pub enum AnimationState {
    Idle,
    Running { elapsed: Duration },
    Paused  { elapsed: Duration },
    Completed,
}

impl AnimationController {
    pub fn start(&self)   { ... }
    pub fn stop(&self)    { ... }
    pub fn pause(&self)   { ... }
    pub fn reset(&self)   { ... }
    pub fn reverse(&self) { ... }
    pub fn on_complete(&self, cb: impl Fn() + 'static) { ... }
    pub fn progress(&self) -> f32 { ... }  // [0.0, 1.0]
}
```

**3g — Animated<T> and use_animation() hook**
```rust
/// An Atom-backed animated value. Widgets read this; the animation driver writes it.
pub struct Animated<T: Lerp>(Atom<T>);

impl<T: Lerp> Animated<T> {
    pub fn get(&self) -> T { ... }
}

/// Returns (Animated<T>, AnimationController) for use inside a #[component].
pub fn use_animation<T: Lerp>(
    ctx: &mut Context,
    initial: T,
) -> (Animated<T>, AnimationController) { ... }
```

Tests required:
```rust
#[test]
fn tween_interpolates_correctly_at_t0_t05_t1() { }

#[test]
fn easing_linear_is_identity() { }

#[test]
fn easing_ease_in_out_is_symmetric() { }

#[test]
fn spring_settles_within_50_frames_at_60fps() { }

#[test]
fn spring_bouncy_overshoots_once() { }

#[test]
fn keyframe_interpolates_between_stops() { }

#[test]
fn animation_controller_start_stop_reset() { }

#[test]
fn animated_value_updates_atom_each_frame() { }

#[test]
fn use_animation_triggers_rebuild_on_value_change() { }
```

**Benchmark required:**
```rust
#[bench]
fn bench_spring_100_steps() { }
// Must complete in under 100µs
```

**Commit:**
```bash
git commit -m "feat(animate): implement Tween, Spring, Keyframe, AnimationController, and use_animation hook"
```

---

### STEP 4 — rosace-scroll crate
**Momentum scrolling. Build after widgets and animate.**

**4a — ScrollView widget**
```rust
pub enum ScrollDirection {
    Horizontal,
    Vertical,
    Both,
}

#[component]
pub fn ScrollView(
    child: Element,
    direction: ScrollDirection,       // default: Vertical
    controller: Option<ScrollController>,
    show_scrollbar: bool,             // default: true
    scrollbar_color: Option<Color>,
    physics: ScrollPhysics,           // default: Momentum
    padding: Option<EdgeInsets>,
) -> Element { ... }
```

**4b — ScrollPhysics**
```rust
pub enum ScrollPhysics {
    /// Natural deceleration via friction.
    Momentum { friction: f32 },       // friction default: 0.92
    /// No momentum — snaps to rest immediately.
    Clamped,
    /// Page-snapping.
    Paged { page_size: Option<f32> },
}
```

**4c — ScrollController**
```rust
pub struct ScrollController {
    /// Current scroll offset (read from Atom, writable for programmatic scroll).
    pub offset: Atom<Point>,
}

impl ScrollController {
    pub fn new() -> Self { ... }
    pub fn scroll_to(&self, offset: Point, animated: bool) { ... }
    pub fn scroll_by(&self, delta: Point, animated: bool) { ... }
    pub fn scroll_to_top(&self)    { ... }
    pub fn scroll_to_bottom(&self) { ... }
    /// Save and restore offset (e.g. on navigation return).
    pub fn save_position(&self) -> Point { ... }
    pub fn restore_position(&self, position: Point) { ... }
}
```

**4d — Scroll restoration**
- `ScrollView` with a `controller` restores its position when re-mounted with the same key
- Integration with navigation (Phase 3) via a `ScrollRestorationProvider`

**4e — Scrollbar rendering**
- Thin floating scrollbar, auto-hides after 1.5s of no activity
- Thumb size proportional to content vs viewport ratio
- Colors from active theme (surface_variant / outline)

Tests required:
```rust
#[test]
fn scroll_view_clips_overflow() { }

#[test]
fn scroll_controller_scroll_to_updates_offset_atom() { }

#[test]
fn momentum_physics_decelerates_over_time() { }

#[test]
fn clamped_physics_stops_immediately() { }

#[test]
fn paged_physics_snaps_to_nearest_page() { }

#[test]
fn scrollbar_thumb_size_proportional_to_content() { }

#[test]
fn scroll_restoration_preserves_position() { }
```

**Commit:**
```bash
git commit -m "feat(scroll): implement ScrollView with momentum physics and ScrollController"
```

---

### STEP 5 — Animated counter example
**Proves animate + widgets + theme integrate correctly.**

Update `rosace-examples/src/counter_window.rs`:

```rust
use rosace::prelude::*;
use rosace_animate::prelude::*;
use rosace_theme::prelude::*;

#[component]
fn AnimatedCounter() -> Element {
    let count = use_atom(ctx, 0i32);
    let (display_value, ctrl) = use_animation(ctx, 0.0f32);

    // Spring-animate to the new count whenever it changes
    use_effect(ctx, count.get(), |new_count| {
        ctrl.spring(Spring::bouncy()).animate_to(*new_count as f32);
    });

    let theme = use_theme(ctx);

    Column::new()
        .child(
            Text::new(format!("{:.0}", display_value.get()))
                .style(theme.typography().display_large.clone())
                .color(theme.color_scheme().primary)
        )
        .child(
            Row::new()
                .child(
                    Button::new("−")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move || count.update(|n| n - 1))
                )
                .child(Spacing::new(theme.spacing().md))
                .child(
                    Button::new("+")
                        .variant(ButtonVariant::Primary)
                        .on_click(move || count.update(|n| n + 1))
                )
        )
        .into_element()
}

fn main() {
    RosaceApp::new()
        .theme(RosaceLightTheme::new())
        .child(AnimatedCounter)
        .run();
}
```

**Verify:**
```
□ App opens at 60fps
□ Number spring-animates on each button press
□ Spring overshoots slightly (bouncy preset)
□ Light theme colors applied correctly
□ No jitter or missed frames during animation
□ RosaceTrace emits AnimationFrame events
```

**Commit:**
```bash
git commit -m "feat(examples): animated counter with spring animation and Light theme"
```

---

### STEP 6 — Multi-screen demo app
**Proves the full Phase 2 stack in a realistic app.**

Create `rosace-examples/src/phase2_demo/` with these screens:

**Screen 1: Theme Gallery**
- Shows all color tokens as swatches
- Shows all text styles
- Toggle between Light and Dark themes using a Button

**Screen 2: Widget Showcase**
- All Button variants side by side
- TextInput with validation (email format, shows error state)
- Horizontal and vertical Dividers
- Images with different ImageFit modes

**Screen 3: Animation Lab**
- Tween demos: each easing function animated in real time
- Spring presets: gentle / bouncy / stiff / slow
- Keyframe animation with custom stops

**Screen 4: Scrolling Feed**
- Vertical ScrollView with 100 items
- Each item is a Card (Column + Padding + themed colors)
- Scroll-to-top Button appears after scrolling 200px
- Demonstrates scroll restoration on back navigation

Navigation between screens via a simple tab bar (Phase 3 gets real navigation; use a plain Atom<Screen> enum for now).

**Commit:**
```bash
git commit -m "feat(examples): phase2_demo — theme gallery, widget showcase, animation lab, scrolling feed"
```

---

### STEP 7 — Phase 2 Verification
**Run ALL exit criteria. Sign off every item.**

```
□ rosace-theme crate ships with built-in Light and Dark themes
□ All theme tokens typed — compile error for missing tokens
□ rosace-widgets ships Button, Text, TextInput, Image, Divider, ScrollView
□ All widgets use #[component] macro and respect theme tokens
□ rosace-animate ships Tween, Spring, Keyframe — driven by Atom<f32>
□ Animated counter example runs at 60fps with spring animations
□ rosace-scroll ships ScrollView with momentum scrolling
□ A real multi-screen demo app (rosace-examples) using all Phase 2 features
□ All new crate tests pass
□ No warnings in release build
□ No unsafe without SAFETY comments
```

**Only when ALL boxes are checked → begin Phase 3.**

---

## PHASE 2 DO NOT LIST

```
✗ Do not implement navigation router — Phase 3
✗ Do not implement hot reload — Phase 3
✗ Do not implement WASM target — Phase 3
✗ Do not implement platform APIs (file picker, notifications) — Phase 3
✗ Do not implement accessibility (a11y) — Phase 3
✗ Do not implement i18n / locale — Phase 3
✗ Do not implement video or audio — Phase 3
✗ Do not implement drag-and-drop — Phase 3
✗ Do not implement IntrinsicSize inside ScrollView (dev warning only)
✗ Do not add dependencies without approval
✗ Do not skip tests
✗ Do not skip doc comments
✗ Do not merge code with warnings
```

---

## APPROVED DEPENDENCIES — PHASE 2

```
# Inherited from Phase 1
skia-safe       → Skia rendering
winit           → window + event loop
cosmic-text     → text layout
harfbuzz-rs     → text shaping (via cosmic-text)
fontdue         → font rasterization
serde           → serialization
serde_json      → JSON (dev tools)
rmp-serde       → MessagePack (trace protocol)
tokio           → async runtime (state only)
rayon           → parallel layout
log             → logging facade
env_logger      → logger implementation (dev only)
thiserror       → error types

# Phase 2 additions — none required
# All Phase 2 crates (rosace-theme, rosace-animate, rosace-scroll, rosace-widgets)
# are built on existing workspace crates only.
# Any new external dependency needs explicit approval before adding.
```

**Any new dependency needs approval before adding.**
**Add to this list when approved.**
