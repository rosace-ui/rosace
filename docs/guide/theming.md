# Theming

Every color, font size, spacing gap, and corner radius your app draws comes from one place: the active **theme**. Change the theme and every subscribed component repaints with the new tokens — no per-widget wiring required.

## `ThemeData`: the design-token bundle

A theme is a `ThemeData` struct — a plain bundle of design tokens:

```rust
pub struct ThemeData {
    pub colors: ColorScheme,
    pub animation: AnimationConfig,
    pub typography: Typography,
    pub spacing: Spacing,
    pub radius: BorderRadius,
    pub is_dark: bool,
    pub app_bar: AppBarStyle,
    // + a type-keyed extension map, see below
}
```

- **`colors`** (`ColorScheme`) — semantic roles, not raw hex values: `primary`/`on_primary`, `primary_container`/`on_primary_container`, `secondary`/`on_secondary`, `surface`/`on_surface`, `surface_variant`, `background`/`on_background`, `error`/`on_error`, `outline`, `shadow`. Reading `colors.primary` instead of hardcoding a hex value is what makes light/dark/platform switching free.
- **`typography`** (`Typography`) — a full type scale (`display_large` down to `label_small`), each a `TextStyle { family, size, weight, line_height, letter_spacing }`.
- **`spacing`** (`Spacing`) — named gaps: `xs` (4), `sm` (8), `md` (16), `lg` (24), `xl` (32), `xxl` (48).
- **`radius`** (`BorderRadius`) — named corner radii: `none`, `sm` (4), `md` (8), `lg` (12), `xl` (16), `full` (9999, for pills).
- **`animation`** (`AnimationConfig { enabled, duration_ms }`) — the single global switch described below.
- **`app_bar`** (`AppBarStyle`) — platform-adaptive `AppBar` defaults (title alignment, height, elevation), covered in its own section below.
- `is_dark` — `true`/`false`, so a widget can branch on theme brightness without inspecting colors directly.

## Built-in themes

`rosace-theme` ships four factory functions, re-exported from the prelude:

```rust
use rosace::prelude::*;

let d = dark_theme();  // the framework default
let l = light_theme();
let m = material();    // Android-flavored: light_theme() + a taller, elevated, left-aligned AppBar
let c = cupertino();   // iOS-flavored: system-blue accent + a centered, flat, 44pt AppBar
```

`App::new()` starts on `dark_theme()`. Both light and dark are a neutral, JetBrains-Darcula-inspired palette (not Material's purple-tinted defaults) — a deep violet accent (`#7C4DFF`) on light, a soft lavender (`#BB86FC`) on dark.

## Reading the theme

Call `use_theme()` anywhere you need the current tokens — typically inside `build`:

```rust
use rosace::prelude::*;

impl Component for Card {
    fn build(&self, _ctx: &mut Context) -> Element {
        let theme = rosace::theme::use_theme();

        Container::new()
            .padding(EdgeInsets::all(theme.spacing.md))
            .radius(theme.radius.lg)
            .child(Text::new("Themed card"))
            .into_element()
    }
}
```

`use_theme()` reads a global reactive atom ([GlobalAtom](../GLOSSARY.md#globalatom)) — components that call it are automatically subscribed, so a later `set_theme(...)` re-renders them like any other state change (see [Components & State](components-and-state.md)).

**A gotcha with colors specifically:** the prelude's `Color` (what `Container::background(...)`/`Text::color(...)` expect) is `rosace_render::Color` — 0–255 `u8` channels. `theme.colors.*` values are `rosace_theme::Color` — 0.0–1.0 `f32` channels, the token type. They're different types on purpose (the token type is what `Lerp`/theme math is built on), so passing a theme color straight into a widget builder call won't compile — convert channel-by-channel first:

```rust
fn to_render_color(c: rosace_theme::Color) -> Color {
    Color::rgba((c.r * 255.0) as u8, (c.g * 255.0) as u8, (c.b * 255.0) as u8, (c.a * 255.0) as u8)
}

Container::new().background(to_render_color(theme.colors.surface))
```

## Setting and switching themes

```rust
use rosace::prelude::*;

// At app startup:
App::new().theme(light_theme()).launch(MyApp);

// At runtime — e.g. a settings toggle:
rosace::theme::set_theme(dark_theme());

// Or just flip the animation switch without changing anything else:
rosace::theme::set_animations(false);
```

`set_theme` replaces the whole `ThemeData` and notifies every subscriber in one go. Build your own theme by starting from a built-in and overriding fields with struct-update syntax:

```rust
let brand = ThemeData {
    colors: ColorScheme {
        primary: rosace::theme::Color::from_hex(0xFF6B35),
        ..light_theme().colors
    },
    ..light_theme()
};
```

(Note `rosace::theme::Color`, not the prelude's `Color` — see the color-type gotcha below.)

## Custom theme values: `with_ext`/`ext`

`ThemeData`'s fixed fields cover the built-in widget set, but a custom widget often needs its own theme-controlled style struct without editing `rosace-theme` itself. The `ext` type-map does this:

```rust
#[derive(Clone, Copy)]
struct BadgeStyle { corner_radius: f32 }

let theme = light_theme().with_ext(BadgeStyle { corner_radius: 6.0 });

// Later, inside the widget:
let style = theme.ext::<BadgeStyle>().cloned().unwrap_or(BadgeStyle { corner_radius: 4.0 });
```

`with_ext`/`ext` key by the Rust type itself, so any number of custom widgets can each stash their own style struct on the same theme with no collisions.

## The global animation switch

`AnimationConfig { enabled: bool, duration_ms: f32 }` (default: on, 160ms) is the one dial that governs every theme-driven animated widget — `Switch`, `Checkbox`, `Radio`, `SegmentedControl`, and the built-in transitions described in the [Animation](animation.md) chapter. Turn it off globally (accessibility, testing, or user preference) with `set_animations(false)`; every governed widget snaps instead of easing. This is separate from the `use_animation`/`use_spring` hooks you drive explicitly in your own components — those keep running either way.

## Platform-adaptive theming: `Themes`

A single `.theme(...)` is enough for most apps. If you want iOS to look Cupertino and Android to look Material from the same codebase, hand the app a **`Themes` bundle** instead — a platform-keyed map with a required fallback:

```rust
use rosace::prelude::*;

let themes = Themes::new(light_theme())              // fallback: desktop, web, unlisted platforms
    .platform(Platform::Ios, cupertino())
    .platform(Platform::Android, material());

App::new().themes(themes).launch(MyApp);
```

The framework resolves this **once at startup**, keyed by the real running `Platform` — widgets never see the bundle, only the single resolved `ThemeData`, exactly like the single-`.theme(...)` path. To preview a platform's look while developing on desktop, force it:

```rust
App::new().themes(themes).platform(Platform::Ios).launch(MyApp);
```

`rsc new` wires this up for you automatically: if you scaffold a project that targets iOS and/or Android, it generates a `themes()` function in `src/theme.rs` and calls `.themes(theme::themes())` in `App::launch` — Cupertino on iOS, Material on Android, `light()` everywhere else.

## What `AppBarStyle` controls

Today the platform-adaptive surface is `AppBarStyle` — `title_align` (`Leading` vs `Center`), `show_traffic_lights`, `height`, and `elevation` (whether the bottom separator draws). `AppBar` reads these as its per-instance defaults; explicit builder calls like `.height(56.0)` on the widget itself always win over the theme.

---

**Under the hood:** why theme reads auto-subscribe, and how the reactive atom under `use_theme`/`set_theme` triggers rebuilds, is covered in [Core: Component, Element, Context](../architecture/core.md).

Next: [Forms & Text Input](forms-and-text.md).
