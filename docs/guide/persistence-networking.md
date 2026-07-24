# Persistence & Networking

Most real apps need to remember things across restarts and talk to a server. ROSACE gives you both as small, atom-shaped APIs — no extra state-management layer to learn.

## Persistence: `ctx.state_permanent`

You already know `ctx.state` from [Components & State](components-and-state.md). `ctx.state_permanent` is the same hook, plus one thing: its value is written to disk and restored the next time the app launches.

```rust
struct Settings;

impl Component for Settings {
    fn build(&self, ctx: &mut Context) -> Element {
        let dark_mode = ctx.state_permanent("dark_mode", false);

        Scaffold::new(
            Row::new()
                .child(Text::new("Dark mode"))
                .child(Switch::new(dark_mode.get()).on_change({
                    let dark_mode = dark_mode.clone();
                    move |v| dark_mode.set(v)
                })),
        )
        .into_element()
    }
}
```

- `key` (`"dark_mode"` above) is the storage key — it's app-global, not per-component, so two components that pass the same key share one stored value.
- On first run (nothing stored yet, or the bytes don't decode) you get `default`. Every `set`/`update` after that writes through to disk.
- Everything else about it is a normal `Atom<T>` — `.get()`, `.set()`, `.update()`, subscription, all the same rules from the previous chapter (call it unconditionally, in a stable slot).

`T` must implement `PersistValue` — this is deliberately a small, closed set: `String`, `Vec<u8>`, `bool`, and the numeric primitives (`i32`, `i64`, `u32`, `u64`, `f32`, `f64`). There's no blanket `serde` impl (yet) — if you need to persist a struct, encode it yourself (e.g. to a `String` of JSON, or bytes) and decode on read.

### Where it's stored

`App::launch` opens a small embedded SQLite database in the platform's app-data directory and installs it as the process's persistence backend automatically — you don't configure this. If opening the store fails for some reason, persistence is disabled for that run and `state_permanent` quietly behaves like plain `ctx.state`: the app still works, values just don't survive a restart. That's a deliberate non-fatal fallback, not a silent bug — check your terminal for a `rosace: persistence disabled (...)` line if values aren't sticking.

**Web note**: on `wasm32`, the on-disk store isn't implemented yet (SQLite's native code doesn't compile to wasm) — `state_permanent` on web behaves like `ctx.state` for now. This is a named gap, not a crash.

### Direct key-value access

If you have data that isn't naturally atom-shaped — a cached blob, a downloaded file — reach for the store directly instead of shoehorning it into an atom:

```rust
use rosace::storage::Storage;

let store = Storage::open(my_path)?;
store.set("last_sync", b"2026-07-23")?;
let value = store.get("last_sync")?; // Option<Vec<u8>>
store.delete("last_sync")?;
```

This is the same store `state_permanent` writes through — it's `get`/`set`/`delete` on string keys and byte values, deliberately not a general query/ORM surface.

## Networking

### Fetching data: `use_query`

For "load this URL and show it," `use_query` is the one-line answer:

```rust
use rosace::net::{use_query, QueryState};

struct Profile;

impl Component for Profile {
    fn build(&self, ctx: &mut Context) -> Element {
        let state = use_query(ctx, "https://api.example.com/me");

        let body = match state {
            QueryState::Idle | QueryState::Loading => Text::new("Loading…"),
            QueryState::Loaded(resp) if resp.is_success() => Text::new(resp.text()),
            QueryState::Loaded(resp) => Text::new(format!("Server error: {}", resp.status)),
            QueryState::Failed(e) => Text::new(format!("Network error: {e}")),
        };

        Scaffold::new(Column::new().child(body)).into_element()
    }
}
```

Call it from `build()` like any other hook. The request runs on a background thread; when it completes, the result is written straight into the component's state and the component re-renders — there's no per-frame polling to write yourself. Change the URL and a fresh request starts automatically; the old one's result is discarded if it lands late. Unmounting the component cancels the write-back too (the connection itself still runs to completion or timeout in the background, but nothing stale gets written into a dead component).

### The HTTP client directly

`use_query` is GET-only and fire-and-forget. For POST/PUT/DELETE, custom headers, or calling from outside `build()`, use `HttpClient`:

```rust
use rosace::net::{HttpClient, HttpRequest, HttpMethod};

// Blocking — call this from a background thread, not the UI thread.
let client = HttpClient::new();
let resp = client.post("https://api.example.com/items", b"{\"name\":\"tea\"}".to_vec())?;
if resp.is_success() {
    println!("{}", resp.text());
}

// Non-blocking: runs on its own thread, poll once per frame (same shape as
// image loading below).
let mut handle = client.fetch(HttpRequest::new(HttpMethod::Get, "https://api.example.com/items"));
// each frame / tick:
if let Some(result) = handle.poll() {
    // Ok(HttpResponse) or Err(String) — transport failure only; a 404 is
    // still Ok with status 404, not an Err.
}
```

A non-2xx response is *not* an error at this layer — the request reached the server and got an answer, so it comes back as `Ok(HttpResponse)` with `status` set; check `resp.is_success()`. `Err` is reserved for transport failures (DNS, TLS, timeout, connection refused).

**Web note**: HTTP isn't implemented on `wasm32` yet — every call returns a clear `Err` instead of panicking. A browser-`fetch()`-backed client is future work.

### Remote images

`RemoteImage` + `ImageLoader` cover the "show a picture from a URL" case:

```rust
use rosace::net::{RemoteImage, ImageLoader};

let img = RemoteImage::new("https://example.com/avatar.png").width(96.0).height(96.0);
img.register(&mut loader);   // starts loading if not already
match img.state(&loader) {
    LoadState::Loading => { /* spinner */ }
    LoadState::Loaded(bytes) => { /* decode + draw */ }
    LoadState::Failed(e) => { /* placeholder */ }
    LoadState::Idle => {}
}
```

`loader.poll()` once per frame drains completed loads into `LoadState`.

### Network status

`use_network_status(ctx)` gives you app-wide connectivity as a reactive read — `Online`, `Offline`, or `Unknown` (before the first check completes):

```rust
use rosace::net::{use_network_status, NetworkStatus};

let status = use_network_status(ctx);
if status == NetworkStatus::Offline {
    // show an offline banner
}
```

On desktop, the first call starts a lightweight background prober (a periodic TCP connect attempt to two independent well-known hosts — reachability of either means Online). On mobile, a native host can report connectivity directly via the same channel, which suppresses the desktop-style prober. On web, status stays `Unknown` for now (no browser `navigator.onLine` backend yet).

---

**Under the hood:** the on-disk store, the persist-tier seams `#[persist]` is built on, and the platform-specific networking backends are covered in the architecture book — see `../architecture/core.md`.

Next: [Multi-Platform & the rsc CLI](multi-platform.md).
