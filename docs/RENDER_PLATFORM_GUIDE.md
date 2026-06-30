# Render & Platform — How Tezzera Draws Pixels

This guide explains the two crates that sit at the bottom of the Tezzera stack:
`tezzera-render` (the pixel engine) and `tezzera-platform` (the window + event loop).
It also covers `tezzera-renderer`, the abstraction layer that was built but not yet
connected.

---

## The Big Picture

```
Your Component::build()
       ↓
   Element tree   ← pure data, no pixels yet
       ↓
  tezzera (umbrella)  ← walks the tree, calls layout() + paint()
       ↓
  tezzera-widgets      ← every widget's layout() and paint() live here
       ↓
  tezzera-render       ← SkiaCanvas: the actual pixel buffer
       ↓
  tezzera-platform     ← winit window + softbuffer display
       ↓
     Screen
```

Each crate does exactly one job. No crate below can import from a crate above.

---

## `tezzera-render` — The Pixel Engine

**Crate path:** `tezzera-render/src/`

This crate owns the CPU pixel buffer and all drawing primitives. Nothing here knows
about widgets, components, or the element tree. It just draws shapes and text into
a grid of RGBA bytes.

### Files at a glance

```
tezzera-render/src/
├── canvas.rs          ← SkiaCanvas — the pixel buffer + draw calls
├── font.rs            ← FontCache — loads TTF, rasterizes glyphs
├── image.rs           ← ImageHandle — decoded PNG pixels
├── layer.rs           ← Layer + LayerCompositor — the 6-layer stack
├── dirty_region.rs    ← DirtyRegionTracker — which rects need repainting
├── render_pipeline.rs ← RenderPipeline — orchestrates a full frame
└── lib.rs             ← re-exports everything above
```

### `SkiaCanvas` — `canvas.rs`

Think of `SkiaCanvas` as a sheet of pixel-graph paper you can draw on.
Internally it holds a `tiny_skia::Pixmap` — a contiguous block of `width × height × 4`
bytes in RGBA order.

```
SkiaCanvas {
    pixmap: Pixmap   // [R G B A  R G B A  R G B A ...]
                     //  ← width × height pixels →
}
```

**Drawing calls:**

| Method | What it does |
|--------|-------------|
| `clear(color)` | Fill every pixel with this color (fastest — one memset) |
| `fill_rect(rect, color)` | Paint a solid rectangle |
| `stroke_rect(rect, color, width)` | Draw a rectangle outline |
| `fill_circle(center, radius, color)` | Paint a filled circle |
| `draw_text(text, origin, color, font, px)` | Rasterize text glyphs onto the canvas |
| `pixels() → &[u8]` | Read raw RGBA bytes (used to copy to screen) |
| `encode_png() → Vec<u8>` | Encode the canvas as a PNG (used for golden tests) |

**Coordinate system:** origin `(0, 0)` is top-left. X grows right, Y grows down.
All coordinates are logical pixels (f32).

**Why tiny-skia?** It is pure Rust, requires no system libraries, and compiles to WASM.
When Tezzera matures it will be swapped for `skia-safe` (C++ Skia, GPU acceleration).
The plan is to abstract this behind a `Canvas` trait so the swap is zero widget changes.

### `FontCache` — `font.rs`

Loads a TrueType font file and rasterizes individual characters on demand using the
`fontdue` crate. Results are cached so each `(char, pixel_size)` combination is
rasterized at most once.

```
FontCache {
    font: fontdue::Font,
    glyph_cache: HashMap<(char, px_bits), (Metrics, Vec<u8>)>,
    metrics_cache: HashMap<(char, px_bits), f32>,  // advance widths
}
```

**Key methods:**

| Method | Returns | Notes |
|--------|---------|-------|
| `system_ui()` | `Option<FontCache>` | Tries Avenir Next → Helvetica → Arial → Linux/Win fallbacks |
| `system_mono()` | `Option<FontCache>` | Tries Menlo → Monaco → Courier → DejaVu |
| `rasterize(c, px)` | `(Metrics, Vec<u8>)` | Coverage bitmap for one character |
| `advance_width(c, px)` | `f32` | Horizontal advance after drawing this character |
| `measure_text(text, px)` | `f32` | Total pixel width of a string |
| `ascender(px)` | `i32` | Distance from top of line box to baseline |
| `line_height(px)` | `f32` | Full line height including descender gap |

**How glyph rasterization works:**
`fontdue` traces the TrueType bezier curves at the requested pixel size and produces
a coverage bitmap — each byte is a value 0–255 representing how much of that pixel
is covered by the glyph. `draw_text` uses these coverage values to alpha-blend each
pixel of each character onto the canvas.

### The 6-Layer Stack — `layer.rs`

Tezzera uses 6 composited layers, one for each visual concern. Higher layers sit
on top of lower ones, just like z-index in CSS.

```
Layer 5 — DEV_TOOLS      ← inspector overlay (debug builds only)
Layer 4 — OVERLAYS       ← tooltips, snackbars, toasts
Layer 3 — MODALS         ← dialogs, bottom sheets
Layer 2 — MODAL_BARRIER  ← semi-transparent scrim behind modals
Layer 1 — NAVIGATION     ← app bar, bottom nav, tab bar
Layer 0 — CONTENT        ← main widget tree (scrollable content)
```

Each layer is its own `SkiaCanvas` (its own pixel buffer). The `LayerCompositor`
holds all six and blends them together when building the final frame.

```
LayerCompositor {
    layers: Vec<Layer>,   // always 6 entries
    width: u32,
    height: u32,
}

Layer {
    canvas: SkiaCanvas,
    dirty: bool,          // only repaint if true
}
```

**Current status:** `composite()` is a no-op (Phase 1 placeholder). The plan is to
implement SourceOver pixel blending so all six layers are visible.

### `DirtyRegionTracker` — `dirty_region.rs`

Tracks which parts of the screen need repainting. Avoids repainting unchanged pixels.

```
DirtyRegionTracker {
    dirty_rects: Vec<Rect>,   // specific changed regions
    full_repaint: bool,       // true = repaint everything
}
```

It starts in `full_repaint=true` so the very first frame always paints completely.
After each frame, `clear()` resets it. If an atom changes, the region it covers can
be marked dirty without touching the whole screen.

### `RenderPipeline` — `render_pipeline.rs`

Orchestrates a complete frame: check dirty → paint → composite → return pixels.

```
RenderPipeline {
    compositor: LayerCompositor,
    dirty: DirtyRegionTracker,
    frame_counter: u64,
    frame_budget_ms: f64,   // 16.667 ms for 60 fps
}
```

**How a frame works:**
```
render_frame(paint_fn):
    1. Check if content layer is dirty
    2. If dirty: call paint_fn(canvas) to repaint it, mark clean
    3. Composite all 6 layers into an output canvas
    4. Log if frame took > 16.667 ms (dropped frame)
    5. Return pixel data
```

**Current status:** The pipeline exists but `PlatformWindow` bypasses it entirely —
it drives `SkiaCanvas` directly. The plan is to wire `PlatformWindow` through
`RenderPipeline` so dirty tracking and layer compositing are active.

---

## `tezzera-renderer` — The Abstraction Layer

**Crate path:** `tezzera-renderer/src/`

This crate defines the `Renderer` trait — a backend-agnostic drawing API. The idea
is that widgets call `renderer.fill_rect(...)` without caring whether the backend
is tiny-skia (CPU) or skia-safe (GPU).

```
tezzera-renderer/src/
├── renderer.rs   ← Renderer trait (the swap point)
├── skia.rs       ← SkiaRenderer: implements Renderer using SkiaCanvas
├── backend.rs    ← RendererBackend enum (TinySkia | SkiaSafe)
└── lib.rs        ← re-exports
```

**The `Renderer` trait:**
```rust
pub trait Renderer {
    fn clear(&mut self, color: Color);
    fn fill_rect(&mut self, rect: Rect, color: Color);
    fn stroke_rect(&mut self, rect: Rect, color: Color, width: f32);
    fn fill_circle(&mut self, center: Point, radius: f32, color: Color);
    fn draw_text(&mut self, text: &str, pos: Point, color: Color, font: &FontCache, size: f32);
    fn encode_png(&self) -> Vec<u8>;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
}
```

**`SkiaRenderer`** wraps a `SkiaCanvas` and delegates every call to it.

**Current problem:** This abstraction exists but `PaintCtx` (what every widget
receives during paint) holds `&mut SkiaCanvas` directly — not `&mut dyn Renderer`.
So the abstraction is defined but bypassed.

**The plan:** Merge `tezzera-renderer` into `tezzera-render`, rename `Renderer` to
`Canvas` (clearer name for a drawing surface), and change `PaintCtx` to hold
`&mut dyn Canvas`. That one change makes swapping tiny-skia for skia-safe automatic.

---

## `tezzera-platform` — The Window and Event Loop

**Crate path:** `tezzera-platform/src/`

This crate creates an OS window, handles input events, and drives the render loop.
It uses two external libraries:
- **`winit`** — cross-platform window creation and event dispatching
- **`softbuffer`** — copies pixel bytes from a `SkiaCanvas` to the screen framebuffer

```
tezzera-platform/src/
├── app.rs    ← PlatformWindow — the window + event loop
├── event.rs  ← InputEvent, MouseButton, Key enums
├── lib.rs    ← re-exports
└── web.rs    ← web/WASM stub (future)
```

### `PlatformWindow` — `app.rs`

The low-level host. Takes a paint closure and runs forever, calling it on every
`RedrawRequested` event.

```rust
PlatformWindow::new()
    .title("My App")
    .size(800, 600)
    .run(|canvas: &mut SkiaCanvas, events: &[InputEvent]| {
        // draw everything here
        // events contains mouse/keyboard input since last frame
    });
```

**Inside the event loop (`AppState`):**

```
AppState {
    window: Arc<WinitWindow>,
    surface: softbuffer::Surface,   // the screen framebuffer
    canvas: SkiaCanvas,             // reused across frames
    pending_events: Vec<InputEvent>,
    cursor_x, cursor_y: f32,
    frame_counter: u64,
}
```

**Frame lifecycle (`RedrawRequested`):**
```
1. Get current window size
2. Resize softbuffer surface to match
3. Reallocate SkiaCanvas if window size changed
4. Drain pending_events
5. Call paint_fn(canvas, events) — this is where widgets draw
6. Copy canvas RGBA → softbuffer's u32 pixel format:
      pixel = (R << 16) | (G << 8) | B    ← alpha is dropped
7. Present the softbuffer to the screen
```

**Input events collected:**

| Event | When |
|-------|------|
| `MouseMove { x, y }` | Cursor moved |
| `MouseDown { x, y, button }` | Button pressed |
| `MouseUp { x, y, button }` | Button released |
| `KeyDown { key }` | Key pressed |
| `KeyUp { key }` | Key released |
| `Text { character }` | Printable character typed |
| `WindowResized { width, height }` | Window resized |

**Current problems:**
1. `ControlFlow::Poll` — spins at full CPU speed even when nothing changes
2. `about_to_wait()` unconditionally calls `request_redraw()` every loop iteration
3. `RenderPipeline` is bypassed — canvas is driven directly
4. No way for `Atom::set()` to wake the loop (`EventLoopProxy` not set up)

### The RGBA → softbuffer conversion

softbuffer expects pixels as `u32` in `0x00RRGGBB` format (alpha unused).
The conversion in `app.rs:159-163`:
```rust
let r = pixels[i * 4];
let g = pixels[i * 4 + 1];
let b = pixels[i * 4 + 2];
// pixels[i * 4 + 3] is alpha — dropped (softbuffer doesn't use it)
*pixel = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
```

This is correct for opaque rendering (all our widgets are opaque on the background).
Transparent overlays would need alpha compositing before this step.

### `InputEvent` and `Key` — `event.rs`

```rust
pub enum InputEvent {
    MouseMove    { x: f32, y: f32 },
    MouseDown    { x: f32, y: f32, button: MouseButton },
    MouseUp      { x: f32, y: f32, button: MouseButton },
    KeyDown      { key: Key },
    KeyUp        { key: Key },
    Text         { character: char },
    WindowResized{ width: u32, height: u32 },
}

pub enum MouseButton { Left, Right, Middle }

pub enum Key {
    Char(char), Enter, Escape, Space, Backspace,
    Tab, ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
}
```

---

## How the Two Crates Connect

```
tezzera-platform                     tezzera-render
─────────────────                    ─────────────────────
PlatformWindow::run(paint_fn)
  │
  │  on RedrawRequested:
  │    paint_fn(&mut SkiaCanvas, &[InputEvent])
  │                 ↑
  └─────────────────┘ SkiaCanvas lives in AppState
                       tezzera-platform imports tezzera-render
```

Right now the connection is direct: platform holds a `SkiaCanvas` and passes a mutable
reference to the paint closure. The `RenderPipeline` in `tezzera-render` is never used.

**What the plan changes:**
```
PlatformWindow::run(paint_fn)
  │
  │  on RedrawRequested:
  │    pipeline.render_frame(|canvas| {
  │        paint_fn(canvas, &events)
  │    })
  │
  │    copy pipeline.pixels() → softbuffer
  │
  └─ pipeline: RenderPipeline  ← tracks dirty, composites 6 layers
```

---

## Key Numbers

| Constant | Value | Where |
|----------|-------|-------|
| Frame budget (60 fps) | 16.667 ms | `render_pipeline.rs` |
| Layer count | 6 | `layer.rs::layer_index::COUNT` |
| Default font size | 16 px | `tezzera/src/lib.rs` (walk_element Text) |
| Default window size | 800 × 600 | `platform/app.rs` |

---

## Swap Path: tiny-skia → skia-safe

Once the `Canvas` trait is in place, the swap is:

1. Add a new dependency on `skia-safe` in `tezzera-render/Cargo.toml`
2. Create `tezzera-render/src/skia_safe_canvas.rs`:
   ```rust
   pub struct SkiaSafeCanvas { surface: skia_safe::Surface }
   impl Canvas for SkiaSafeCanvas { ... }
   ```
3. Change `App::launch()` to instantiate `SkiaSafeCanvas` instead of `TinySkiaCanvas`
4. Zero widget changes required

The `RendererBackend` enum in `backend.rs` (`TinySkia` | `SkiaSafe`) already
documents this intent — it just needs the implementation.
