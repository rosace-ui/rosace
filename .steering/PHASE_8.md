# Phase 8 — Renderer Abstraction, IME, Unicode Bidi, Media Stubs

> Status: IN PROGRESS
> Target: Production-quality input and rendering abstraction layer

## Steps

### Step 1 — Renderer abstraction layer (D032 prep)
- New crate `rosace-renderer` (trait crate, not the impl)
- `Renderer` trait: `clear(color)`, `fill_rect`, `stroke_rect`, `fill_circle`, `draw_text`, `encode_png() -> Vec<u8>`, `width() -> u32`, `height() -> u32`
- `RendererBackend` enum: `TinySkia`, `SkiaSafe` (for future swap)
- `SkiaRenderer` — wraps existing `SkiaCanvas` from `rosace-render`, implements `Renderer`
- This isolates all canvas calls so swapping to skia-safe in v1.0 only touches `SkiaRenderer`
- Re-export `Renderer`, `SkiaRenderer` from `rosace-renderer`

### Step 2 — IME input stub (`rosace-ime`)
- New crate `rosace-ime`
- `ImeEvent` enum: `Preedit { text: String, cursor_range: Option<(usize, usize)> }`, `Commit(String)`, `Disabled`, `Enabled`
- `ImeComposition` — tracks preedit state: `text: String`, `cursor: usize`, `active: bool`
- `ImeHandler` trait: `on_ime_event(&mut self, event: &ImeEvent)`
- `NoopIme` — stub that converts `ImeEvent::Commit(s)` to a simple `String` output
- `ImeState` — state machine: Idle → Composing → Committed → Idle
- Platform note: real OS integration deferred to v1.0; this crate provides the data model

### Step 3 — Unicode bidi levels (`rosace-bidi`)
- New crate `rosace-bidi`
- Implements a subset of Unicode TR#9 Bidirectional Algorithm (not full ICU)
- `BidiClass` enum: `L`, `R`, `AL`, `EN`, `AN`, `WS`, `ON`, `B`, `S`, `NSM` (10 most common classes)
- `bidi_class(ch: char) -> BidiClass` — classify a Unicode char into its bidi class
- `paragraph_level(text: &str) -> u8` — P2/P3: detect paragraph embedding level (0=LTR, 1=RTL)
- `resolve_levels(text: &str) -> Vec<u8>` — simplified X1-X8 + W rules, returns per-char level
- `reorder_line(text: &str, levels: &[u8]) -> String` — L2: reverse RTL runs for display
- `BidiParagraph { text: String, levels: Vec<u8>, base_level: u8 }`
- `BidiParagraph::new(text)` — full pipeline: classify → resolve → reorder

### Step 4 — Media stubs (`rosace-media`)
- New crate `rosace-media`
- `AudioFormat` enum: `Wav`, `Mp3`, `Ogg`, `Aac`
- `VideoFormat` enum: `Mp4`, `Webm`, `Gif`
- `MediaError` enum: `Unsupported`, `NotFound(String)`, `DecodeFailed(String)`, `PlatformUnavailable`
- `AudioPlayer` stub: `load(path: &str) -> Result<AudioHandle, MediaError>`, `play`, `pause`, `stop`, `volume(f32)`
- `AudioHandle { id: u64, format: AudioFormat, duration_secs: f32, playing: bool }`
- `VideoFrame { width: u32, height: u32, data: Vec<u8>, timestamp_ms: u64 }`
- `VideoDecoder` stub: `open(path: &str) -> Result<VideoDecoder, MediaError>`, `next_frame() -> Option<VideoFrame>`
- All impls return `Err(MediaError::PlatformUnavailable)` — real decoding is v1.0
- WASM: same stubs, same errors

### Step 5 — Phase 8 showcase
- `rosace-examples/src/bin/phase8_demo.rs`
- 1400×900 PNG, 4 panels:
  1. Renderer abstraction — Renderer trait diagram, SkiaRenderer wrapping SkiaCanvas
  2. IME input — composition state machine, preedit/commit flow
  3. Unicode bidi — per-char bidi level visualization for mixed LTR/RTL text
  4. Media stubs — AudioPlayer/VideoDecoder state machines, format cards

## Exit Criteria

- [ ] `Renderer` trait is object-safe (`Box<dyn Renderer>` compiles)
- [ ] `SkiaRenderer` passes all draw calls through to `SkiaCanvas`
- [ ] `ImeComposition` tracks preedit text and cursor correctly
- [ ] `ImeState` state machine transitions correctly
- [ ] `bidi_class('A') == BidiClass::L`, `bidi_class('أ') == BidiClass::AL`
- [ ] `paragraph_level("Hello") == 0`, `paragraph_level("مرحبا") == 1`
- [ ] `reorder_line` reverses RTL runs correctly
- [ ] `AudioPlayer::load` returns `Err(MediaError::PlatformUnavailable)` on all platforms
- [ ] All workspace tests pass, zero warnings, clean release build

## Approved dependencies

- No skia-safe yet — D032 says swap in v1.0; this phase is prep only
- No ICU4X or unicode-bidi crate — hand-roll the subset we need
- No rodio/cpal — media is stubs only
- No ffmpeg bindings — video stubs only

## DO NOT

- DO NOT implement real IME OS integration — v1.0
- DO NOT link skia-safe C++ — build times would explode; prep only
- DO NOT implement real audio/video decode — v1.0
- DO NOT add full 17-level bidi algorithm — this phase is L/R classification + basic reordering
- DO NOT add text shaping (HarfBuzz) — v1.0
