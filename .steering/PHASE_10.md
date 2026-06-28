# Phase 10 — Animation, Accessibility, Test Harness, Package CLI

> Status: IN PROGRESS
> Target: v1.0 completeness — animation system, a11y tree, test utilities, packaging

## Steps

### Step 1 — Animation system (`tezzera-anim`)
- `Easing` enum: `Linear`, `EaseIn`, `EaseOut`, `EaseInOut`, `CubicBezier(f32,f32,f32,f32)`, `Spring { stiffness, damping }`
- `easing_fn(easing: Easing, t: f32) -> f32` — maps normalized t (0.0–1.0) to progress
- `Tween<T>` where `T: Lerp` — `from: T`, `to: T`, `duration_secs: f32`, `easing: Easing`
- `Lerp` trait: `lerp(a: &Self, b: &Self, t: f32) -> Self` — impl for f32, f64, Color (tezzera_theme::Color)
- `AnimationState` enum: `Idle`, `Running { elapsed: f32 }`, `Finished`
- `AnimationController<T>` — drives a `Tween<T>`, `tick(dt: f32) -> T`, `reset()`, `reverse()`
- `Keyframe<T> { time: f32, value: T, easing: Easing }`
- `Timeline<T>` — `Vec<Keyframe<T>>`, `sample(time: f32) -> T` (linear search, lerp between adjacent keyframes)
- No winit/event-loop dependency — pure math, driven by external dt

### Step 2 — Accessibility tree (`tezzera-a11y`)
- `Role` enum: `Button`, `Checkbox`, `Link`, `Heading`, `Image`, `Text`, `TextInput`, `List`, `ListItem`, `Dialog`, `None`
- `A11yNode { id: u64, role: Role, label: Option<String>, description: Option<String>, children: Vec<u64>, focusable: bool, checked: Option<bool> }`
- `A11yTree` — `HashMap<u64, A11yNode>`, `root: u64`
  - `add_node(node: A11yNode)`
  - `remove_node(id: u64)`
  - `find_by_role(role: Role) -> Vec<&A11yNode>`
  - `find_by_label(label: &str) -> Option<&A11yNode>`
  - `children_of(id: u64) -> Vec<&A11yNode>`
- `FocusManager` — `focused: Option<u64>`, `focus_order: Vec<u64>`
  - `focus_next() -> Option<u64>`
  - `focus_prev() -> Option<u64>`
  - `set_focus(id: u64)`
  - `tab_order(&A11yTree) -> Vec<u64>` — BFS traversal of focusable nodes

### Step 3 — Test harness (`tezzera-test-utils`)
- `WidgetEnv` — lightweight test environment holding a `SkiaCanvas` and `FontCache`
  - `new(width: u32, height: u32) -> Self`
  - `render_text(text: &str, x: f32, y: f32, size: f32, color: Color)`
  - `encode_png() -> Vec<u8>`
  - `pixel_at(x: u32, y: u32) -> Color` — reads raw pixel from canvas buffer
- `EventSim` — simulates `InputEvent` sequences without a real window
  - `tap(x: f32, y: f32) -> Vec<InputEvent>` — generates MouseDown + MouseUp
  - `type_text(s: &str) -> Vec<InputEvent>` — generates KeyDown + KeyUp per char
  - `scroll(x: f32, y: f32, delta: f32) -> Vec<InputEvent>`
- `SnapshotAssert` — simple image comparison
  - `save_snapshot(name: &str, png: &[u8])` — writes to `test_snapshots/<name>.png`
  - `assert_snapshot(name: &str, png: &[u8])` — compares with saved snapshot pixel-by-pixel; panics on diff > threshold
  - `pixel_diff_count(a: &[u8], b: &[u8]) -> usize` — counts differing pixels
- `[dev-dependencies]` only — this crate is test infrastructure, not production

### Step 4 — `tzr package` subcommand
- Add `tzr package` to `tezzera-cli`
- `PackageConfig { name: String, version: String, target: String, output_dir: String }`
- `PackageManifest { crates: Vec<String>, examples: Vec<String>, built_at: String }`
- `run_package(config: &PackageConfig) -> CommandResult`
  - runs `cargo build --release --workspace`
  - collects binary paths
  - writes `PackageManifest` as JSON to `<output_dir>/manifest.json`
- Wire into `main.rs` match arm `"package"`

### Step 5 — Phase 10 showcase
- `tezzera-examples/src/bin/phase10_demo.rs`
- 1400×900 PNG, 4 panels:
  1. Animation — `Easing` curve chart (6 easing types as SVG-like curve in canvas), `Tween<f32>` sampled at 10 steps, `Timeline` keyframe display
  2. Accessibility — `A11yTree` node graph (role chips + parent arrows), `FocusManager` tab order list
  3. Test harness — `WidgetEnv` diagram, `EventSim` event sequence, `SnapshotAssert` flow
  4. Package CLI — `tzr package` flow diagram, `PackageManifest` JSON display

## Exit Criteria

- [ ] `Tween::<f32>::new(0.0, 100.0, 1.0, Easing::EaseInOut).tick(0.5)` returns ~50.0
- [ ] `Timeline::sample(0.0)` returns first keyframe value
- [ ] `A11yTree::find_by_role(Role::Button)` returns all button nodes
- [ ] `FocusManager::focus_next()` cycles through focusable nodes
- [ ] `WidgetEnv::pixel_at(x, y)` reads correct color after render_text
- [ ] `EventSim::tap(x, y)` returns exactly 2 events (MouseDown + MouseUp)
- [ ] `tzr package` subcommand registered in CLI
- [ ] All workspace tests pass, zero warnings

## Approved dependencies

- No winit in tezzera-anim — pure dt-based math
- No AT-SPI/UIA bindings — a11y is data model only, platform integration is v1.0
- No image-diff crate — hand-roll pixel comparison
- No serde for PackageManifest serialization — write JSON manually

## DO NOT

- DO NOT implement spring physics beyond simple approximation
- DO NOT implement real platform accessibility APIs — data model only
- DO NOT add golden-file update mode — just save/compare
- DO NOT add cargo-publish — package only bundles binaries
