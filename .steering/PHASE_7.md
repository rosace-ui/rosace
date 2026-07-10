# Phase 7 — Text Polish, Clipboard, WebSocket, Multi-touch

> Status: IN PROGRESS
> Target: Production-quality text, real input, live data, WASM touch

## Steps

### Step 1 — Real glyph metrics in rosace-text
- Replace `char_count * font_size * 0.55` heuristic with fontdue `metrics()` per glyph
- Add `GlyphMetrics { advance_width, height, bearing_x, bearing_y }` wrapper
- `measure_text(text, font_size, font: &FontCache) -> f32` — accurate pixel width
- Update `TextSpan::estimated_width()` to delegate to `measure_text` when a `FontCache` is available
- Update `word_wrap` to accept a `measure_fn: impl Fn(&str) -> f32` callback
- All layout tests updated to use the new signature

### Step 2 — RTL text support (D020)
- `TextDirection` enum: `Ltr`, `Rtl`, `Auto` (detect from first strong bidi char)
- `detect_direction(text: &str) -> TextDirection` — scan for Arabic/Hebrew Unicode ranges
- `TextStyle::direction` field (default `Auto`)
- `TextLayout::layout()` respects direction: RTL lines are right-aligned
- `word_wrap_rtl` — wraps right-to-left, reversing word order per line
- `PainterContext::draw_text_rtl` stub

### Step 3 — Text selection + OS clipboard (`rosace-clipboard`)
- New crate `rosace-clipboard`
- `ClipboardProvider` trait: `read() -> Option<String>`, `write(s: &str)`
- `SystemClipboard` — platform impl:
  - macOS: `pbcopy` / `pbpaste` via `std::process::Command`
  - Linux: `xclip` / `xsel` fallback
  - WASM: `web-sys` `navigator.clipboard` stub
- `TextSelection { anchor: TextCursor, focus: TextCursor }` in `rosace-text`
- `Selection::text(lines: &[String]) -> String` — extract selected substring
- Integration: `TextInput` widget in `rosace-widgets` gains `selection: Option<TextSelection>` and `on_copy` / `on_paste` callbacks

### Step 4 — WebSocket client (`rosace-ws`)
- New crate `rosace-ws`
- `WsMessage` enum: `Text(String)`, `Binary(Vec<u8>)`, `Ping`, `Pong`, `Close`
- `WsClient` — wraps `std::net::TcpStream` with a minimal WS handshake (RFC 6455)
  - `connect(url: &str) -> Result<WsClient, WsError>`
  - `send(msg: WsMessage) -> Result<(), WsError>`
  - `recv() -> Option<WsMessage>` (non-blocking via `set_nonblocking(true)`)
  - `close()`
- `WsError` enum: `Connect`, `Handshake`, `Send`, `Recv`, `Closed`
- WASM stub: `WsClient::connect` returns `Err(WsError::Connect("use web-sys WebSocket on WASM".into()))`
- `WsStream<T>` — wraps `WsClient` + `LoadState<T>`, calls `poll()` each frame

### Step 5 — Pinch gesture on WASM (`rosace-gesture` extension)
- Add `PinchRecognizer` to `rosace-gesture`
- Desktop: maps scroll-wheel delta to `GestureEvent::Pinch { scale, center_x, center_y }`
  - `InputEvent::Scroll { delta_y }` maps to scale = `1.0 + delta_y * 0.01`
- WASM: `TouchEvent` with 2 fingers → distance ratio between frames
- `PinchRecognizer { last_scale: f32, sensitivity: f32 }`
- `GestureEvent::Pinch` added (was deferred from Phase 6)

### Step 6 — Phase 7 showcase
- `rosace-examples/src/bin/phase7_demo.rs`
- 1400×900 PNG, 4 panels:
  1. Glyph metrics — side-by-side old vs new width estimates for sample text
  2. RTL text — "Hello World" vs "مرحبا بالعالم" layout direction visualization
  3. Clipboard + selection — TextInput with selection highlight and copy indicator
  4. WebSocket + Pinch — WsClient state machine diagram + PinchRecognizer scale UI

## Exit Criteria

- [ ] `measure_text("Hello", 14.0, &font)` returns fontdue-accurate width
- [ ] `word_wrap` with `measure_fn` breaks lines correctly for proportional fonts
- [ ] `detect_direction` returns `Rtl` for Arabic input
- [ ] `SystemClipboard::write` + `read` round-trips on macOS
- [ ] `TextSelection::text` extracts correct substring across lines
- [ ] `WsClient::connect` returns meaningful error on bad URL
- [ ] `WsStream::poll()` non-blocking, safe to call each frame
- [ ] `PinchRecognizer` emits scale > 1.0 on scroll-up, < 1.0 on scroll-down
- [ ] All workspace tests pass, zero warnings, clean release build

## Approved dependencies

- No external clipboard crate — use `std::process::Command` for pbcopy/xclip
- No `tungstenite` or `tokio-tungstenite` — hand-rolled WS handshake (learning exercise)
- No ICU / unicode-bidi crate — simple Unicode range scan for RTL detection
- fontdue already present — use `Font::metrics()` for glyph advance widths

## DO NOT

- DO NOT add full Unicode bidi algorithm (D020 says stub only) — Phase 8
- DO NOT add IME (Input Method Editor) support — Phase 8
- DO NOT add video/audio — Phase 8
- DO NOT swap tiny-skia for skia-safe yet — Phase 8 (D032)
- DO NOT add server-side WebSocket — client only
