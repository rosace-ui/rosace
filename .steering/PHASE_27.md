# Phase 27 — GPU-Native Core Rendering (D109)

> Status: Scoped, not started.
> Started: —
> Completed: —
> Decision: **D109** — move ROSACE's core rendering off `tiny-skia` (CPU
> software rasterization) onto wgpu GPU shaders, for both built-in shapes
> and custom effects, through one `PipelineRegistry` mechanism. Text moves
> to a cached GPU glyph atlas. This is a rewrite of D109's original,
> narrower scope (a custom-shader escape hatch only) — that use case now
> falls out of the core mechanism for free. Backdrop blur/glassmorphism
> stays a named, deliberately deferred follow-up, not part of this
> phase's exit bar.

## Why This Phase (and why it changed shape mid-scoping)

Raised 2026-07-10 out of a comparative review against `tessera-ui`. The
first pass at D109 only added a `ShaderPaint` escape hatch for *custom*
effects, leaving `tiny-skia` as the permanent renderer for every built-in
shape and all text. The user pushed back with real research: CPU
software rasterization is a known-bad tradeoff for a framework that wants
sustained animation at 120fps on battery-constrained devices, and named
specific industry precedent (Flutter's Impeller, Compose's Skia-GPU
backend, SwiftUI's Metal compositing) as evidence it's not a niche
concern.

**Confirmed, not assumed**: `rosace-render`/`rosace-widgets` pin
`tiny-skia = "0.11"` — a pure-Rust reimplementation of only Skia's
CPU/software raster backend (it never had a GPU mode). `rosace-compositor`
only touches the GPU as a blit-only compositor: upload a CPU-drawn pixel
buffer as a texture, draw one fullscreen quad. Damage-rect + frame-skip
(already landed, see `RENDER_ENGINE.md`) only helps *idle* apps burn zero
CPU; it does nothing for D108's pervasive default animation, where
animated widgets are dirty by design every frame for the animation's
duration — every one of those frames pays a real CPU tiny-skia
rasterization cost today.

**Also confirmed by code review 2026-07-10 (missing from this doc's first
draft — each item below is verified in code, not assumed):**

1. **Text is NOT re-rasterized every frame today.** `rosace-render`'s
   `FontCache` (fontdue) rasterizes each distinct glyph once into a
   CPU-side cache (`glyph_cache` in `font.rs`). The real per-frame text
   cost is blitting those cached coverage bitmaps into the pixel buffer
   plus uploading the buffer — that is what Step 4 actually eliminates.
   State the win honestly so the before/after measurement targets the
   right thing.
2. **`rosace-shaping` is not in the render path at all.** `DrawText`
   renders through `FontCache` directly; `ShapingEngine`/`FallbackShaper`
   has zero call sites outside the umbrella crate's re-export — another
   built-but-never-wired crate (Known Issue #12, `CRATE_CONTRACTS.md`).
3. **Web never touches the GPU.** `rosace-platform/src/web.rs` presents
   via 2D-canvas `putImageData` and never constructs a `GpuPresenter`.
   This phase is desktop/mobile-only; see Out of Scope.
4. **Desktop has a softbuffer CPU fallback** (`app.rs`, taken when
   `GpuPresenter::new` returns `None`) that CPU-composites the canvas
   pixels directly. GPU-only shapes would render nothing on that path.
5. **The base frame is ONE full-screen CPU buffer.** `app.rs` composites
   exactly: base `CompositorLayer::tracked(canvas.pixels(), ...)` →
   scroll layers (`CompositorLayer::placed`, each a CPU content buffer
   sampled with a GPU offset, D090) → overlay. There is no per-command
   layer structure to interleave GPU work into today — see Design
   Constraints below, this is the phase's central design problem.

**What the comparison actually showed**: `tessera-ui`'s renderer has no
CPU shape rasterizer at all — every primitive is a registered wgpu
`DrawablePipeline` with its own WGSL fragment shader; text uses `glyphon`
(rasterize each distinct glyph once via `cosmic-text`, cache in a GPU
texture atlas, every later frame is pure GPU sampling). The same pattern
holds industry-wide: Flutter replaced Skia's CPU-adjacent path with
Impeller specifically to fix jank/battery drain from CPU tessellation and
JIT shader compilation; Compose uses Skia's GPU backend (Ganesh), never
its CPU backend; SwiftUI is GPU-composited via Core Animation/Metal
throughout. ROSACE is currently the outlier among every framework
compared.

## Explicit lesson carried in from Impeller's own mistakes

Impeller initially shipped with runtime (JIT) shader compilation and had
to fix first-render/first-animation stutter by moving to ahead-of-time
compilation. This phase avoids repeating that: every registered pipeline
compiles at registration time (app startup / `register_shader` call),
never lazily on first paint. No step in this phase ships a
"compile-on-demand" path, even temporarily.

## Design Constraints (found by code review 2026-07-10 — resolve in-phase, before the step each one blocks)

- **C1 — Command-granularity CPU/GPU z-order interleaving (blocks Step 3;
  design it during Step 2).** Step 2's `CompositorLayer::Shader`
  interleaves at *layer* granularity, but Step 3 migrates at *command*
  granularity while `DrawText`/`BlitRgba`/clips stay CPU: a card's
  GPU-drawn background must render below its own CPU-drawn text but above
  earlier CPU-drawn content. Stacking whole layers cannot express that
  without splitting the single base buffer at every CPU↔GPU boundary —
  unbounded full-screen uploads per frame. The workable shape is one GPU
  render pass per frame that draws built-in shape quads AND CPU-rasterized
  content (glyph/blit regions uploaded as textured quads) in command
  order. Step 2's exit now includes writing this design down in this doc
  before Step 3 starts — do not discover it mid-Step-3.
- **C2 — D090 scroll layers assume CPU content buffers (blocks Step 3's
  exit measurement).** Scroll content lives in a CPU pixel buffer
  (`sl.pixels`) that the GPU samples with an offset. GPU-drawn shapes
  inside a scrolled list have no CPU buffer to land in — scroll-layer
  content needs render-to-texture (or re-expression inside C1's render
  pass). Step 3's exit measures momentum scroll, which hits this
  immediately.
- **C3 — Softbuffer fallback (blocks Step 3).** Decide explicitly:
  keep tiny-skia as the fallback renderer when `GpuPresenter::new` fails
  (recommended — consistent with tiny-skia surviving this phase anyway),
  and verify the fallback still renders shapes after Step 3, or drop
  softbuffer support as a named decision. Silence here means blank UIs on
  GPU-init failure.
- **C4 — Damage-rect/frame-skip semantics change (blocks Step 3's exit
  measurement).** Today's dirty tracking is buffer-based (per-layer
  `dirty`, `take_frame_dirty`). In a GPU pass, a dirty frame is a full
  re-record/redraw (normal and cheap on GPU) — but the frame-skip
  behavior (nothing dirty → no present at all, idle apps burn zero
  CPU/GPU) MUST be preserved and re-verified, since it's a landed,
  documented win (`RENDER_ENGINE.md`).

## C1 Resolution — segment interleaving design (written during Step 2, as required)

Step 3's executor partitions each Picture's command stream **in order** into
segments:

- A run of GPU-able commands (the 8 built-in shape variants) becomes
  consecutive `FrameItem::Shader` quads (built-in pipelines).
- A run of CPU commands (`DrawText`/`BlitRgba` until Steps 4/later land)
  rasterizes into its own transparent sub-buffer sized to the run's
  bounding box — reusing today's tiny-skia code paths untouched — and is
  presented as a `FrameItem::Pixels` with `dest` = that bbox. The D090
  placed-layer mechanism already supports exactly this; no new compositor
  feature is needed.
- The `FrameItem` list IS the z-order: `present_frame` (landed in Step 2)
  draws items strictly in slice order, so a GPU shape between two CPU
  segments composites exactly where the command stream put it — the Stack
  case (text under a card under more text) is correct by construction,
  because every CPU segment owns its own texture.
- Cost bound: segment count = number of CPU↔GPU transitions in the stream,
  not full-screen buffers; contiguous text runs coalesce. The compositor's
  existing per-slot caches (D089) mean an unchanged text segment re-uploads
  nothing on frames where only a shape animated. Step 4 (glyph atlas) then
  removes most CPU segments entirely, collapsing frames to pure quads.
- C2 sketch (detail at Step 3): scroll-layer content gets the same
  treatment at publish time, rendered into an offscreen texture
  (render-to-texture) and sampled with the live offset per frame.
- C3 DECIDED during Step 2: GPU-shapes mode is a runtime switch the
  platform enables only when a `GpuPresenter` exists — softbuffer and web
  automatically keep the full tiny-skia path. (Today, ShaderFill-only
  content is dropped with a loud one-time warning on those paths.)

## Out of Scope (deliberately, not silently dropped)

- **Web/wasm GPU rendering.** Confirmed: web presents CPU pixels via
  2D-canvas `putImageData` (`web.rs`) and never constructs a
  `GpuPresenter`. Bringing this phase's pipelines to web (wgpu WebGPU
  backend, canvas surface creation, atlas upload on wasm) is its own
  future phase. Stated consequence: **web keeps the full tiny-skia path,
  so "delete tiny-skia" is a desktop/mobile endgame gated on web catching
  up — not achievable at the end of this phase even if every step
  lands.** The final-cleanup note below is bounded by this.
- **Backdrop blur / glassmorphism.** Needs `rosace-compositor` to render
  every layer below a blurred rect to an offscreen texture first, then
  sample it in a second pass — real new two-pass render-target state in
  `GpuPresenter`, not just a pipeline registration. Reuses this phase's
  registry once it exists, scoped in detail only after Steps 1-4 land
  and are verified.
- **Deleting `tiny-skia` outright.** It stays as the renderer for
  anything not yet migrated until GPU parity is proven shape-by-shape
  and for text. Removing it is the natural conclusion of this phase, not
  a step inside it — tracked as a final cleanup once every step below
  has shipped and been verified in a real app, AND once web has a GPU
  path (see the web/wasm bullet above — web keeps tiny-skia until then).
- **A built-in shader effect library** (blur, noise, procedural
  gradients as ready-to-use `ShaderSpec`s beyond the built-in shapes
  this phase migrates). `rosace-shader` is the intended future home,
  but shipping a library before the mechanism has real, verified
  consumers is designing blind.
- **Hot-reloading shader source.** Separate, independently scopable once
  shaders exist at all.

## Steps

### Step 1 — `rosace-shader` crate + typed uniform derive
New Layer 5 crate. Exports `PipelineId` (newtype, stable hash/eq),
`ShaderSpec { wgsl_source: String, uniform_layout: ..., blend: BlendMode }`,
and the `ShaderUniforms` trait (`fn to_bytes(&self) -> Vec<u8>`). The
`#[derive(ShaderUniforms)]` macro (`rosace-macros`, proc-macro only per
its existing "no runtime logic" contract) generates `to_bytes()` with
compile-time-checked field-order/alignment packing — no widget author
hand-packs a byte buffer. `rosace-shader` depends only on `rosace-core`
(types) — zero wgpu dependency of its own; wgpu types stay inside
`rosace-compositor`.

Exit: a standalone unit test defines a `#[derive(ShaderUniforms)] struct
Foo { a: f32, b: [f32; 4] }`, calls `to_bytes()`, asserts the exact
expected byte layout/length. No UI involved yet.

### Step 2 — Registry + `DrawCommand::ShaderFill` + eager compilation
`rosace-render`: new `DrawCommand::ShaderFill { pipeline_id: PipelineId,
rect: Rect, uniforms: Vec<u8> }`, threaded through `offset`/`morph` like
every other variant (Hero transitions and damage-rect translation work on
GPU-drawn regions for free). `rosace-compositor`: `GpuPresenter::register_shader(...)` compiles and
stores a `wgpu::RenderPipeline` keyed by pipeline id — compiled once at
registration (never lazily, per the Impeller lesson above), a stable
resource per D091 discipline, not per-node/per-paint state. **Contract
resolution (added 2026-07-10)**: `rosace-compositor` is Layer 0 with a
hard "zero rosace-* deps" contract, so it cannot import `rosace-shader`'s
types — its registration API takes only primitive/std types (`u64` id,
`&str` WGSL, a compositor-owned blend enum); `rosace-platform` (already
its only consumer) converts from the typed `ShaderSpec` at the boundary. New `CompositorLayer::Shader { pipeline_id, rect, uniforms }`
variant; `present_layers` executes it interleaved with existing
pixel-buffer layers in z-order, binding the stored pipeline and
uploading only the uniform bytes each call.

Exit: a real running app registers one trivial shader (e.g. a solid-color
fragment shader) and renders it via a raw `DrawCommand::ShaderFill` (no
widget wrapper yet), pixel-verified end to end: record → registry lookup
→ compositor-layer → present. **Additionally** (added 2026-07-10): the
command-granularity interleaving design (Constraint C1 above) is written
into this doc before Step 3 starts — one shader layer above/below the
base buffer proves the registry, not the migration architecture.

### Step 3 — Built-in shapes move to GPU (the actual point of this phase)
Migrate `FillRect`, `FillRRect`, `FillCircle`, `StrokeRect`,
`StrokeRRect`, `FillGradient`, `FillArc`, `DrawShadow` from `tiny-skia`
CPU calls to built-in registered pipelines (SDF-style WGSL fragment
shaders — a well-understood technique for exactly these shapes) using
Step 2's registry. (`FillCircle` was missing from this list as first
drafted — it's real and heavily used: `radio`, `switch`, `slider`,
`avatar`, `badge`, `icon`, `app_bar` all draw it. Omitting it would have
left tiny-skia permanently required for radio buttons.) This step is
gated on Constraints C1-C4 above having written resolutions. Existing call sites (`cx.fill_rect(...)` etc.) are unchanged —
only the implementation moves. `BlitRgba`/`DrawText`/`PushClip`/`PopClip`
are explicitly NOT touched by this step (text is Step 4; clip and raw
blits need their own evaluation).

Exit: every existing widget-rendering test still passes unchanged
(proves call-site compatibility); a real running app is pixel-compared
before/after this step on a representative screen (buttons, cards,
gradients, shadows) to confirm visual parity, not just "it compiles."
Frame-time/CPU-usage measured during a sustained animation (e.g. Phase
26's press-feedback or momentum scroll) before and after, to confirm the
actual motivation for this phase — lower CPU during animation — is real,
not assumed.

### Step 4 — Cached GPU glyph atlas for text
Replace `DrawText`'s current CPU compositing path — cached glyph coverage
bitmaps blitted into the pixel buffer every frame — with a glyphon-style
mechanism built on the real text stack: `rosace-render`'s `FontCache`
(fontdue) already rasterizes each distinct glyph once into a CPU-side
cache; keep that rasterization, move the cache's *storage* into a GPU
texture atlas, and render already-seen glyphs as pure GPU instanced quads
on every subsequent frame. **Correction from the 2026-07-10 review**: the
first draft said "reuse the existing `rosace-shaping`/`rosace-text`
shaping pipeline" — but `rosace-shaping` has zero call sites in the
render path (Known Issue #12); `DrawText` goes through `FontCache`
directly, and `rosace-text` contributes only `word_wrap` at the widget
layer. Do NOT wire `rosace-shaping` in as a side effect of this step —
that is its own future decision. This is the
highest-risk step (font atlas eviction/growth, subpixel positioning,
HiDPI scale interaction with the existing HiDPI fixes in
`RENDER_ENGINE.md`) — scoped last, after Step 3 proves the registry
mechanism works for real in production shapes.

Exit: real app, real text-heavy screen, pixel-verified for correctness
(not just "text appears") — kerning, baseline, HiDPI scale all re-checked
against the existing `project_text_rendering` conventions, since this
step touches the same code paths those fixes landed in. CPU-usage
comparison during a scroll of a long text list, before/after.

### Step 5 — `ShaderPaint` widget (the original custom-effect use case)
Now that the registry is proven by Steps 2-4's real, built-in consumers,
add the app-facing `ShaderPaint` widget (own `Widget` impl, own
`layout()`/`size()` builder matching `CustomPaint`'s ergonomics) that
records `DrawCommand::ShaderFill` for *custom* app-authored effects. Does
NOT extend `CustomPaint`'s closure — a dedicated type, per D109.

Exit: one real novel-effect shader (something `tiny-skia`'s built-in
vocabulary couldn't express — e.g. animated procedural noise) shipped in
a demo-app screen, screenshotted in a real running app.

## Sequencing

Steps 1→2→3 are strictly sequential. Step 4 depends on Step 2's registry
but is independent of Step 3 (can be scoped in parallel once Step 2
lands, though doing Step 3 first is recommended — it's lower-risk and
validates the mechanism before the higher-risk text work). Step 5 depends
on Step 2 only, but should come last so it's built on a registry that's
already proven by real built-in consumers, not the first thing routed
through it.

## Migration Rule

Every existing widget's call sites (`cx.fill_rect`, `cx.draw_text`, etc.)
are unchanged through Steps 1-4 — only the implementation underneath
moves from CPU to GPU. No app code changes required to benefit. `tiny-skia`
is not removed by this phase (see Out of Scope) — it can be dropped as a
dependency only once every shape and text are proven at GPU parity AND
web has its own GPU presenter.

## Tracking Checklist

Scoping-review fixes (2026-07-10 code review of this doc — all applied):

- [x] `FillCircle` added to Step 3's migration list (was omitted; 7+
      widgets draw it — radio, switch, slider, avatar, badge, icon,
      app_bar)
- [x] D109 + Step 4 corrected: fontdue `FontCache` is the real glyph
      path — not `cosmic-text`, not `rosace-shaping` (which has zero
      render-path call sites → Known Issue #12)
- [x] Compositor Layer-0 contract contradiction resolved on paper
      (primitives-only registration API; `rosace-platform` converts)
- [x] Web/wasm reality named: `putImageData` path, no `GpuPresenter` on
      web, tiny-skia not deletable until a web GPU phase exists
- [x] Softbuffer fallback + damage-rect/frame-skip + D090 scroll layers
      named as in-phase design constraints (C1-C4)
- [x] Text-cost motivation corrected (glyphs already cached CPU-side;
      the win is eliminating per-frame bitmap blits + buffer upload)

Design constraints (each must have a written resolution before the step
it blocks):

- [x] C1 — command-granularity CPU/GPU z-order interleaving design
      (written 2026-07-10, see "C1 Resolution" section above; the ordered
      `FrameItem` executor it needs landed with Step 2)
- [ ] C2 — D090 scroll-layer content under GPU shapes (before Step 3
      exit)
- [x] C3 — softbuffer fallback decision (2026-07-10: GPU-shapes mode is a
      runtime switch enabled only when a `GpuPresenter` exists; softbuffer
      + web keep the full tiny-skia path — see C1 Resolution)
- [ ] C4 — damage-rect/frame-skip preservation re-verified on the GPU
      path (before Step 3 exit)

Steps:

- [x] Step 1 — `rosace-shader` crate + `#[derive(ShaderUniforms)]`
      (landed 2026-07-10: 7 layout/queue unit tests asserting exact WGSL
      uniform byte layouts incl. the vec3-tail and vec2-padding edge
      cases; derive rejects unsupported/tuple/empty structs at compile
      time; `ShaderRegister` trace variant added. Layering note:
      `DrawCommand::ShaderFill` will carry the raw `u64` — render is
      Layer 4, cannot import Layer 5's `PipelineId`.)
- [x] Step 2 — registry + `DrawCommand::ShaderFill` + eager compilation
      (+ C1 design written) — landed 2026-07-10, exit bar met for real: a
      running app registered a two-color split shader (uniforms via the
      derive) and rendered it through a raw `ShaderFill` recorded in
      `CustomPaint`; window pixel-scanned: left half green, right half
      blue (0,0,245), at the recorded rect. Landed pieces: compositor
      `register_shader` (primitives-only, eager, error-scoped),
      `present_frame(&[FrameItem])` ordered executor with per-slot quad
      caches + skip-present extended to quads (C4-compatible: animated
      shaders must take time as a uniform), `SkiaCanvas` quad collection
      with widget-clip (not damage-clip) capture, platform drain at
      startup + per frame, retained quads across skipped frames, warn-once
      drops on softbuffer/web/overlay paths, `PaintCtx::shader_fill`.
- [ ] Step 3 — built-in shapes on GPU (incl. `FillCircle`), pixel parity
      + animation CPU measurement
      - [x] 3a (landed 2026-07-10): `rosace-shader::builtin` — 5 SDF
            pipelines covering all 8 shape variants (fill-rrect serves
            rect/rrect/circle; stroke-rrect serves both strokes; gradient;
            arc with round caps; Gaussian-approx shadow), one shared
            80-byte uniform layout, shape→(quad, uniforms) conversion fns
            that are scale-agnostic (uniforms carry the quad's recorded
            size; the shader derives px_scale, so logical-px recordings
            stay HiDPI-correct). Parity verified in a real running A/B app
            (`builtin_shapes_ab`, CPU column vs GPU column): rect/circle/
            stroke/arc visually identical; gradient BYTE-IDENTICAL at
            top/mid/bottom samples after switching the shader to
            tiny-skia's sRGB-space stop interpolation (linear-space mixing
            measured +16/255 red at midpoint — real find); shadow falloff
            profiles match within ~3/255 with σ = blur/2. sRGB→linear
            color conversion round-trip unit-tested against the known
            gamma-bug color (#2B2D30).
      - [x] 3b (landed 2026-07-11): `play_picture` GPU-shapes mode — the 8
            shape variants divert to builtin quads; text/blits rasterize
            into per-run bbox segments cut out of the scratch pixmap
            (transparent-erased after extraction, so z-order interleaving
            is correct by construction — unit-tested with the
            shape→text→shape Stack case); `clear()` becomes the frame's
            background quad; platform enables per-canvas (base only —
            scroll/overlay canvases confirmed to be separate `SkiaCanvas`
            instances, engine.rs:391, so they stay CPU until C2), carries
            the flag across resize recreation, retains items through
            frame-skip; engine forces full repaint in GPU mode (damage
            culling is a CPU-buffer economy). Kill switch + A/B lever:
            `ROSACE_CPU_SHAPES=1`. Verified for real: all existing widget
            tests pass unchanged; app_demo pixel-compared CPU-vs-GPU on
            two screens via `ROSACE_DEMO_SCREEN` deep-link (home: 99.53%
            of pixels within 8/255; Widget Gallery: 99.96%, zero pixels
            differ >60/255, remainder is AA-edge noise).
      - [x] 3c-measurement (2026-07-11): `bench_paint` (release,
            Retina-sized 1800×1280, gallery-like 24-row screen + spinner,
            steady-state animated frames): CPU shapes 7.595 ms/frame →
            GPU shapes 0.748 ms/frame — **10.2× lower CPU-side cost per
            animated frame** (excludes CPU-mode's full-frame texture
            upload, so the real gap is larger). The remaining 0.75ms is
            text raster + segment copies — exactly Step 4's target.
            Idle evidence for C4: occluded GPU-mode app measured 0.00s
            CPU over 10s (frame-skip + skip-present engaged).
      - [x] 3c-C2 (landed 2026-07-11): scroll-layer render-to-texture —
            engine propagates GPU mode to content canvases (items instead
            of a pixel-buffer copy), `GpuPresenter::render_offscreen`
            renders them into a per-key offscreen texture on publish
            frames (transient resources by design — publishes are rare vs
            scrolled frames), `FrameItem::Offscreen` samples it at the
            live scroll offset per frame; skip-present covers offscreen
            refs; unreferenced targets evicted after each present. Real
            bug caught during verification: the offscreen target MUST use
            the surface's format (macOS is Bgra8UnormSrgb; Rgba8 aborted
            at pipeline/attachment validation). Verified on app_demo's
            GPU Scroll Layer screen: CPU-vs-GPU settled diff 0.66% within
            8/255, ZERO pixels >60 (an earlier 4.34% reading was the
            scroll settle animation caught at different times — offset is
            live by design); row pitch byte-identical (111px both modes).
      - [x] 3c-C4 (2026-07-11): frame-skip verified on the GPU path with
            logs — idle app emits repeating
            "skip present (1 layers + 7 quads + 1 offscreen unchanged)"
            and 0.00s process CPU over 10s; dirty tracking upstream
            (needs_paint) unchanged.
      - [ ] 3c-remaining: on-device mobile sanity check (same code path
            as desktop, unverified there — do alongside the next `rsc
            run` mobile session rather than as its own boot cycle).
- [ ] Step 4 — GPU glyph atlas on `FontCache`, text-heavy scroll
      measurement
- [ ] Step 5 — `ShaderPaint` widget + real novel-effect demo
- [ ] Final cleanup (desktop/mobile tiny-skia removal) — BLOCKED on web
      GPU phase, do not attempt inside this phase
