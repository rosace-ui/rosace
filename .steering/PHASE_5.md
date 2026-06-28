# Phase 5 — Image Rendering, Overlays, Transitions & Templates

> Status: IN PROGRESS
> Started: 2026-06-28
> Target: Complete UI toolkit — images, overlays, animated transitions, better DX

## Steps

### Step 1 — Image widget (D033)
- `ImageWidget` in `tezzera-widgets/src/image.rs`
- Sources: file path (loads via `std::fs::read` + `tiny_skia::Pixmap::decode_png`), raw `&[u8]` bytes
- `ImageFit` enum: Fill (stretch), Contain (letterbox), Cover (crop center), None (natural size)
- Fallback: colored placeholder rect when image fails to load
- `ImageCache` struct: caches decoded `Pixmap` by path to avoid re-decoding every frame

### Step 2 — Modal, Dialog, Toast overlays
- `Modal` in `tezzera-widgets/src/overlay.rs`
  - Full-canvas dim overlay (`rgba(0,0,0,160)`) drawn first
  - Centered content box in `surface` color with border
  - `dismiss_on_backdrop: bool`
- `Dialog` wraps `Modal` — title string + message string + up to 3 action buttons
- `Toast` — bottom-center notification, `lifetime: f32` (seconds), auto-dismiss when elapsed
  - `ToastQueue`: holds up to 5 active toasts, each with a countdown timer
  - `tick(dt: f32)` removes expired toasts

### Step 3 — Screen transition animations
- `tezzera-nav-anim/` new crate
- `TransitionStyle` enum: Slide { direction: SlideDirection }, Fade, Scale, None
- `SlideDirection`: Left, Right, Up, Down
- `ScreenTransition` struct: wraps two `Spring` instances (enter and exit offsets)
  - `trigger(style, direction)` — starts both springs
  - `update(dt)` — steps springs, returns `(enter_offset, exit_offset, is_complete)`
- `NavigatorAnimated<R>` — wraps `tezzera_nav::Navigator<R>` with a `ScreenTransition`
  - `push_animated(route, style)`, `pop_animated(style)`
  - `current_transform()` → `(f32, f32)` offset to apply when drawing current screen
  - `previous_transform()` → `(f32, f32)` offset for the outgoing screen

### Step 4 — `tzr new --template` project templates
- Extend `tezzera-cli/src/commands/new.rs` with `--template <name>` flag
- Templates: `counter` (default, existing), `nav-app` (3-screen navigation), `form-app` (login form with validation), `dashboard` (stats cards + charts placeholder)
- Each template generates a complete, runnable `src/main.rs`
- `tzr new my_app --template nav-app` scaffolds a Navigator-based app

### Step 5 — Accessibility stubs (D035)
- `tezzera-a11y/` new crate
- `Role` enum: Button, TextInput, Checkbox, Image, Heading, Label, None
- `A11yNode { role, label, description, focusable, focused }` 
- `A11yTree` — flat list of nodes, built alongside the widget tree
- `FocusManager` — tracks focused node index, `focus_next()`, `focus_prev()`, `focused_id()`
- Output: `A11yTree::to_aria_json()` → JSON string for web bridge (WASM only)
- Phase 5 is stubs only — no screen reader integration yet

### Step 6 — Phase 5 showcase PNG
- `tezzera-examples/src/bin/phase5_demo.rs`
- 1400×900 PNG, 4 panels:
  1. Image Panel — `ImageWidget` with Contain/Cover/Fill modes on a sample image
  2. Overlay Panel — `Modal` + `Dialog` + `Toast` rendered on dark bg
  3. Transitions Panel — 6 boxes showing slide/fade/scale transition states
  4. Templates Panel — screenshots of the 4 `tzr new` templates side by side

## Exit Criteria

- [ ] `ImageWidget` renders a PNG file at correct size with Contain fit
- [ ] `ImageCache` prevents re-decoding the same file path twice
- [ ] `Modal` renders dim overlay + centered box
- [ ] `Dialog` shows title, message, and buttons
- [ ] `ToastQueue` auto-dismisses toasts after `lifetime` frames
- [ ] `ScreenTransition` spring reaches settled state within 1 second at 60fps
- [ ] `tzr new --template nav-app` generates a 3-screen Navigator app
- [ ] `A11yNode` and `FocusManager` have test coverage
- [ ] Phase 5 demo PNG saved to `phase5_demo.png`
- [ ] Full workspace tests pass with zero warnings

## Approved dependencies

- `tiny_skia` (already present) — use `Pixmap::decode_png()` for image loading
- `tezzera-animate` (already present) — `Spring` for transition physics
- `tezzera-nav` (already present) — wrapped by `NavigatorAnimated`
- No serde/JSON deps for a11y — hand-write the JSON string for now

## DO NOT

- DO NOT integrate with OS screen readers — stubs only in Phase 5
- DO NOT add video/GIF support — static images only
- DO NOT add gesture-driven transitions (swipe to go back) — Phase 6
- DO NOT add drag-and-drop — Phase 6
- DO NOT add network image loading (URL fetch) — Phase 6
