# Phase 23 — Platform-Adaptive Theming (D105)

> Status: PLANNED (not started)
> Started: —
> Completed: —
> Decision: **D105** — ONE widget set; the theme is the only platform authority.

## Why This Phase

Desktop, iOS, and Android chrome differ in real, structural ways — not just
color: macOS traffic-light inset; iOS centered title + edge-back gesture;
Android left-aligned title + elevation; Cupertino pill switch vs Material
track; scroll physics (rubber-band vs edge-glow); touch density (44pt / 48dp /
tight desktop). Today TEZZERA has **one** widget set with ad-hoc props
(`AppBar.show_traffic_lights`) and **zero platform awareness** — `ThemeData`
carries only global tokens (colors/typography/spacing/radius/animation).

We explicitly reject a second widget library (Material + Cupertino): double the
maintenance, against the one-concept ethos. Instead, **push every platform
difference into the theme (data)** so widget code stays platform-agnostic.

## The Model (SwiftUI / Flutter-ThemeExtension, minus the dual widget set)

- **One widget set.** A widget reads `ctx.theme` and renders; it NEVER branches
  on platform.
- **The framework resolves the active theme once.** At startup it detects the
  running platform and picks the active `ThemeData` from a platform-keyed
  bundle (with a required fallback). Widgets are dumb readers.
- **`ThemeData` carries per-widget Style structs** so a theme can change
  *structure*, not just color. "Material theme" and "Cupertino theme" are two
  `ThemeData` values with identical widget code.
- **`ThemeExtension` type-map** lets new/custom widgets add their own theming
  without editing core `ThemeData`.

## Decisions

- **D105** — Platform-adaptive theming (locked here). See DECISIONS.md.

## Steps

### Step 1 — Platform detection + active-platform on the theme
Add `enum Platform { Desktop, Ios, Android, Web, Windows, Linux, Macos }` (in
`tezzera-core` or `tezzera-theme`) and a startup detector: `cfg(target_os)` on
native, `navigator.platform`/user-agent on web. Store the resolved platform so
widgets/theme can read it (`ctx.theme.platform` or a global). No widget branches
on it directly — it drives theme resolution (Step 2) and is available for the
rare escape hatch.

Exit: `App::run` resolves and exposes the running platform; forced-override
(`.platform(Platform::Ios)` for preview) works on desktop.

### Step 2 — `Themes` bundle + `App::themes(..)` + fallback resolution
`Themes::new(fallback).platform(Platform::Ios, ios).platform(Platform::Android,
android)`. `App::themes(themes)` (keep `App` with no themes = single-theme
today, back-compat). The frame loop selects the active `ThemeData` =
`themes.get(running_platform).unwrap_or(fallback)` **once**, sets it as THE
theme (existing `set_theme` path). `set_theme(ThemeData)` still works for the
single-theme case.

Exit: an app given `ios`+`android` themes renders the iOS theme on the iOS
simulator and the Android theme on Android, both falling back on desktop; the
existing single-theme apps are unaffected.

### Step 3 — Per-widget Style structs in `ThemeData`; convert `AppBar` (the proof)
Introduce `AppBarStyle { title_align: Leading|Center, show_traffic_lights,
height, elevation, background/foreground overrides }` as a field on `ThemeData`.
`AppBar::build` reads `ctx.theme.app_bar` for its defaults; per-instance props
(`.traffic_lights()`, `.height()`, …) override the theme style. `material()`
sets `title_align: Leading`, `cupertino()` sets `Center` + traffic-lights off,
desktop default sets traffic-lights on for macOS. This is the vertical slice
that proves the whole model end-to-end.

Exit: the SAME `AppBar` renders macOS-style on desktop, centered-title iOS-style
under the cupertino theme, left-title Material under the material theme — no
platform branch in `AppBar`, only theme reads. Follow-ups: `ButtonStyle`,
`SwitchStyle`, then the rest, one widget at a time.

### Step 4 — `ThemeExtension` type-map
`ThemeData` gains `ext: HashMap<TypeId, Arc<dyn Any + Send + Sync>>` with
`ThemeData::with_ext<T>(T)` and `ctx.theme.ext::<T>() -> Option<&T>` (or a
`Default`-backed getter). A custom widget defines its own `MyStyle`, stashes it
in the theme, and reads it — no core `ThemeData` edit required.

Exit: a demo custom widget themes itself purely via an extension struct; adding
it required no change to `tezzera-theme`.

### Step 5 — Built-in `material()` / `cupertino()` + `tzr new` integration
Ship `built_in::material()` and `built_in::cupertino()` (structural, not just
color). Update `tzr new` (D104) so a project that selects iOS+Android is
scaffolded with a `Themes` bundle wiring `cupertino()` for iOS and `material()`
for Android + a fallback, in the generated `theme.rs`.

Exit: `tzr new app --platforms desktop,ios,android` produces an app that looks
native-appropriate on each target with no hand-editing.

## Migration Rule

Back-compat throughout: single-theme apps (`set_theme(ThemeData)` /
`App` with one theme) keep working unchanged. Per-widget Style structs land one
widget at a time; until a widget is converted it reads global tokens as today.
No widget may add a `cfg(target_os)` or runtime platform branch — platform
variance lives in the theme only.

## DO NOT

- Do not create a second widget set (Cupertino/Material widgets). One set.
- Do not branch on platform inside widget code — read the theme.
- Do not make `App::themes` mandatory — single-theme apps stay simple.
- Do not bake platform look into widget defaults — it belongs in the theme's
  per-widget Style structs.
