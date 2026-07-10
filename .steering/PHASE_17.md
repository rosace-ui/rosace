# Phase 17 — TransformLayer: GPU Scroll Without CPU Re-render

> Status: COMPLETE
> Started: 2026-07-01
> Completed: 2026-07-01

## Why This Phase

Phase 16 introduced two-layer GPU compositing. Phase 17 adds GPU-side transforms
so a cached layer can be translated (scrolled) without the CPU re-recording or
re-uploading the layer texture.

Current scroll situation: `ScrollView` clips content and adjusts draw coordinates
each frame via CPU. Every pixel of the scrolled content must be re-rasterized
even if none of the content changed — only the scroll offset changed.

Phase 17 makes scroll zero-repaint:
1. The `TransformLayer` widget captures its child into a CPU Picture
2. The Picture is rasterized once into a separate SkiaCanvas at its natural size
3. The canvas is uploaded to a GPU texture
4. A WGSL uniform buffer carries the translation offset
5. On scroll: only the uniform changes — no CPU re-render, no texture re-upload
6. The GPU clips the visible region from the large texture using a scissor rect

---

## Decisions

- **D080** — TransformLayer model: `TransformLayer<W>` captures child into a
  Picture/texture sized to child natural size (possibly larger than the window).
  The compositor receives a `TransformLayer { texture, offset: Vec2, clip: Rect }`.
  The vertex shader applies `clip_position.xy += offset / viewport_size`. No
  new sampler — the same nearest-neighbour sampler is reused.

- **D081** — Transform uniform buffer: A wgpu `Buffer` with usage
  `UNIFORM | COPY_DST`. Each frame the offset is written via `queue.write_buffer`.
  The fragment shader clips UV coordinates to `[0, 1]` — out-of-range = transparent.
  This is simpler than a scissor rect and avoids pipeline state changes.

- **D082** — TransformLayer size limit: Phase 17 caps captured content at
  4096×4096 physical pixels. Larger content falls back to CPU clip scroll.
  The cap is checked at capture time; a debug warning is emitted if hit.

- **D083** — ScrollView integration: `ScrollView` detects if its content fits
  in a `TransformLayer` (content height ≤ 4096 physical pixels) and uses it
  when available. The scroll offset atom drives the GPU uniform write.

---

## Steps

### Step 1 — CompositorLayer gains optional transform

Extend `CompositorLayer` with `offset: (f32, f32)` (default `(0.0, 0.0)`).

In the compositor WGSL shader:
```wgsl
struct Uniforms {
    offset: vec2<f32>,
};
@group(0) @binding(2) var<uniform> uniforms: Uniforms;
```

UV in the fragment shader is shifted: `uv + uniforms.offset`. Out-of-range UV
clamps to the edge (via `ClampToEdge` sampler) — so content stops at boundaries.

Actually, for scroll we want transparent outside the content bounds, not clamped.
Use `uv - offset / texture_size` where values outside [0,1] return transparent.

### Step 2 — rosace-compositor: transform pipeline

Add a third pipeline variant: `pipeline_transform` with a different fragment
shader that samples with offset + transparency for out-of-range UV.

Or: update the single fragment shader to always read an offset uniform. The
default (0, 0) makes it behave identically to Phase 16 for base/overlay layers.

### Step 3 — TransformCanvas in AppState

`AppState` gains a `transform_canvases: Vec<SkiaCanvas>` (one per active
transform layer). Each `TransformCanvas` is sized to the child's natural size.

The `run_layered` closure signature becomes:
```rust
FnMut(&mut SkiaCanvas, &mut SkiaCanvas, &mut Vec<TransformLayerData>, &[InputEvent])
```
Where `TransformLayerData = { canvas: &mut SkiaCanvas, offset: (f32, f32) }`.

### Step 4 — ScrollView uses TransformLayer

`ScrollView` in its `paint()` checks if content fits in the transform budget:
- If yes: record content into the transform canvas, return TransformLayerData
- If no: current CPU clip path (unchanged)

### Step 5 — phase17_demo

Long scrollable list (50 items) inside a ScrollView with TransformLayer.
Demonstrate that scrolling is smooth: no CPU paint calls visible per scroll step.

---

## Exit Criteria

```
□ CompositorLayer carries optional (offset_x, offset_y)
□ WGSL shader reads offset uniform; out-of-bound UV → transparent
□ TransformLayer captures child into separate GPU texture
□ On scroll offset change: only queue.write_buffer called, no texture re-upload
□ ScrollView uses TransformLayer when content height ≤ 4096px
□ phase17_demo scrolls smoothly without CPU re-render
□ Workspace tests pass
□ D080–D083 in DECISIONS.md
```

---

## Approved dependencies (new)
- None

## DO NOT
- DO NOT add texture atlas yet (Phase 18)
- DO NOT support > 1 TransformLayer per frame (Phase 18)
- DO NOT add scale/rotation transforms (Phase 18)
- DO NOT port ScrollView to TransformLayer on all paths (fallback stays for > 4096px)
