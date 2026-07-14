# Phase 30 — Real Networking: Sync HTTP/WebSocket, Not Hand-Rolled TLS (D113)

> Status: Steps 1-2 LANDED + live-verified. Steps 3-4 not started.
> Started: 2026-07-14
> Completed: —
> Decision: **D113** — `rosace-net` gains a general HTTP client on
> `ureq` (sync, `rustls`-based); `rosace-ws` moves from a hand-rolled
> RFC 6455 handshake to `tungstenite` (sync). `D012`'s decided-but-
> never-built hooks (`use_query`/`use_websocket`/`use_network_status`)
> get real implementations on top.

## Why This Phase

`rosace-net`'s `Cargo.toml` has zero HTTP dependencies today — it only does non-blocking remote *image* loading. `rosace-ws`'s own doc comment says "hand-rolled RFC 6455 handshake over `std::net::TcpStream` ... no `tungstenite` dep." Both trace back to an explicit Phase 1/6/7-era stance: *"No reqwest ... No tungstenite ... hand-rolled WS handshake (learning exercise)."* That was a reasonable minimal-dependency choice for early bring-up. It is the wrong choice for a shipped framework — and the actual state (verified 2026-07-10) is starker than "hand-rolled TLS": `rosace-net`'s `parse_url` **rejects `https://` URLs outright** (unit test `parse_url_rejects_https`, `http.rs`). There is no TLS at all today, hand-rolled or otherwise — a remote image on an https URL cannot load, and virtually every real 2026 endpoint is https-only. The options are hand-rolling TLS (a real security liability, not evidence of framework quality — the same "avoid a shortcut that's actually a shortcut on something that matters" reasoning as D109's scope-discipline note and [[feedback_no_mvp_shortcuts]]) or adopting `rustls` via `ureq`; this phase picks the latter. `rosace-ws`'s handshake IS genuinely hand-rolled, per its own doc comment.

Separately, `D012` (LOCKED at original scoping) decided a whole external-data model — `Stream<T>` as a universal bridge, with `use_websocket`/`use_query`/`use_file_watch`/`use_network_status`/`use_app_lifecycle` as built-in typed adapters — but grepping the workspace found zero implementation of any of them (this phase covers the network-shaped ones; `use_app_lifecycle` is D110/Phase 29's job, `use_file_watch` is out of scope here).

## Wasm Constraint (added 2026-07-10 — resolve at Step 1, not at web build breakage)

The whole SDK compiles for wasm today (a landed, defended property of the
multiplatform work). `ureq` and `tungstenite` are `std::net`-based —
non-functional on `wasm32-unknown-unknown` — and `rosace-net`'s existing
`std::thread` + `mpsc` loader pattern is equally non-functional there (a
latent pre-existing gap, since `RemoteImage` was desktop-proven only).
Step 1 must target-gate the native implementation and explicitly decide
the web story: either real `fetch()`/browser-`WebSocket` backends via
`web-sys` behind the same hook API, or a named, documented "web
networking not yet supported" gap. What is NOT acceptable is silently
breaking `cargo build --target wasm32-unknown-unknown` or compiling code
that panics at runtime on web without a documented decision.

## Out of Scope (deliberately, not silently dropped)

- **GraphQL / gRPC clients.** REST-shaped HTTP + WebSocket cover the common case. A GraphQL-specific client is real, separate follow-up work if a real need surfaces.
- **`use_file_watch`, `use_sensor`.** Named in D012 but not network-shaped — `use_sensor` is an FFI-capability candidate (D106's pattern), `use_file_watch` is a filesystem concern. Neither belongs in a networking phase.
- **Offline request queueing / retry-with-backoff as a built-in policy.** Real apps need this, but it's a design surface of its own (what counts as retryable, how long to queue, interaction with D114/Phase 31's persistence for the queue itself) — scope after the basic client is real and used.
- **HTTP/2, connection pooling tuning.** `ureq` handles the common case; deep tuning is premature before real usage patterns exist.

## Steps

### Step 1 — `ureq`-backed HTTP client in `rosace-net`
Generalize `rosace-net` beyond image loading: a `HttpClient` (or similar) that does GET/POST/PUT/DELETE with headers/body, still non-blocking via the existing `std::thread`/`mpsc` pattern the crate already uses for image loads — `ureq`'s blocking calls run inside that same background-thread wrapper, no architecture change, just a real HTTP implementation underneath instead of none. `RemoteImage`'s existing loader can be rebuilt on top of this same client (dogfooding, not a parallel implementation).

Exit: a real running app fetches real JSON from a real HTTP endpoint (including HTTPS) and renders it — verified live, not mocked.

**Landed 2026-07-14.** `rosace-net/src/client.rs`: `HttpClient` (Clone —
shared connection-pooling `ureq::Agent`, 30s timeout) with blocking
`get/post/put/delete/send` (documented background-thread contract) +
non-blocking `fetch() -> HttpHandle` (thread + mpsc, `poll()` once per
frame — the `ImageLoader` shape, and the seam Step 2's `use_query`
builds on). `HttpRequest` builder (headers/body), `HttpResponse`
(status/headers/body + `text()`/`is_success()`); non-2xx is a RESPONSE
(status carried), `Err` is transport-only. `ureq = "2"` target-gated to
non-wasm. **Wasm story resolved as the named-gap option**: wasm builds
compile (verified `cargo check --target wasm32-unknown-unknown`), every
request returns a documented `Err` (no panic — including `fetch`, whose
wasm variant avoids `std::thread::spawn`, which PANICS at runtime on
wasm; the browser-`fetch()` backend remains future work, this
paragraph is the tracking record). The Phase-6 hand-rolled HTTP/1.0
`http.rs` (the one that rejected https) is DELETED; `ImageLoader::load`
now runs on the shared client (dogfooding — non-2xx maps to
`LoadState::Failed("HTTP <code>")`, which the old status-line sniffing
never did properly). `RemoteImage`'s public API untouched (Migration
Rule held). Tests: builder/response unit tests, connection-refused
transport error, plus `https_get_fetches_real_json`
(`#[ignore = "hits the real network"]`, run explicitly — passed).
Live exit bar: `http_demo` (new example bin) fetched
`https://httpbin.org/json` in a running app — screenshot shows HTTP 200
+ the rendered JSON body.

### Step 2 — `use_query` hook
The `Stream<T>`-bridge shape D012 decided: `use_query(url) -> QueryState<T>` (Idle/Loading/Loaded(T)/Failed) built on Step 1's client, auto-cleanup on unmount (D012's stated rule — "all connections auto-cleaned").

Exit: a real running app's screen shows a loading state, then real fetched data, and the request is provably cancelled/cleaned up when the screen is popped (not just "looks fine") — verified via a real test that checks the connection/thread is actually gone, not just that the UI stopped rendering it.

**Landed 2026-07-14.** `rosace-net/src/query.rs`: `use_query(ctx, url) ->
QueryState` (D012's Idle/Loading/Loaded/Failed; `Loaded` carries the
whole `HttpResponse` — non-2xx is Loaded-with-status, `Failed` is
transport-only, matching Step 1's client semantics). NO per-frame
polling: the worker thread writes the state atom directly on completion
(`Atom::set` cross-thread marks the subscribed component dirty — the
app-lifecycle watcher pattern), so clean frames stay clean mid-flight.
Auto-cleanup (D012's rule) via a shared `alive: Arc<AtomicBool>` flipped
by `on_unmount`: the worker checks it (AND that the component still
wants this URL — a changed URL can't be overwritten by a stale slow
response) before writing, then terminates, dropping its connection.
Disclosed limit: sync `ureq` can't abort a blocking read mid-flight, so
an in-flight request runs to completion or the 30s timeout before the
thread exits — bounded, never leaked. Exit bar's hard half proven by
`unmount_discards_a_late_response_and_the_connection_is_actually_gone`:
a local hold-the-response server, unmount fires (`cleanup_store::
fire_and_clear`), THEN the server responds — the test asserts the
worker's connection actually closes (server reads EOF) and the state
atom was never written. Live half: `http_demo` rewritten onto
`use_query` (dogfood), screenshot shows loading→HTTP 200 + rendered
JSON over HTTPS. wasm: `use_query` short-circuits to `Failed(named-gap
message)` without spawning (`std::thread::spawn` panics on wasm).

### Step 3 — `tungstenite`-backed `rosace-ws` + `use_websocket` hook
Swap the hand-rolled handshake for `tungstenite` (sync crate, no tokio). `use_websocket(url) -> WsState` hook on top, matching `use_query`'s shape.

Exit: a real running app maintains a live WebSocket connection to a real server, receives and displays real messages, verified live.

### Step 4 — `use_network_status` hook
Platform connectivity detection (desktop: attempt-based/OS API; mobile: real capability via the D106 FFI bridge, same shape as camera/lifecycle).

Exit: a real running app observably reacts to the network being disabled/re-enabled on a real device or OS-level toggle.

## Sequencing

Step 1 is the foundation everything else needs. Steps 2 and 4 depend only on Step 1. Step 3 is independent of Steps 2/4 (different transport) but depends on Step 1 existing as the pattern to follow, not its code.

## Migration Rule

`RemoteImage`'s existing public API is unchanged — Step 1 only replaces what's underneath it. No app using image loading today needs to change anything.
