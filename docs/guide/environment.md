# System Environment

Beyond your app's own state, the OS itself carries signals your UI should react to: is the system in dark mode right now, has the user cranked up accessibility text size, do they want bold text everywhere, do they want animations to snap instead of ease, do they prefer a 24-hour clock. ROSACE surfaces all five as one bundle — `MediaQuery` — kept live and in sync with the real OS setting on every platform.

## `use_media_query()`

```rust
use rosace::prelude::*;

impl Component for Banner {
    fn build(&self, _ctx: &mut Context) -> BoxedWidget {
        let mq = rosace::media_query::use_media_query();

        Text::new(if mq.is_dark { "Night mode" } else { "Day mode" })
            .boxed()
    }
}
```

`MediaQuery` is a small `Copy` struct:

```rust
pub struct MediaQuery {
    pub text_scale: f32,             // OS accessibility text-size multiplier, 1.0 = default
    pub is_dark: bool,                // OS is in dark mode right now
    pub bold_text: bool,              // OS "bold text everywhere" accessibility toggle
    pub reduce_motion: bool,          // OS "reduce motion" accessibility toggle
    pub always_24_hour_format: bool,  // OS/locale prefers a 24-hour clock
}
```

`use_media_query()` reads a global reactive atom — same pattern as `use_theme()` — so a component that calls it re-renders automatically the next time the OS pushes a new value; you never poll for changes yourself.

## What's already wired for you

You rarely need to read `MediaQuery` directly, because the framework already applies most of it at the few choke points every widget goes through:

- **`text_scale`** is applied automatically wherever text is measured or drawn (`FontCache::measure_text`, `PaintCtx::text`/`text_styled`) — a widget asking to draw at `14.0`px actually renders (and lays out) at `14.0 * text_scale`px, with zero per-widget code.
- **`is_dark`** drives the active theme automatically, unless you've pinned one — see "Following OS brightness" below.
- **`bold_text`** and **`reduce_motion`** are applied at the same text/animation choke points `text_scale` uses (see `rosace-widgets/src/tree/mod.rs`'s `bold_text_weight` helper).
- **`always_24_hour_format`** is detection-only today — no built-in widget has a 24-hour clock mode yet, so this field exists for your own components to read.

## Following OS brightness

`ThemeMode` controls whether your theme tracks the OS or stays pinned:

```rust
use rosace::theme::{ThemeMode, use_theme_mode, set_theme_mode, register_theme_pair};

// Default: ThemeMode::System — the active theme follows `MediaQuery.is_dark`
// automatically. Pin it if you want a manual light/dark toggle instead:
set_theme_mode(ThemeMode::Dark);   // locked, ignores the OS from now on
set_theme_mode(ThemeMode::System); // re-enable following the OS
```

If your app has its own customized light/dark pair (not the plain built-ins), register it once so system-follow uses *your* themes instead of the generic defaults:

```rust
register_theme_pair(my_light_theme(), my_dark_theme());
```

## Platform coverage

Every platform pushes `MediaQuery` the same way — a real OS notification, not a poll — through one shared entry point (`rosace_core::set_media_query`):

| Platform | OS signal | Native call site |
|---|---|---|
| iOS | `traitCollectionDidChange` + `NotificationCenter` (content-size-category) | generated Swift host, `rsc_engine_set_media_query` (FFI) |
| Android | `Activity.onConfigurationChanged` | generated Kotlin host, `nativeSetMediaQuery` (JNI) |
| macOS / Windows / Linux | winit's `WindowEvent::ThemeChanged` | `rosace-platform` |
| Web | `window.matchMedia(...)` + `"change"` listener on `prefers-color-scheme`/`prefers-reduced-motion` | `rosace-platform` |

Desktop/web currently only detect brightness and reduce-motion (no OS-wide accessibility text-scale concept distinct from DPI exists there — DPI is handled separately). iOS/Android report all five fields.

---

**Under the hood:** how a push from a native OS callback reaches your components as an ordinary reactive read — including the dirty-tracking fix that makes it apply on the very next frame instead of an unpredictable delay — is covered in [Platform & the App Loop](../architecture/platform-and-app-loop.md).

Next: [Forms & Text Input](forms-and-text.md).
