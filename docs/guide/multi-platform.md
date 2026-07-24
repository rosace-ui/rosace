# Multi-Platform & the `rsc` CLI

The same `Component` tree you built in earlier chapters runs on desktop, web, iOS, and Android without changes to your UI code. This chapter covers the pieces that *are* platform-specific: the `Platform` abstraction your app can read, and the `rsc` commands that scaffold, run, and package each target.

## The `Platform` enum

ROSACE detects which OS it's running on at startup and exposes it as a flat enum:

```rust
pub enum Platform {
    MacOs,
    Windows,
    Linux,
    Ios,
    Android,
    Web,
}
```

`Platform::MacOs`/`Windows`/`Linux`/`Ios`/`Android`/`Web` are deliberately separate variants rather than one `Desktop` bucket — macOS needs its traffic-light-aware app bar, Windows and Linux don't, so folding them together would just need un-folding later.

Read it with `rosace::core::use_platform()`, or the two coarse helpers:

```rust
use rosace::core::use_platform;

let platform = use_platform();
if platform.is_mobile() {   // Ios | Android
    // ...
}
if platform.is_desktop() {  // MacOs | Windows | Linux
    // ...
}
```

**Widgets never branch on `Platform` directly** — the intended use is driving *theme* selection. A `Themes` bundle picks the active `ThemeData` once at startup based on the running platform (covered in the Theming chapter); most app code should reach for the resolved theme, not re-derive platform-specific styling by hand. You can force an override for local testing — `App::new().platform(Platform::Ios)` previews the iOS theme on your desktop machine — set once at launch, not meant for runtime switching.

## `rsc new` — scaffold an app

```bash
rsc new my-app --platforms macos,ios,web --bundle-id dev.example.my-app
cd my-app
```

- `--platforms <list>` — comma-separated: `macos,windows,linux,web,ios,android`. Skips the interactive prompt.
- `--all` — every platform.
- `--bundle-id <id>` — your app's bundle/package identifier (e.g. `dev.example.my-app`), used in `Info.plist`, the Android manifest, etc. Skips that prompt too.
- With neither flag, `rsc new` interactively asks which platforms to target (defaulting to your host OS) and for a bundle id.

Each selected platform gets its real, buildable project scaffolding, not a stub:

- **macOS/Windows/Linux** — a `Cargo.toml` binary target plus the platform's icon/config file (entitlements, a manifest, a `.desktop` entry respectively).
- **Web** — a `web/index.html` host page ready for `wasm-bindgen`.
- **iOS** — an actual Xcode project (`ios/`) with `Info.plist` and Swift glue that calls into your Rust code through an FFI boundary — not just a manifest file.
- **Android** — a real Gradle project (`android/`) with the JNI bridge wired up.

Run `rsc new --help` for the full flag list.

## `rsc doctor` and `rsc devices`

Before building for a platform, check your toolchain:

```bash
rsc doctor
```

This is read-only — it never installs anything — and reports what's present/missing plus the exact command to fix it, for: the Rust toolchain, macOS/Xcode/iOS, Android (SDK/NDK/Gradle/adb), Windows/Linux cross-compilation, and web (`wasm32-unknown-unknown`).

```bash
rsc devices
```

Lists available run targets across every platform — iOS simulators, Android emulators/connected devices — each with an ID you can pass to `rsc run --device`.

## `rsc run` — build and run on a target

```bash
rsc run                      # defaults to your host OS (macOS/Windows/Linux)
rsc run --target web         # or: --mac / --win / --lnx for desktop shorthand
rsc run --target ios --device "iPhone 15 Pro"
rsc run --target android
rsc run --target web --port 8080
```

`rsc run` hides the manual steps per target: `wasm-bindgen` + serving for web, codesign + `simctl` for iOS, `gradlew`/`adb` for Android. It reads `rsc.toml` for your app name and bundle id. `--device <name>` takes an ID from `rsc devices` (an iOS simulator name/UDID or an Android adb serial); left unset, iOS defaults to a sensible simulator and Android auto-detects whatever's connected.

## `rsc package` — ship it

```bash
rsc package --name "My App" --version 1.0.0 --out dist/
```

Bundles a distributable artifact for the current platform (`.app` on macOS, `.deb` on Linux, `.exe`/installer on Windows) into `--out` (default `dist/`). On macOS, pass `--identity "Developer ID Application: ..."` to code-sign for real distribution — without it, packaging uses an ad-hoc signature, which runs locally but won't pass Gatekeeper for others.

## `rsc build` — just build, no run

```bash
rsc build --target desktop
rsc build --target web
```

A release build for a target without launching it — useful in CI.

## Platform-specific gotchas

- **Web has no GPU path yet.** Rendering falls back to a CPU canvas blit; this is an explicit out-of-scope for the current GPU-migration work, not an oversight.
- **Web SEO/accessibility**: `rsc build`'s web output embeds a build-time semantic-tree HTML shadow (a `#rsc-seo` shadow-DOM host) so crawlers and assistive tech see real content, not a blank canvas. At runtime, `rosace-platform` keeps that shadow DOM in sync as your app's state changes — cheaply, with a string-diff gate so an unchanged frame never touches the DOM. `rsc dev`'s web mode attaches a fresh shadow root on the fly since it skips the build-time export step.
- **iOS/Android are real native host projects**, not a generic wrapper — the scaffolded Xcode/Gradle projects are meant to be opened and inspected like any other iOS/Android project when you need to touch native-only concerns (push notification entitlements, permissions, App Store metadata).
- **`Platform::detect()` is compile-time**, not a runtime query — it's `cfg(target_arch = "wasm32")` for web and `cfg(target_os = ...)` for everything else. There is exactly one way each target compiles, so there's nothing to detect at runtime.

---

**Under the hood:** the platform abstraction, the winit-based unified app loop (desktop and web share one event-loop implementation), and the `rsc` command internals are covered in the architecture book — see `../architecture/platform-and-app-loop.md` and `../architecture/cli.md`.

Next: [Hot Reload](hot-reload.md).
