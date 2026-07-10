# Phase 15 — wgpu GPU Compositor

> Status: COMPLETE
> Started: 2026-07-01
> Completed: 2026-07-01

## Why This Phase

Phase 14 completed the CPU rendering pipeline: tiny-skia draws to a pixel
buffer, softbuffer copies it to the screen. Phase 15 replaces softbuffer with
wgpu so that:

- The CPU pixel buffer is uploaded to a GPU texture once per frame
- A fullscreen-quad shader blits the texture to the wgpu surface
- Future phases can composite multiple Picture layers on the GPU without a full
  CPU readback (Layer compositing, transforms, opacity — without re-running
  tiny-skia for unchanged layers)
- 120 fps is possible because GPU blit is ~0.1 ms vs a large memcpy in software

The swap is isolated to `rosace-compositor` + `rosace-platform`. Everything
above (widget tree, state, layout, paint) is unchanged.

---

## Decisions to Record

- **D072** — GPU backend choice: wgpu (not raw vulkan/metal/dx12). wgpu picks
  the best native backend per OS (Metal on macOS, DX12/Vulkan on Windows/Linux)
  and has a pure-Rust API. No C++ toolchain needed (D032: tiny-skia stays as
  CPU rasterizer; wgpu is the display backend, not the drawing backend).
  
- **D073** — Pixel format: RGBA8 for the CPU→GPU upload texture. tiny-skia
  produces RGBA8 (or BGRA8 on some platforms). The wgpu texture format is
  `Bgra8UnormSrgb` on most backends; we detect and adapt in the compositor.

- **D074** — Compositor architecture: `rosace-compositor` is a standalone
  crate. `rosace-platform` gains an optional `GpuPresenter` (struct that wraps
  the wgpu device/queue/surface/pipeline). Platform falls back to softbuffer if
  wgpu init fails. Feature flag `"gpu"` on `rosace-platform` activates it.

- **D075** — Shader: a minimal WGSL fullscreen-quad shader. No vertex buffer;
  vertex shader generates the quad from `vertex_index`. Fragment shader samples
  the uploaded texture with nearest-neighbour sampling (pixels are already at
  physical resolution from HiDPI canvas).

---

## Steps

### Step 1 — rosace-compositor crate

**Cargo.toml**:
```toml
[package]
name = "rosace-compositor"
version = "0.1.0"

[dependencies]
wgpu = "24"
raw-window-handle = "0.6"
```

**src/lib.rs** — `GpuPresenter`:
```rust
pub struct GpuPresenter {
    surface:   wgpu::Surface<'static>,
    device:    wgpu::Device,
    queue:     wgpu::Queue,
    config:    wgpu::SurfaceConfiguration,
    pipeline:  wgpu::RenderPipeline,
    sampler:   wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    width:     u32,
    height:    u32,
}

impl GpuPresenter {
    pub async fn new_async(window: Arc<WinitWindow>, width: u32, height: u32) -> Self { ... }
    pub fn new(window: Arc<WinitWindow>, width: u32, height: u32) -> Option<Self> {
        pollster::block_on(Self::new_async(...)).ok()
    }
    pub fn resize(&mut self, w: u32, h: u32) { ... }
    pub fn present(&mut self, pixels: &[u8], pixel_width: u32, pixel_height: u32) { ... }
}
```

**src/shader.wgsl**:
```wgsl
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var pos = array<vec2<f32>,6>(
        vec2(-1.0,  1.0), vec2( 1.0,  1.0), vec2(-1.0, -1.0),
        vec2( 1.0,  1.0), vec2( 1.0, -1.0), vec2(-1.0, -1.0),
    );
    var uv = array<vec2<f32>,6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );
    var out: VertexOutput;
    out.clip_position = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = uv[idx];
    return out;
}

@group(0) @binding(0) var t_frame: texture_2d<f32>;
@group(0) @binding(1) var s_frame: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_frame, s_frame, in.uv);
}
```

### Step 2 — rosace-platform integration

Add `rosace-compositor` as an optional dep behind `features = ["gpu"]`.

In `AppState`:
- Add `presenter: Option<GpuPresenter>`
- In `resumed()`: try `GpuPresenter::new(window, w, h)`, set `presenter`
- In `RedrawRequested`: if `presenter.is_some()` → call `presenter.present(pixels)`;
  else → fallback softbuffer path (unchanged)
- In `Resized`: call `presenter.resize(w, h)` if present

### Step 3 — pollster dep

wgpu's `request_adapter` / `request_device` are async. Use `pollster::block_on`
to bridge from sync `run()` at startup. `pollster = "0.3"` in compositor deps.

### Step 4 — phase15_demo

Simple counter demo run with `GpuPresenter` active. On macOS Metal backend.
Print to stderr: "wgpu: Metal backend, adapter = Apple M-series GPU".

---

## Exit Criteria

```
□ GpuPresenter initializes on macOS (Metal backend, no crash)
□ GpuPresenter.present(pixels) blits the CPU frame to screen correctly
□ App renders identically to softbuffer path (pixel-for-pixel on flat colors)
□ phase15_demo runs at 60fps; no dropped frames on idle
□ Fallback: if wgpu init fails → softbuffer path activates automatically
□ No regression in any other demo (phase13_demo, phase14_demo still work)
□ All workspace tests pass
□ D072–D075 logged in DECISIONS.md
```

---

## Approved dependencies (new)
- `wgpu = "24"` in rosace-compositor
- `pollster = "0.3"` in rosace-compositor
- `raw-window-handle = "0.6"` (already used by winit/softbuffer transitively)

## DO NOT
- DO NOT replace tiny-skia with GPU drawing (D032 — v1.0 milestone)
- DO NOT add multi-layer GPU compositing (Phase 16)
- DO NOT add wgpu texture atlas (Phase 16)
- DO NOT add opacity/transform GPU blend modes (Phase 16)
