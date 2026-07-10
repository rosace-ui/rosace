# Phase 6 — Gestures, Rich Text, Network Images & Localization

> Status: PLANNED
> Target: Production-ready UX primitives — real touch input, i18n, network assets

## Steps

### Step 1 — Gesture recognition (`rosace-gesture`)
- `GestureRecognizer` trait: `on_event(InputEvent) -> Option<GestureEvent>`
- `GestureEvent` enum: `Tap { x, y }`, `DoubleTap { x, y }`, `LongPress { x, y, duration }`, `Swipe { direction, velocity }`, `Drag { dx, dy, phase: DragPhase }`, `Pinch { scale, center }` (desktop: scroll wheel)
- `SwipeDirection`: Left/Right/Up/Down
- `DragPhase`: Begin/Move/End
- `TapRecognizer`: detects single/double tap within 300ms window
- `SwipeRecognizer`: min 80px movement, velocity > 200px/s
- `DragRecognizer`: continuous position delta from MouseDown to MouseUp
- Platform: mouse events on desktop, touch events on WASM via web-sys TouchEvent

### Step 2 — Rich text layout (`rosace-text`)
- `TextSpan { text, font_size, color, bold, italic, underline }`
- `RichText` — list of `TextSpan`s forming a paragraph
- `TextLayout` — wraps `RichText` to a given max width, returns laid-out lines
- `TextLine { spans, baseline_y, width }` — a single wrapped line
- `word_wrap(text, max_width, char_width_fn)` — greedy word wrapper
- Text cursor: `TextCursor { line, col }`, `advance()`, `backspace()`
- Render: `SkiaCanvas::draw_rich_text(layout, x, y)`

### Step 3 — Network image loading (`rosace-net`)
- `ImageLoader` — async-friendly loader using `std::thread` + channel
- `LoadState<T>`: `Idle / Loading / Loaded(T) / Failed(String)`
- `RemoteImage { url, fit, width, height }` widget — shows placeholder while loading, swaps to image when `LoadState::Loaded`
- HTTP client: use `std::net::TcpStream` for basic GET requests (no reqwest dep)
- Caching: `ImageCache` keyed by URL string (reuse existing `ImageCache` from rosace-widgets)
- WASM: stub that returns `LoadState::Failed("net not available on web".into())`

### Step 4 — Custom painters (D034)
- `PainterContext` — wrapper around `SkiaCanvas` with clip rect and transform stack
- `CustomPainter` trait: `fn paint(&self, ctx: &mut PainterContext, size: Size)`
- `PainterWidget { painter: Box<dyn CustomPainter>, width, height }` in `rosace-widgets`
- Lets power users draw arbitrary shapes without wrapping every API

### Step 5 — Localization stubs (D044)
- `rosace-i18n/` new crate
- `Locale` struct: `{ language: String, region: Option<String> }` (e.g. "en", "en-US", "fr")
- `MessageBundle` — HashMap<String, String> (key → translated string)
- `t!(key)` macro — looks up key in the current locale bundle, falls back to key if missing
- `I18nProvider` — wraps `GlobalAtom<MessageBundle>`, `set_locale(locale, bundle)`, `t(key)`
- Bundle loading: from `&str` (newline-delimited `key=value` format), no JSON/TOML dep

### Step 6 — Phase 6 showcase
- `rosace-examples/src/bin/phase6_demo.rs`
- 1400×900 PNG, 4 panels:
  1. Gesture diagram — Tap/Swipe/Drag/Pinch event flows with arrows
  2. Rich text — paragraph with mixed bold/colored spans and word-wrap demo
  3. Network image — `RemoteImage` load states: Idle → Loading → Loaded → Failed
  4. i18n — same UI rendered in EN/FR/ES/JA with translated strings

## Exit Criteria

- [ ] `TapRecognizer` correctly identifies single vs double tap within 300ms
- [ ] `SwipeRecognizer` emits direction and velocity
- [ ] `word_wrap` produces correct line breaks at max width
- [ ] `TextLayout` renders multi-line text via `draw_rich_text`
- [ ] `ImageLoader` returns `LoadState::Loading` immediately, `Loaded` after thread completes
- [ ] `PainterWidget` calls `CustomPainter::paint` each frame
- [ ] `t!("greeting")` returns the translated string from the active bundle
- [ ] All workspace tests pass with zero warnings
- [ ] `cargo build --release` is clean

## Approved dependencies

- No reqwest — HTTP is raw TcpStream or stubbed
- No serde for i18n — key=value plain text format
- No tokio/async-std — threading via `std::thread` + `mpsc`
- `web-sys` already present — use `TouchEvent` for WASM gesture support

## DO NOT

- DO NOT add multitouch pinch on desktop (mouse has no pinch) — WASM only
- DO NOT add RTL text rendering (D020) — Phase 7
- DO NOT add text selection UI — Phase 7
- DO NOT add OS clipboard integration — Phase 7
- DO NOT add WebSocket or streaming — Phase 7
