# Phase 34 — Video widget + controller (D128)

> Status: PLANNED (scoped 2026-08-04, not started).
> Decision: **D128** — read DECISIONS.md first; it holds the constraints, the four researched facts this plan is built on, and why Flutter's/Compose's own approaches weren't copied as-is.
> Prereqs: D127 Platform Channel (LANDED), D109 GPU-shapes rendering (LANDED), D107 web SEO DOM shadow tree (LANDED, reused for the web adapter).
> Scope: single-video playback (local file or HTTP URL) with native decode per platform. Not a media framework — no DRM, no adaptive bitrate, no subtitle tracks, no playlist/queue management. Desktop (Windows/Linux) real decode is explicitly out of scope for this phase (see Step 8).

## Why This Phase

`rosace-media::VideoDecoder` has existed as a stub since its creation — `open()` unconditionally returns `MediaError::PlatformUnavailable`, `next_frame()` unconditionally returns `None`. No widget, no controller, no wiring to `rosace-ffi` exists at all. Before writing any of that, five real subsystems were read in full to find out what's actually reusable and what genuinely isn't, rather than assuming either "we can just wire up rosace-media" or "we need to build everything from scratch":

- **`rosace-ffi`'s Platform Channel (D127)** is a one-shot request/response bridge (`invoke_method` → an `Atom<ChannelCallState>` that resolves once via `report_call_result`/`_error`). D127's own text names "event channels (continuous native→Rust stream)" as an explicit, undone v1 deferral — there is no existing mechanism for native to push a steady stream of frames into Rust.
- **`rosace-compositor`** (2979 lines, read in full) has six wgpu render pipelines and a real `FrameItem` enum (`Pixels`/`Shader`/`Offscreen`/`Glyphs`/`Image`/`Backdrop`), but nothing resembling Flutter's `TextureRegistry` — no way to hand it an externally-produced GPU texture and have it sampled directly into a frame's render pass. Everything it draws is either CPU-rasterized bytes or content it assembled itself from `CanvasFrameItem`s.
- **`DrawCommand::BlitRgba`** — the existing "raw pixels in" primitive — already has a working GPU-shapes path (`CanvasFrameItem::Image` → `rosace-compositor::ImageQuad`), but its cache is keyed by a **content hash of the pixel bytes** (`blit_key`), read once per distinct hash and reused thereafter. A video frame is different bytes every tick by definition; pushing raw frames through this path as-is means hashing and re-uploading the full frame every single tick — the opposite of what the cache is for.
- **No raw-pixel FFI convention exists to imitate.** `rsc_engine_frame(engine: *mut Engine)` takes no buffer — ROSACE's own rendered frame never crosses the FFI boundary as bytes; `wgpu` draws straight into the native `CAMetalLayer`/`ANativeWindow` surface it was configured against. Mobile video frame delivery has no existing "give Rust some bytes each frame" channel to reuse; it's a genuine gap.
- **`rosace-net::use_query`/`LoadState<T>`** is the established idiom for "this needs an async round trip, expose it as a reactive load-state" — per-instance `ctx.state(...)` cells, a background completion writing an `Atom` directly (no polling loop the app has to run), an unmount guard (`alive: Atom<Arc<AtomicBool>>` + `on_unmount`) discarding stale results. `VideoController`'s `open()` should be built the same way, substituting a platform-channel round trip for the HTTP GET.

**Flutter vs Compose, compared, and why neither was copied wholesale**: Flutter's `video_player` registers an external GPU texture (`CVPixelBuffer` on iOS via `AVPlayerItemVideoOutput`, `SurfaceTexture` on Android backed by `MediaCodec`'s hardware decoder) and the engine's raster thread samples the *current* frame of that texture directly into the same Skia/Impeller command buffer as everything else — zero CPU copy, one compositor. This is the real ceiling, but it needs an `import_texture`-class primitive in the receiving renderer, which `rosace-compositor` doesn't have. Jetpack Compose's Android answer is different in kind, not just degree: `AndroidView { PlayerView(...) }` literally embeds ExoPlayer's own native `View` as a sibling inside the Compose tree, which works because Compose UI on Android still lives inside a real Android `Window`/`ViewGroup` that the OS itself composites. ROSACE's mobile rendering doesn't have that structure — confirmed via `rsc_engine_frame`/`CAMetalLayer` wiring (D106): Rust owns the entire GPU surface and draws every pixel itself; there's no OS-level view compositor interleaving ROSACE content with native views today. Native-view z-order embedding (Flutter's own "Hybrid Composition" `PlatformView` mode does exactly this) is *possible* but is new mobile-rendering-model surface with Flutter's own documented downsides (clipping/rotation/3D-transform don't compose correctly across the boundary) — named in Future-Proofing, not chosen for v1.

## The VideoController Model

```rust
// rosace-media/src/video_controller.rs

pub enum VideoMetadata {
    // duration, natural size, whether the source has audio, etc.
}

pub struct VideoController {
    // per-instance, NOT GlobalAtom — multiple concurrent players need
    // independent state, same reasoning use_query's per-call `ctx.state`
    // cells already establish.
    state: Atom<LoadState<VideoMetadata>>,   // rosace_net::LoadState<T>, reused as-is
    position: Atom<Duration>,
    playing: Atom<bool>,
    buffering: Atom<bool>,
    volume: Atom<f32>,
    frame: Atom<Option<VideoFrameHandle>>,   // instance_id + generation + pixels, see below
    alive: Atom<Arc<AtomicBool>>,            // same unmount-guard shape as use_query
}

impl VideoController {
    pub fn open(ctx: &mut Context, source: VideoSource) -> Self { /* mirrors use_query's body */ }
    pub fn play(&self);
    pub fn pause(&self);
    pub fn seek_to(&self, pos: Duration);
    pub fn set_volume(&self, v: f32);
    pub fn set_looping(&self, v: bool);
}
```

`open()`'s async round trip goes over the existing Platform Channel (`invoke_method("rosace/video", "open", {source, instance_id})`) exactly like a capability request — this is small, infrequent control data, squarely what D127 is for. Frame delivery is the one thing that does **not** go over that channel (see Step 2).

## The Rendering Path (core design)

```rust
// rosace-render/src/draw_command.rs — new sibling of BlitRgba, not a reuse of it
DrawCommand::VideoFrame {
    instance_id: u64,      // one per live VideoController
    generation: u64,       // monotonic per delivered frame — the cache key
    pixels: Arc<Vec<u8>>,  // RGBA, src_width * src_height * 4
    src_width: u32,
    src_height: u32,
    dest_rect: Rect,
}
```

```rust
// rosace-render/src/canvas.rs — GPU-shapes dispatch, mirrors BlitRgba's own
// arm but keys the frame-item cache by (instance_id, generation) instead of
// blit_key's content hash — correct invalidation for content that changes
// every tick, and it leaves BlitRgba's own static-image cache untouched.
DrawCommand::VideoFrame { instance_id, generation, pixels, src_width, src_height, dest_rect } => {
    if self.gpu_shapes {
        self.cut_segment();
        self.pending_frame_items.push(CanvasFrameItem::Video {
            key: (*instance_id, *generation),
            pixels: ImagePixels(pixels.clone()),
            src_w: *src_width, src_h: *src_height,
            dest: sr(*dest_rect).into(),
        });
    } else {
        self.blit_rgba(pixels, *src_width, *src_height, sr(*dest_rect), 1.0);
    }
}
```

`rosace-compositor` gets a matching `FrameItem::Video(VideoQuad)` — structurally identical to today's `ImageQuad`, differing only in cache-key semantics (generation counter, not content hash) and doc comment. No new wgpu pipeline needed; it reuses the existing `image_pipeline` (a video frame is, per-draw-call, just a textured quad).

## Steps

### Step 1 — `VideoController` + `LoadState`-based async open
- `rosace-media/src/video_controller.rs`: the struct above, `open()` built directly off `use_query`'s body shape (per-instance `ctx.state`, background-thread-equivalent via the Platform Channel's `Atom<ChannelCallState>`, unmount guard).
- Unit tests: state transitions (`Idle → Loading → Loaded/Failed`), unmount discards a late result, play/pause/seek are no-ops before `Loaded`.
- Exit: `cargo test -p rosace-media` green; a headless test opens a controller against a fake/mock channel response and observes the state transition.

### Step 2 — New `DrawCommand::VideoFrame` + CPU path
- `rosace-render/src/draw_command.rs`: add the variant (+ `.offset()`/`.morph()` arms, matching every other variant's pattern).
- `rosace-render/src/canvas.rs`: CPU path first (`self.blit_rgba(...)`, reusing the existing raster blit — works on every platform immediately, no GPU-shapes work required yet).
- Exit: a headless canvas test paints a `VideoFrame` command and confirms the destination rect's pixels match the input buffer (same shape as existing `BlitRgba` tests).

### Step 3 — GPU-shapes path
- `CanvasFrameItem::Video` (canvas.rs) + `rosace-compositor::FrameItem::Video`/`VideoQuad` (generation-keyed cache, reusing `image_pipeline`).
- Exit: a GPU-shapes-mode headless test confirms two calls with the same `(instance_id, generation)` reuse the cached texture (no re-upload), and a bumped `generation` triggers a fresh upload — the specific behavior `BlitRgba`'s content-hash cache would get wrong for this content.

### Step 4 — `VideoPlayer` widget
- `rosace-widgets/src/tree/video_player.rs`: layout (aspect-ratio-aware, matches `AspectRatio`'s existing sizing logic), poster-frame state (before `Loaded`), a controls overlay (play/pause, scrubber bound to `position`/`duration`, volume, fullscreen toggle) — reuses `CircularProgress::spinner()` for the loading state and `Slider` for the scrubber, no new low-level primitives.
- Exit: a headless paint test with a mock `VideoController` in each `LoadState` renders the expected chrome (poster / spinner / controls); wired into the showcase app once a real adapter exists (Step 5+), not before — a widget that can't actually play anything doesn't belong in the demo yet.

### Step 5 — macOS/iOS native adapter
- `AVPlayerItemVideoOutput` pulls `CVPixelBuffer`s; native copies to RGBA and calls the **new, dedicated, non-JSON** FFI export (binary pointer + `instance_id`/`generation`/`width`/`height` — not routed through `rsc_platform_channel_dispatch`'s JSON path, per D128).
- Control commands (`play`/`pause`/`seek`/`volume`) go over the existing Platform Channel, handled the same way camera/push already are.
- Exit: a real macOS app plays a real local `.mp4` — live-verified, not just headless-tested (per this project's standing "verify, don't assume" discipline for platform rendering).

### Step 6 — Android native adapter
- `ExoPlayer` + `ImageReader` (or `SurfaceTexture` + a GL readback), same frame-delivery FFI shape as Step 5.
- Exit: live-verified on a real Android device/emulator, same bar as Step 5.

### Step 7 — Web adapter
- The browser's own `<video>` element, bridged through the existing D107 semantic-DOM-shadow-tree machinery (`rosace-web-seo`) rather than a new mechanism — web is the one platform where "just use the native element" is nearly free.
- Exit: live-verified in a real browser tab.

### Step 8 — Desktop (Windows/Linux) — NOT started this phase
- Named, scoped, deliberately not built: needs an explicit decision on `ffmpeg-next` vs `gstreamer-rs` (both require a system-installed native library + LGPL/GPL licensing review, per D128) before any code lands. Tracked as an open sub-decision, not silently deferred.

## Future-Proofing (named now, wired later)

- **Zero-copy GPU texture import** (`wgpu::Device::create_texture_from_hal` or a platform shared-handle import) — the real v2 performance path once `rosace-compositor` has a second consumer that justifies the added surface area. `VideoFrame`'s `(instance_id, generation)` keying was chosen specifically so this can slot in later without a widget-facing API change — only the frame-delivery internals would swap.
- **Native-view z-order embedding** (Compose/Flutter-Hybrid-Composition style) — possible, not chosen; would need its own decision about ROSACE's mobile rendering model, not a video-specific one.
- DRM (FairPlay/Widevine), adaptive bitrate (HLS/DASH), subtitle/caption tracks, background/audio-only playback lifecycle.
- Desktop real decode backend (Step 8).

## Performance Guardrails

- Frame bytes never go through the JSON Platform Channel — a dedicated binary FFI entrypoint only (D128 fact #4's whole point).
- Cache invalidation is generation-keyed, never content-hash, for anything frame-like — re-hashing a full RGBA buffer every tick is wasted work `BlitRgba`'s existing cache was never meant to absorb.
- The CPU decode→RGBA→upload copy (v1's accepted cost, not hidden) means large frame sizes will cost real bandwidth; `VideoPlayer` should default to a sane max render size rather than uncritically trusting an arbitrarily large source resolution — revisit once Step 1 of the zero-copy Future-Proofing item lands.

## Migration Rule

Purely additive. `rosace-media::VideoDecoder`'s existing stub stays exactly as-is (still returns `PlatformUnavailable`) until a real per-platform adapter (Steps 5-7) actually replaces it — no widget-facing API changes once `VideoPlayer`/`VideoController` ship; a stubbed platform just means `LoadState::Failed` at `open()`, not a different shape to code against.
