# TEZZERA — CRATE CONTRACTS
> Each crate has one job. It does that job and nothing else.
> Dependencies only flow downward.
> A crate never reaches into another crate's internals.

> **Rewritten 2026-07-08** against the actual workspace (34 crates) — the
> previous version was Phase-1 planning fiction, frozen since early
> development: it covered 16 of 34 crates, named two crates that don't exist
> (`tezzera-ffi`, `tezzera-test`), and described widgets (`Navigator`,
> `Snackbar`, `BottomSheet`, `SizedBox`) under the wrong crate or not shipped
> at all. Every entry below was verified against each crate's actual
> `src/lib.rs` and `Cargo.toml`, not inferred from the old document.

---

## DEPENDENCY HIERARCHY

The only place the intended layering was actually written down was a header
comment in `tezzera/Cargo.toml` — reproduced here as the source of truth,
extended to place the 22 "service" crates the original 8-layer sketch didn't
individually order (see **Known Issues** below — that gap is exactly how two
real violations went unnoticed).

```
Layer 0  tezzera-trace        zero tezzera deps
         tezzera-macros       zero tezzera deps (proc-macro crate)
         tezzera-compositor   zero tezzera deps (GPU presenter; external wgpu only)
Layer 1  tezzera-state        → trace
Layer 2  tezzera-core         → trace, state
Layer 3  tezzera-layout       → core, trace
Layer 4  tezzera-render       → core, layout, trace
         tezzera-theme        → core, state, trace
Layer 5  (services — see below) → layers 0-4 only, in principle
Layer 6  tezzera-widgets      → layers 0-5
Layer 7  tezzera              → everything (app code only depends on this)
Layer 8  tezzera-ffi          → tezzera + layers 0-6 (D106 Phase 24 Step 1;
                                 the first crate that depends on `tezzera`
                                 itself, not just its layers — a native-host
                                 adapter consumed by generated app code, not
                                 by `tezzera`)
```

**Layer 5 services** (22 crates; the original doc only ever named a handful
of these and never gave them a relative order — see Known Issues):
`tezzera-anim`, `tezzera-animate`, `tezzera-scroll`, `tezzera-nav`,
`tezzera-nav-anim`, `tezzera-hot-reload`, `tezzera-devtools`,
`tezzera-forms`, `tezzera-a11y`, `tezzera-gesture`, `tezzera-net`,
`tezzera-text`, `tezzera-shaping`, `tezzera-bidi`, `tezzera-i18n`,
`tezzera-clipboard`, `tezzera-ws`, `tezzera-ime`, `tezzera-media`,
`tezzera-style`, `tezzera-platform`, `tezzera-test-utils`.

**Rule**: If crate A is above crate B in this hierarchy, A cannot depend on
B. Ever. Enforced today only by convention (never adding `tezzera` as a dep
inside any sub-crate) — there is no automated check.

---

## KNOWN ISSUES (found during the 2026-07-08 audit, not yet fixed)

These are real findings, not just doc staleness — flagged here rather than
silently baked into the contracts below, per the project's violation policy
(§ bottom of this file: redesign or document, never silently absorb).

1. **`tezzera-anim` is dead code.** It compiles and is re-exported as
   `tezzera::anim`, but grepping the whole workspace for `tezzera_anim::`
   finds zero consumers outside its own crate and that one re-export.
   `tezzera-animate` (a similarly-named, independently-implemented crate) is
   the one actually used by `tezzera-widgets`, `tezzera-platform`,
   `tezzera-nav-anim`, and `tezzera-examples`. These are NOT a shim/alias
   pair — they're two real, separately-maintained animation systems, and one
   of them is unused. Needs a decision: remove `tezzera-anim`, or find/state
   its intended purpose.
2. **`tezzera-gesture` depends on `tezzera-platform`.** Both are Layer-5
   services; the documented layer rule only governs Layers 0–4 → nothing
   stops one service depending on another. This ties gesture recognition
   (conceptually input-source-agnostic) to the concrete windowing crate.
   Not caught by any existing rule because Layer 5 has no internal ordering.
3. **`tezzera-test-utils` depends on `tezzera-platform`** (winit/softbuffer)
   for what's meant to be lightweight headless test infrastructure, AND is
   pulled into the umbrella `tezzera` crate as a normal `[dependencies]`
   entry — contradicting its own doc comment ("use it only in
   `[dev-dependencies]`"). Test/snapshot infrastructure currently ships
   inside the production SDK crate. **Explicitly DEFERRED (user, 2026-07-08)**
   — known, not urgent, not being fixed right now.
4. **`tezzera-style` is unintegrated.** A CSS-like stylesheet system exists
   (selectors, rules, computed/inline styles) but `tezzera-widgets` does not
   depend on it — nothing in the actual widget rendering path uses it.
   **Open question raised 2026-07-08**: do we need this at all, given
   `tezzera-theme` already provides a design-token/theme system? Not
   resolved — could be (a) genuinely redundant with theme and worth
   deleting, (b) intended for a different job theme doesn't cover (e.g.
   arbitrary per-instance style overrides via a selector/cascade model,
   closer to inline CSS than design tokens), or (c) a mid-migration
   leftover nobody finished wiring up. **Explicitly DEFERRED (user,
   2026-07-08)** — no action now; revisit the question above before
   deciding anything.
5. **`tezzera-cli`'s Windows packaging/build support is generated but
   unverified.** `tzr new`'s `windows/` output (`icon.ico`, `app.manifest`),
   `tzr run --win`, and `package.rs::bundle_windows` all exist and
   type-check (confirmed via `cargo check --target x86_64-pc-windows-gnu` —
   no Windows rustup target/toolchain was installed on this machine before
   this work, so it had never been compile-checked at all before), but
   nothing here has been through a real Windows build+run, because no
   Windows execution environment or cross-linker (mingw-w64) exists on the
   machines this was developed on. `windows/app.manifest` deliberately uses
   a side-by-side manifest (`<exe>.exe.manifest`, loaded automatically by
   Windows with no resource compiler) rather than `rc.exe` icon-in-exe
   embedding, specifically because the side-by-side form is inspectable
   without a Windows toolchain and `rc.exe` embedding isn't. `tzr run --win`
   preflights the rustup target and prints install instructions
   (`rustup target add x86_64-pc-windows-gnu` + `brew install mingw-w64`)
   rather than pretending to succeed; it only ever cross-*builds*, never
   attempts to run the result. Flag this whole area for a real end-to-end
   verification pass once a Windows environment is available. Linux
   packaging (`bundle_linux`) is in a similar but lesser position: also
   type-checked via a cross target (`x86_64-unknown-linux-gnu`) rather than
   run on a real Linux box, but Linux's toolchain requirements are much
   lighter (no separate resource compiler / manifest system to get right),
   so the residual risk is smaller.
6. **`tzr new`'s Android support (D106 Phase 24 Step 3) is unverified on a
   real device/emulator.** Unlike the Windows gap above, the NDK
   cross-compile and Gradle build ARE both verified for real on this
   machine — `cargo build --target aarch64-linux-android` produces a
   genuine ARM64 `.so` with correctly JNI-mangled exported symbols
   (confirmed via `llvm-nm -D`), and `./gradlew assembleDebug` against a
   real `tzr new --platforms android`-scaffolded project produced a real
   APK containing that `.so` plus compiled Kotlin (`classes.dex`). What's
   NOT verified: actually installing and launching on an emulator or
   device. No Android system image could be installed on this machine
   without risking a disk-full failure (free disk hovered around 2-3GB for
   most of this work; a system image download is 1GB+) — `adb devices`
   was empty throughout. `tzr run --target android` handles this honestly
   (builds, detects no device, prints a manual `adb install` instruction)
   rather than claiming success. `MainActivity.kt`'s safe-area handling is
   also a known simplification — it passes zero insets to `nativeResize`
   rather than deriving them from a real `WindowInsets` callback (iOS's
   Step 2 equivalent uses real `UIView.safeAreaInsets`). NDK/linker
   discovery in the generated `cargoBuildAndroid` Gradle task only handles
   macOS/Linux/Windows x86_64 host tags, not ARM hosts. Flag all of this
   for a real on-device verification pass once an Android environment
   (device or a machine with enough disk for an emulator image) is
   available.
7. **Web (D107 Phase 25) leftovers, flagged 2026-07-09 when the phase
   completed — none blocking, all explicitly deferred rather than fixed:**
   - **No per-route static SEO export.** `tzr build --target web`'s
     build-time semantic-HTML export runs `AppRoot::build()` exactly once,
     with no forced navigation — it only captures the app's default/
     initial screen. Content behind `nav.push(...)` (e.g. a counter
     screen reached via a list tile) isn't in the crawler-visible HTML
     until wasm loads and Step 4's runtime shadow-DOM sync catches up. A
     real per-route export would need the extractor to iterate every
     `Screen` variant, forcing navigation state per iteration.
   - **Two independently hand-maintained `index.html` templates.** `tzr
     new`'s `web_index_html()` (writes `web/index.html`) and `tzr build`'s
     `generate_index_html()` (writes the real `dist/index.html` output)
     are separate functions kept in sync by hand — `build_web()` never
     reads the scaffolded `web/index.html` at all. Already caused one real
     bug this session (`generate_index_html()` hardcoding `app.js`/
     `app.wasm` instead of the real crate name); a second divergence could
     recur silently.
   - **`llms.txt` output isn't spec-compliant** with the emerging
     `llmstxt.org` markdown convention (`#`/`>` headers, link lists) — it's
     a flat plain-text summary. That was the phase's actual design intent
     (see `PHASE_25.md`'s Step 3 description: "plain-text summary"), not
     an oversight, but flag it if strict convention compliance is ever
     wanted.
   - **VoiceOver (the real macOS screen reader) was never run.** Step 5
     verified spec-correct DOM structure and Chrome's own accessibility
     tree (the same underlying tree VoiceOver consumes on this platform),
     but not VoiceOver itself — that needs a human, or explicit
     authorization to toggle a system-wide accessibility feature via
     automation.
   - **GPU rendering is disabled on web.** `GpuPresenter` (wgpu) is gated
     off entirely on `wasm32` — web always uses the `softbuffer` CPU
     fallback path. Never revisited for whether web should get
     GPU-accelerated rendering (e.g. via `wgpu`'s WebGL/WebGPU backends).
   - **Web has no broader widget/layout test coverage** beyond the
     scaffolded counter MVP that's been used for verification throughout
     Phases 25 and the earlier multiplatform work.
8. **`tezzera-state`'s `frame_scheduler` tests race each other under
   `cargo test --workspace`'s parallel execution.** `request_frame_sets_flag`
   /`take_clears_flag`/`multiple_requests_collapse_to_one`
   (`tezzera-state/src/frame_scheduler.rs`) all read/write the same
   process-global frame-requested flag with no per-test isolation — flaky
   only under full-workspace parallel load, always passes when run in
   isolation (`cargo test -p tezzera-state`). Found 2026-07-09 while
   verifying Phase 26 Step 2 (unrelated code); not fixed here since it's
   pre-existing and outside that step's scope. Same class of bug Phase 26
   Step 2 itself hit and fixed in `tezzera/src/engine.rs`'s new tests (a
   `static ... Mutex` guard around tests that mutate global state) — the
   same fix shape would apply here.
9. **`ScrollPhysics::Bounce` (D108/Phase 26 Step 2) still jitters/oscillates
   on real macOS trackpad input — unresolved after multiple real-device
   debugging rounds, left as a known issue rather than silently shipped as
   working.** Drag-to-pan and the underlying momentum primitives
   (`ScrollController::apply_momentum`/`coast`/`settle_bounce`,
   `tezzera-scroll`) are real, tested, and correct in isolation (44 unit
   tests, including regression tests for every bug found this round — unit
   mismatch, frame-rate-dependent decay, unbounded overscroll, velocity
   clamping). The unresolved part is specifically the FEEL of `Bounce`
   physics driven by real trackpad wheel events, confirmed via a real
   screen recording (frame-extracted with a one-off Swift/AVFoundation
   script, not assumed): the view visibly settles at the content edge,
   overscrolls again on its own about a second later, then re-settles — a
   genuine oscillation, not a one-off glitch. Root-caused (not guessed) to
   a real platform quirk: macOS delivers trackpad momentum as the OS's own
   native event stream after the user's fingers lift
   (`NSEvent.momentumPhase`, confirmed by reading winit 0.30.13's actual
   macOS backend source at `platform_impl/macos/view.rs`'s
   `scroll_wheel:`), and winit collapses both real finger movement and
   that OS momentum tail into the same `TouchPhase::Moved` — there is no
   reliable way to tell them apart from the event alone. The fix applied
   (wheel input applies directly via `apply_momentum`, no longer injects a
   synthetic velocity for TEZZERA's own `coast` to decay, avoiding a second
   momentum source fighting the OS's) measurably changed the failure mode
   across rounds but did not fully eliminate it in the reporter's live
   testing. Real remaining candidates, not yet tried: (a) the
   `WHEEL_IDLE_GRACE`/`COAST_STOP_THRESHOLD`/`MAX_VELOCITY` constants
   (`tezzera-scroll/src/controller.rs`) may need further tuning against
   real hardware rather than calculation alone; (b) `bounce_axis`'s
   resistance model (35% flat factor, no true spring/mass simulation) may
   need to become a real damped spring; (c) properly distinguishing
   momentum-phase from user-phase wheel events would need
   `tezzera_platform::InputEvent::Scroll` to carry that info end-to-end
   (winit exposes it at the `WindowEvent::MouseWheel` boundary today but
   `tezzera-platform/src/app.rs`'s conversion discards it) — real plumbing
   work, not attempted here. `ScrollPhysics::Momentum` (the non-iOS/macOS
   default) was not implicated in any of the live testing and is not
   suspected of the same issue, since it has no overscroll/spring-back
   phase to fight the OS's momentum tail. Revisit with either more
   real-device iteration time or the plumbing work in (c).

None of these are fixed by this doc rewrite — this is the audit that found
them. Fixing them (removing `tezzera-anim`, reordering `gesture`, moving
`test-utils` to dev-deps, deciding `tezzera-style`'s fate) is separate work.

---

## CRATE CONTRACTS

---

### tezzera-trace  (Layer 0)
**Job**: The framework's tracing/instrumentation bus — a global event bus
(`TRACING_BUS`) that every other crate emits structured events to.
**Exports**: `TezzeraTrace` (event enum), `TracingBus`, `TRACE_BUS`,
`TraceSubscriber` (trait), plus `pub mod bus`/`event`/`subscribers`.
**Must NOT**: Depend on any other `tezzera-*` crate. Contain UI logic.

---

### tezzera-macros  (Layer 0)
**Job**: Proc-macro crate — component/state codegen sugar and the `view!`
declarative element-tree DSL.
**Exports**: `#[component]`, `#[state]`, `view!`.
**Must NOT**: Depend on any other `tezzera-*` crate (verified: it doesn't).
Contain runtime logic — everything here is macro expansion only.

---

### tezzera-compositor  (Layer 0)
**Job**: GPU compositor — uploads CPU RGBA pixel buffers (from `SkiaCanvas`)
as wgpu textures and blits them onto the window surface via a fullscreen-quad
shader, with per-layer dirty-tracking to skip redundant uploads/presents.
**Exports**: `GpuPresenter` (`new`/`resize`/`present`/`present_layers`/
`surface_size`), `CompositorLayer` (+ `opaque`/`tracked`/`placed`
constructors), `LayerRect`.
**Must NOT**: Depend on any `tezzera-*` crate (verified: it doesn't — only
external `wgpu`/`pollster`/`log`). Know about widgets, layout, or components.
**Note**: a model example of correct layering — sophisticated and
framework-specific, yet zero internal deps. `tezzera-platform` is its only
consumer.

---

### tezzera-state  (Layer 1)
**Job**: Reactive atom-based state — `Atom<T>`/`GlobalAtom<T>` values that
auto-subscribe reading components, plus the `RefreshEngine` that computes
minimal rebuild sets.
**Exports**: `Atom`, `GlobalAtom`, `AsyncState`, `batch`/`is_batching`,
`Priority`, `RefreshEngine`, `mark_dirty`/`is_global_dirty`/
`take_dirty_components`, `request_frame`/`take_frame_requested`,
`scroll_offset`/`set_scroll_offset`/`scroll_offset_by` (the non-reactive
scroll channel, D090), `hook_state`.
**Must NOT**: Depend on anything but `tezzera-trace`. Know about layout or
rendering.

---

### tezzera-core  (Layer 2)
**Job**: The component/element model — `Component`, `Element`, `Context`,
`App`, plus shared geometric/id types used everywhere.
**Exports**: `App`, `Component`, `Context`, `Element`/`NativeElement`/
`ComponentElement`/`TextElement`, `ChildContainer`, `ErrorBoundary`,
`TezzeraError`/`TezzeraResult`, `AxisBound`/`Canvas`/`Constraints`/
`RenderObject`, `SafeArea`/`use_safe_area`/`set_safe_area` (D106 groundwork),
`Role`/`SemanticNode` (D099 accessibility tree — see also `tezzera-a11y`,
which has its own, richer `Role`; not yet unified, see D107 planning notes),
`AtomId`/`ComponentId`/`Key`/`Location`/`Point`/`Rect`/`Size`.
**Must NOT**: Know about specific widgets. Implement layout algorithms. Touch
rendering. Depend on anything but `trace`, `state`.

---

### tezzera-layout  (Layer 3)
**Job**: Constraint-based layout math (`Flexure`) plus the handful of
layout-only widgets that are pure geometry (no painting of their own beyond
what their children provide).
**Exports**: `Constraints`/`AxisBound` (re-exported from `tezzera-core`),
`Flexure`, `LayoutResult`, `CrossAxisAlignment`/`MainAxisAlignment`,
`Height`/`Width`, `layout_column`/`layout_row`, `AspectRatio`, `Flex`/
`FlexDirection`, `Grid`, `Wrap`.
**Must NOT**: Call into `tezzera-render` (rasterization is a higher layer).
Know about navigation, animation, or theme.
**Note**: most user-facing layout widgets (`Column`, `Row`, `Stack`,
`Container`, `ScrollView`, ...) actually live in `tezzera-widgets`, NOT here
— this crate is the underlying constraint-solving math plus a few widgets
(`AspectRatio`, `Grid`, `Wrap`) that are pure geometry with no paint logic of
their own beyond delegating to children.

---

### tezzera-render  (Layer 4)
**Job**: Software rasterizer and display-list recording — `SkiaCanvas`
(tiny-skia backed), `PictureRecorder`/`Picture` for recording/replaying draw
commands, `FontCache` for glyph rasterization.
**Exports**: `Color`, `SkiaCanvas`, `DrawCommand`, `FontCache`, `Picture`/
`PictureRecorder`, `ImageHandle`/`ImageFit`/`CachePolicy`.
**Must NOT**: Implement layout algorithms. Know about navigation or
animation state. Depend on anything but `core`, `layout`, `trace`.

---

### tezzera-theme  (Layer 4)
**Job**: Design tokens and the reactive global theme — colors, typography,
spacing/radius/shadow scales, `use_theme()`/`set_theme()`.
**Exports**: `ThemeData`, `ColorScheme`, `Color`, `Typography`/`TextStyle`/
`FontFamily`, `Spacing`, `BorderRadius`, `Shadows`/`ShadowLayer`,
`AnimationConfig` (the theme-global animation toggle, not per-widget),
`set_theme`/`use_theme`/`set_animations`, `built_in::{light_theme,
dark_theme}`.
**Must NOT**: Implement rendering or layout. Know about specific widgets.
Depend on anything but `core`, `state`, `trace`.

---

### tezzera-widgets  (Layer 6)
**Job**: The built-in widget library — the primary tree app authors build
UIs from (`Column`/`Row`/`Stack`/`Container`, buttons/inputs/dialogs, the
overlay system, scroll views, etc.) plus the widget-authoring plumbing
(`Widget` trait, `PaintCtx`, hit-testing/focus/semantics declarations —
Phase 21, see `.steering/WIDGET_AUTHORING_GUIDE.md`).
**Exports** (representative — the real list is ~60 items):
- Protocol: `Widget`, `BoxedWidget`, `Children`, `PaintCtx`, `Semantics`,
  `HitTarget`/`ScrollTarget`, `WidgetApp`
- Structure: `Column`, `Row`, `Stack`, `Container`, `Scaffold`, `Positioned`,
  `Expanded`, `Spacer`, `ScrollView`/`ScrollAxis`, `ListView`,
  `ScreenTransitionView` (D108/Phase 26 Step 3 — paints a `ScreenNav`
  transition's outgoing/incoming screens at their spring-eased offsets;
  not generic over the app's route enum), `Hero`/`HeroApi` (D108/Phase 26
  Step 5 — `.hero_tag(id)` on any widget; a pass-through outside a
  transition, morphs into a floating copy between two same-tagged widgets
  on the outgoing/incoming screens during one — see `hero.rs`'s own doc
  comment for the full mechanism; deliberately does NOT use `tezzera-core`'s
  `Key`/`Element::with_key`, confirmed still completely dead — no widget
  sets it, the reconciler never reads it, real node identity is purely
  positional — Hero identity is a separate, out-of-band tag registry
  instead)
- Components: `AppBar`, `Avatar`, `Badge`, `Button`, `Card`, `Checkbox`,
  `Chip`, `Dialog`, `Divider`, `Dropdown`, `Drawer`, `Expander`, `Radio`,
  `SegmentedControl`, `Menu`, `NavRail`, `ProgressBar`/`CircularProgress`,
  `Sheet`, `Toast`, `Slider`, `Switch`, `TabBar`, `TextInput`, `Tooltip`,
  `ListTile`, `Skeleton`
- Overlay system: `push_overlay`/`drain_overlays`/`clear_overlays`,
  `OverlayEntry`/`OverlayKind`/`LayerPosition`, `FocusApi`
- Text/image: `Text`, `Image`/`ImageWidget`/`ImageCache` (`ImageCache` is
  orphaned — `Image::paint` decodes PNGs inline via `std::fs::read` +
  `tiny_skia::Pixmap::decode_png` on every paint call, uncached, and never
  touches `ImageCache` at all; found during D108/Phase 26 Step 4, same
  duplicate-parallel-implementation shape as `tezzera_scroll::ScrollView`
  and `Navigator`/`NavigatorAnimated`)
- Escape hatches: `CustomPaint`, `RepaintBoundary`, `TransformLayer`,
  `RectReader`
**Must NOT**: Bypass the `Widget`/render-tree protocol. Import internals of
lower crates directly instead of their public API.
**Depends on**: `a11y`, `animate` (not `anim` — see Known Issues), `nav`
(D108/Phase 26 Step 3, for `ScreenTransitionView`), `scroll`, `text`, plus
`core`/`state`/`layout`/`render`/`theme`/`trace`. Does NOT depend on
`tezzera-style` (see Known Issues).

---

### tezzera-cli  (`tzr`, Layer 7 — bin only)
**Job**: The `tzr` command-line tool.
**Commands** (`src/commands/`): `new` (scaffold an app — D104/D106),
`dev` (dev server, trace output), `build`, `run` (per-platform build/run:
desktop/web/iOS — D106), `package`, `analyze` (workspace health), `snapshot`,
`workspace`.
**Depends on**: `trace`, `core`, `hot-reload`.
**Must NOT**: Contain framework logic. Be imported by any other crate.

---

### tezzera-examples  (Layer 7 — bin/example only)
**Job**: Sample apps exercising the framework end-to-end.
**Contents**: `src/bin/{counter, animated_counter, app_showcase, app_demo,
widget_authoring_demo}.rs`, `examples/web.rs` (wasm cdylib entry, browser
MVP).
**Depends on**: the umbrella `tezzera` crate, `animate`, `state`.
**Must NOT**: Be imported by any other crate.

---

### tezzera-ffi  (Layer 8 — native host adapter; D106 Phase 24 Step 1)
**Job**: The C-ABI boundary a native iOS/Android host (Swift `AppDelegate`,
Kotlin `Activity`) drives instead of winit owning the app lifecycle — see
`.steering/PHASE_24.md`.
**Contents**: `Engine` (`src/engine.rs` — wraps `tezzera::FrameEngine` +
`tezzera_compositor::GpuPresenter` + base/overlay `SkiaCanvas`, exposing safe
`init`/`resize`/`input`/`frame`), `RawSurface` (`src/surface.rs` — implements
`wgpu::rwh::HasWindowHandle`/`HasDisplayHandle` directly from a raw
`CAMetalLayer*`/`ANativeWindow*` pointer; the only `unsafe` in the crate),
`TzrInputEventFfi` (`src/event.rs` — one flat `#[repr(C)]` struct covering
every `InputEvent` variant via a `kind` tag), `include/tzr_engine.h`
(hand-written C header, not `cbindgen` — the surface is small and stable).
**Must NOT**: Export `#[no_mangle] extern "C"` functions itself — only a
concrete app knows its root `Component`, so the actual `tzr_engine_init/
resize/input/frame/shutdown` symbols are per-app glue (~15 lines; see
`examples/ios_stub.rs` for the reference pattern Step 2's `tzr new` codegen
follows). Never imported by `tezzera` or any lower layer — dependency flows
one direction only, `tezzera-ffi` → `tezzera`, never the reverse.
**Depends on**: `tezzera` (for `FrameEngine`), `tezzera-core`,
`tezzera-platform` (for `InputEvent`/`ScrollLayer`), `tezzera-render`,
`tezzera-theme`, `tezzera-state`, `tezzera-compositor`, `wgpu` (only for the
`wgpu::rwh` re-export of `raw-window-handle`, pinned to the same version
`tezzera-compositor` uses).

---

### tezzera-platform  (Layer 5 — service)
**Job**: Windowing/platform integration — owns the winit event loop and
window, translates OS input into `InputEvent`, provides the wasm32 web entry
point (`run_web`), tracks scroll-layer state for the compositor.
**Exports**: `PlatformWindow`, `InputEvent`/`MouseButton`/`Key`,
`ScrollLayer`/`publish_scroll_layers`/`take_scroll_layers`, `run_web`
(wasm32-only).
**Depends on**: `animate`, `compositor`, `core`, `render`, `state`, `trace`;
external `winit`, `softbuffer`, plus wasm32-only `wasm-bindgen`/`web-sys`.
**Must NOT**: Implement widgets. Know about navigation routes.
**Note (D106)**: on iOS this crate's winit-owns-the-app-lifecycle model is
being replaced by a real native host (Xcode project + our own AppDelegate) —
see D106/`PHASE_24.md`. Desktop/web keep winit.

---

### tezzera-anim  (Layer 5 — service, DEAD CODE, see Known Issues)
**Job as implemented**: Pure-math animation primitives (`Tween`, `Easing`,
`Timeline`/`Keyframe`, `AnimationController`) explicitly documented as
"driven by external dt," with no hook/state integration.
**Exports**: `easing_fn`/`Easing`, `Lerp`, `Keyframe`/`Timeline`,
`AnimationController`/`AnimationState`, `Tween`.
**Depends on**: `tezzera-theme` only.
**Status**: compiles, re-exported as `tezzera::anim`, but has zero real
consumers anywhere in the framework. See Known Issues #1.

---

### tezzera-animate  (Layer 5 — service, the ACTIVE animation system)
**Job**: Animation wired into the reactive-state/hook model —
`use_animation`/`use_spring` let widgets drive per-frame animated values
through `Context`. This is what actually backs Switch/Checkbox/Radio's
theme-global animation and every animated transition in the widget set.
**Exports**: `use_animation`, `AnimCtrl`, `Progress`, `frame_dt`/
`set_frame_dt`, `AnimationController`/`AnimationState`, `Easing`,
`Keyframe`/`KeyframeStop`, `Lerp`, `Spring`, `Tween`, `use_spring`,
`Animated`, `SpringController`/`SpringState`.
**Depends on**: `core`, `state`, `trace`.
**Consumed by**: `tezzera-widgets`, `tezzera-platform`, `tezzera-nav-anim`,
`tezzera-examples`, the umbrella `tezzera` crate.

---

### tezzera-scroll  (Layer 5 — service)
**Job**: Momentum-scroll physics — `ScrollController` with configurable
`ScrollPhysics` (momentum/friction), page snapping, scrollbar geometry.
**Exports**: `ScrollController`, `ScrollPhysics`, `clamp_offset`/
`snap_to_page`, `MomentumState`/`ScrollDirection`, `render_scrollbar`/
`ScrollbarMetrics`, `ScrollView` (a lower-level one — the widget most apps
use is `tezzera_widgets::ScrollView`, which builds on this).
**Depends on**: `core`, `state`, `layout`, `render`, `trace`.

---

### tezzera-nav  (Layer 5 — service)
**Job**: The navigation router — typed-enum `Route`s (never stringly-typed),
per-`Navigator` independent history stack, navigation guards, keep-alive
memory for navigated-away screens. Since D108/Phase 26 Step 3, also owns
the screen-transition spring physics that `ScreenNav` (the type real apps
actually use) drives directly — moved here from `tezzera-nav-anim` because
`tezzera-nav-anim` already depends on `tezzera-nav`, so the dependency
could not run the other way without a cycle.
**Exports**: `Navigator` (the real one — NOT in `tezzera-widgets`),
`Route`, `NavigationDecision`, `NavigationStack` (not `StackNavigator`),
`ScreenNav`, `NavigationGuard`/`AllowAllGuard`/`BlockWhenGuard`,
`HistoryEntry`, `KeepAliveRegistry`, `ScreenTransition`/`SlideDirection`/
`TransitionStyle` (`transition` module), `NavTransitionStyle`/
`TransitionStyleKind` (`screen_nav` module — platform-default resolution,
same shape as `tezzera-scroll::ScrollStyle`).
**Depends on**: `animate` (for `Spring`, transition physics), `core`,
`state`, `theme` (for the `ext`-map platform-default-override read),
`trace`.
**Note (D108/Phase 26 Step 3)**: `ScreenNav<R>` and `Navigator<R>`/
`NavigatorAnimated<R>` (`tezzera-nav-anim`) remain two separate,
non-overlapping implementations — `NavigatorAnimated` wraps `Navigator`,
not `ScreenNav`, and still has zero consumers outside its own tests. Only
`ScreenNav` is wired to anything real (`tzr new`'s generated apps). Flagged
as a known consolidation opportunity, not fixed — same spirit as
`tezzera_scroll::ScrollView`'s already-flagged duplication (Known Issue
above, D108 Phase 26 Step 2).

---

### tezzera-nav-anim  (Layer 5 — service, composes two other services)
**Job**: Animated screen transitions layered on `tezzera-nav::Navigator` —
slide/other transition styles driven per-frame. Still not wired to
anything real (see the note under `tezzera-nav` above) — `ScreenNav`
(what real apps use) drives its own copy of the same transition math
directly via `tezzera-nav::transition`, not through this crate.
**Exports**: `NavigatorAnimated`, `ScreenTransition`/`SlideDirection`/
`TransitionStyle` (re-exported from `tezzera-nav::transition`, which now
owns the actual implementation).
**Depends on**: `nav`, `trace`.

---

### tezzera-hot-reload  (Layer 5 — service)
**Job**: Dev-time hot reload — watches source directories for `.rs` changes
(debounced) and triggers rebuilds for a target platform. Groundwork for
D102/D103's fuller hot-reload architecture (PLANNED, not yet built).
**Exports**: `Debouncer`, `ChangeEvent`, `RebuildRunner`/`RebuildTarget`,
`FileWatcher`.
**Depends on**: `trace` only.

---

### tezzera-devtools  (Layer 5 — service)
**Job**: Developer tooling — trace viewer, atom-state inspector,
component/layout inspector, frame profiler, aggregated into `DevConsole`.
**Exports**: `DevConsole`, `AtomInspector`/`AtomEntry`/`AtomSnapshot`,
`ComponentInspector`/`LayoutNode`, `FrameProfiler`, `TraceViewer`.
**Depends on**: `core`, `state`, `trace`.
**Must NOT**: Ship in release builds without `#[cfg(debug_assertions)]`
gating.

---

### tezzera-forms  (Layer 5 — service)
**Job**: Form state and validation — `Form`/`FormField` with composable
`Validator`s.
**Exports**: `Form`, `FormField`, `FieldError`, `Validator` (trait) +
`Required`/`Email`/`MinLength`/`MaxLength`/`Range`/`Contains`.
**Depends on**: `state`, `trace`.

---

### tezzera-a11y  (Layer 5 — service)
**Job**: Accessibility semantic tree and focus management — `A11yTree`/
`A11yNode`/`Role` data model, `FocusManager` for keyboard/screen-reader
focus traversal. Platform AT-SPI/UIA bindings explicitly deferred (see
D107's web-SEO plan, which reuses this tree for a different purpose:
`RenderTree::collect_semantics()` in `tezzera-widgets` derives from
`tezzera_core::SemanticNode`, a parallel/simpler type — the two `Role`
enums are not yet unified; see Known Issues in `PHASE_25.md`).
**Exports**: `A11yTree`/`A11yNode`, `Role`, `FocusManager`, `FocusNode`.
**Depends on**: `core`, `state`.

---

### tezzera-gesture  (Layer 5 — service, see Known Issues #2)
**Job**: Gesture recognition — converts raw `InputEvent`s into higher-level
gestures (tap, double-tap, drag, swipe, pinch).
**Exports**: `GestureRecognizer` (trait), `TapRecognizer`, `DragRecognizer`/
`DragPhase`, `SwipeRecognizer`/`SwipeDirection`, `PinchRecognizer`,
`GestureEvent`.
**Depends on**: `platform` (for `InputEvent` — see Known Issues #2), `trace`.

---

### tezzera-net  (Layer 5 — service)
**Job**: Non-blocking network image loading via `std::thread`/`mpsc` — no
async runtime dependency.
**Exports**: `ImageLoader`, `RemoteImage`/`RemoteImageFit`, `LoadState`.
**Depends on**: `core`, `trace`.

---

### tezzera-text  (Layer 5 — service)
**Job**: Rich text layout — styled spans, paragraph word-wrapping, cursor/
selection, basic direction detection.
**Exports**: `RichText`, `TextSpan`/`TextStyle`, `TextLayout`/`TextLine`,
`word_wrap`/`word_wrap_simple`, `measure_text`, `TextCursor`/
`TextSelection`, `detect_direction`/`TextDirection`.
**Depends on**: `core`, `render`, `theme`.

---

### tezzera-shaping  (Layer 5 — service)
**Job**: Text shaping abstraction — `ShapingEngine` trait with a
`FallbackShaper` (fontdue-backed, one-glyph-per-char). A real HarfBuzz
shaper is explicitly deferred to v1.0 (D014).
**Exports**: `ShapingEngine` (trait), `FallbackShaper`, `GlyphRun`/
`ShapedGlyph`, `ShapingPipeline`, `Script`.
**Depends on**: `core`, `render`, `text`, `trace`.

---

### tezzera-bidi  (Layer 5 — service)
**Job**: A simplified subset of the Unicode Bidirectional Algorithm (UAX #9)
for mixed LTR/RTL (Latin + Arabic + Hebrew) text. Full ICU/`unicode-bidi`
integration deferred to v1.0.
**Exports**: `bidi_class`/`BidiClass`, `paragraph_level`/`resolve_levels`,
`BidiParagraph`, `reorder`/`reorder_line`.
**Depends on**: `trace` only.

---

### tezzera-i18n  (Layer 5 — service)
**Job**: Localization — `MessageBundle`/`Locale`, global-locale accessor,
`t()` translation lookup with graceful fallback to the key itself.
**Exports**: `MessageBundle`, `Locale`, `t`, `set_locale`/`current_locale`.
**Depends on**: `state`, `trace`.

---

### tezzera-clipboard  (Layer 5 — service)
**Job**: OS clipboard integration — shells out to `pbcopy`/`pbpaste` (macOS)
or `xclip`/`xsel` (Linux); `NoopClipboard` for tests/wasm.
**Exports**: `ClipboardProvider` (trait), `SystemClipboard`, `NoopClipboard`,
`ClipboardError`.
**Depends on**: `trace` only.

---

### tezzera-ws  (Layer 5 — service)
**Job**: WebSocket client with no async runtime and no `tungstenite` dep —
hand-rolled RFC 6455 handshake over `std::net::TcpStream`, non-blocking
`recv()` safe to poll every frame.
**Exports**: `WsClient`, `WsMessage`, `WsState`/`WsStream`, `WsError`.
**Depends on**: `trace` only.

---

### tezzera-ime  (Layer 5 — service)
**Job**: IME (CJK/complex-script input) data model — composition/preedit/
commit events. Real OS/winit IME wiring deferred to v1.0; only `NoopIme`
exists today.
**Exports**: `ImeHandler` (trait), `NoopIme`, `ImeComposition`, `ImeEvent`,
`ImeState`.
**Depends on**: `trace` only.

---

### tezzera-media  (Layer 5 — service)
**Job**: Audio/video data-model stubs — every operation currently returns
`MediaError::PlatformUnavailable` pending real rodio/cpal/platform decode in
v1.0.
**Exports**: `AudioPlayer`/`AudioHandle`, `VideoDecoder`/`VideoFrame`,
`AudioFormat`/`VideoFormat`, `MediaError`.
**Depends on**: `trace` only.

---

### tezzera-style  (Layer 5 — service, UNINTEGRATED, see Known Issues #4)
**Job as implemented**: CSS-like style system — stylesheets, rules,
selectors, computed/inline styles, decoupling appearance from structure.
**Exports**: `StyleSheet`/`StyleRule`/`Selector`, `ComputedStyle`,
`InlineStyle`/`InlineStyleBuilder`, `StyleProperty`/`StyleValue`,
`LengthUnit`.
**Depends on**: `theme`, `core`.
**Status**: not depended on by `tezzera-widgets` — nothing in the actual
widget rendering path uses it today. See Known Issues #4.

---

### tezzera-test-utils  (Layer 5-ish — see Known Issues #3)
**Job**: Test infrastructure for widget/render tests — `WidgetEnv` (headless
render canvas), `EventSim` (input simulation), `SnapshotAssert` (PNG pixel
comparison). Its own doc comment: "use it only in `[dev-dependencies]`."
**Exports**: `WidgetEnv`, `EventSim`, `SnapshotAssert`.
**Depends on**: `render`, `platform`, `core`.
**Must NOT** (per its own stated contract, currently VIOLATED — see Known
Issues #3): ship inside the production `tezzera` SDK crate as a regular
dependency.

---

## VIOLATION POLICY

If any crate violates its contract:
1. Do not merge
2. Redesign the boundary
3. Update this document if the contract needs adjusting
4. Never just add the import and move on

This document itself went eighteen crates and several renamed widgets stale
before anyone noticed — re-verify it against real code (`grep pub use`,
`Cargo.toml` deps) periodically rather than trusting it as permanently
accurate; the note at the top of this rewrite is a reminder, not a guarantee
this won't happen again.
