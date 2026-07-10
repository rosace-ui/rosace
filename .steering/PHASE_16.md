# Phase 16 — Multi-Layer GPU Compositing

> Status: COMPLETE
> Started: 2026-07-01
> Completed: 2026-07-01

## Why This Phase

Phase 15 blits a single merged pixel buffer via GPU. Phase 16 separates the
rendering pipeline into independent GPU layers:

1. **Base layer** — the main widget tree (rendered by tiny-skia into canvas A)
2. **Overlay layer** — dialogs, tooltips, dropdowns (rendered into canvas B)

Each layer is uploaded as its own GPU texture. The compositor blits and
alpha-blends them on the GPU without any CPU compositing. Future phases add:
- Scroll transform (translate layer on GPU — zero CPU re-render for scroll)
- Animated opacity (fade layer in/out on GPU — no CPU paint needed)

---

## Decisions

- **D076** — Layer compositing model: Each logical layer (base, overlay, future:
  scroll regions) is a separate `SkiaCanvas`. Each canvas produces its own pixel
  buffer. The compositor uploads N textures and blends bottom-to-top using
  `SRC_ALPHA` over `ONE_MINUS_SRC_ALPHA`.

- **D077** — CompositorLayer struct: `{ pixels: &[u8], width: u32, height: u32,
  opacity: f32 }`. `GpuPresenter::present_layers(&[CompositorLayer])` replaces
  the single-buffer `present()` method. The old `present()` is kept as a shim
  calling `present_layers` with a single opaque layer.

- **D078** — Overlay canvas: The overlay SkiaCanvas is cleared to transparent
  before each frame. Transparent pixels in the overlay texture are discarded by
  the blend equation — the base shows through where the overlay is transparent.

- **D079** — WGSL shader update: The multi-layer shader composites layers in a
  single pass using a loop or unrolled blending. For Phase 16 the loop is
  unrolled for 2 layers (base + overlay). The fragment output is:
  `base_color * (1 - overlay.a) + overlay_color * overlay.a`.

---

## Steps

### Step 1 — SkiaCanvas: transparent-clear + second canvas

Add `SkiaCanvas::clear_transparent(&mut self)` — fills pixmap with RGBA(0,0,0,0).
Used to clear the overlay canvas each frame before overlay rendering.

### Step 2 — GpuPresenter: multi-layer pipeline

`CompositorLayer` struct:
```rust
pub struct CompositorLayer<'a> {
    pub pixels:   &'a [u8],
    pub width:    u32,
    pub height:   u32,
    pub opacity:  f32,
}
```

`present_layers(&mut self, layers: &[CompositorLayer])`:
- Creates one wgpu texture per layer
- All textures bound in a single bind group (or sequential passes)
- For Phase 16: two-pass approach — blit base, then alpha-blend overlay on top
  using `SrcAlpha / OneMinusSrcAlpha` blend mode (already in the pipeline config)

Actually: the simplest correct approach for two-layer compositing is two render
passes, each writing to the same surface target. Pass 1 draws the base texture
(opaque). Pass 2 draws the overlay texture with alpha blending enabled.

### Step 3 — Render loop update

`rosace/src/lib.rs`:
- Maintain `overlay_canvas: SkiaCanvas` alongside `canvas`
- Before overlay pass: `overlay_canvas.clear_transparent()`
- Play overlay Pictures into `overlay_canvas` instead of main `canvas`
- Call `presenter.present_layers(&[base_layer, overlay_layer])`

### Step 4 — phase16_demo

Demo with a dialog overlay that fades in (opacity controlled by atom). The base
layer stays static; only the overlay layer changes its alpha. No CPU re-render
of the base layer needed for the fade — only the GPU blend factor changes.
(Note: opacity change still triggers a frame in Phase 16; the "no CPU re-render"
optimization — changing only a uniform — is Phase 17.)

---

## Exit Criteria

```
□ Overlay renders in a separate SkiaCanvas → separate GPU texture
□ GpuPresenter.present_layers() composites base + overlay correctly
□ Transparent overlay pixels show the base layer through
□ Opaque overlay pixels fully cover the base
□ phase16_demo shows overlay dialog rendered on top of base content
□ All workspace tests pass
□ D076–D079 logged in DECISIONS.md
```

---

## Approved dependencies (new)
- None

## DO NOT
- DO NOT add scroll TransformLayer yet (Phase 17)
- DO NOT change opacity per-frame via GPU uniform (Phase 17)
- DO NOT add texture atlas (Phase 17)
- DO NOT add more than 2 hardcoded layers (Phase 17 adds dynamic N layers)
