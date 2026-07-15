# Phase 31 — Real Persistence: `#[persist]` Backed by `rusqlite` (D114)

> Status: Steps 1-2 LANDED + live-verified. Step 3 (encrypted) WAITS
> by its own rule — Phase 29 landed without a secure-storage capability,
> and this phase forbids a plaintext fallback for data marked encrypted;
> it unblocks when the Keychain/Keystore capability is added to the
> D106 bridge (same shape as camera/lifecycle/push).
> Started: 2026-07-15
> Completed: 2026-07-15 (Steps 1-2; Step 3 gated as scoped)
> Decision: **D114** — implement `D008`'s `#[persist(reload/session/
> permanent/encrypted)]` for real. `reload`/`session` stay in-process;
> `permanent` writes to embedded `rusqlite`; `encrypted` (secure
> storage) is deferred to Phase 29's FFI capability bridge, not solved
> here.

## Why This Phase

`D008` (`#[persist(reload)]`/`#[persist(session)]`/`#[persist(permanent)]`/`#[persist(permanent, encrypted)]`) has been LOCKED since early project planning. Grepped `rosace-state` and `rosace-macros` for `persist`/`#[persist]` — zero implementation anywhere. State does not survive an app restart today, at all. A framework positioned to ship real apps needs this — login sessions, cached API responses (feeding off D113/Phase 30's new HTTP client), user preferences all need it.

## Out of Scope (deliberately, not silently dropped)

- **`encrypted` tier's actual implementation.** Secure storage isn't a Rust-crate problem — it needs the platform Keychain (iOS)/Keystore (Android), reachable only through the D106 FFI bridge. Scoped as an addition to Phase 29's capability list (same three-piece shape as camera/lifecycle/push), not duplicated here. This phase defines the `#[persist(permanent, encrypted)]` *syntax* and routes it to that capability once Phase 29 has it; it does not implement the native side.
- **Cross-device sync (iCloud/Google-account-linked sync).** Real feature, real separate scope — local persistence first.
- **Migrations/schema versioning for `permanent` storage.** Needed eventually for any real app with evolving data shapes, but premature before the basic tier exists and has real usage to learn from.
- **A general query/ORM layer over the SQLite store.** `#[persist]` atoms are simple key→serialized-value pairs, not a relational data model apps query directly. If an app wants real relational local data (not just persisted atoms), that's a separate, bigger feature — not assumed needed by this phase.

## Steps

### Step 1 — Decide the crate boundary + add `rusqlite`
**Wasm constraint (added 2026-07-10)**: `rusqlite` links C SQLite and does not build on `wasm32-unknown-unknown` — the dependency must be target-gated so the SDK keeps compiling for wasm, and the web story for the `permanent` tier (localStorage/IndexedDB via `web-sys`) is explicitly named-deferred: decided here at Step 1 as either a real web backend or a documented gap, never a silently broken wasm build.

New thin crate `rosace-storage` (Layer 5) rather than pulling a SQL dependency directly into `rosace-state` — keeps `rosace-state`'s existing dependency footprint (`trace` only) unchanged for apps that never persist anything. `rosace-storage` exports a minimal key-value-over-SQLite store (`get`/`set`/`delete` by string key, serialized value as bytes) — not a general query API, per Out of Scope.

Exit: a standalone test writes and reads back a value through `rosace-storage`'s API against a real on-disk SQLite file, confirms it survives closing and reopening the connection (proving real persistence, not just in-memory).

**Landed 2026-07-15.** New crate `rosace-storage` (deps: trace only +
target-gated `rusqlite 0.32` with `bundled` — same SQLite version on
every platform, no system-lib drift; iOS/Android link it directly, no
Swift/Kotlin layer, same principle as D113's sockets-direct networking).
`Storage::open(path)` (creates `kv(key TEXT PRIMARY KEY, value BLOB)`),
`get`/`set` (upsert)/`delete`, `&self` methods over an internal
`Mutex<Connection>` so any thread can write. Exit bar met by
`value_survives_close_and_reopen` (real on-disk file, connection fully
dropped between write and read) + round-trip/overwrite/binary tests.
**Wasm story resolved as the named-gap option** (D113's convention):
compiles on wasm32 (verified), every call returns a documented `Err`;
localStorage/IndexedDB is the tracked future backend.

### Step 2 — Wire `#[persist(permanent)]` for real
The `#[persist]` macro (`rosace-macros`) generates code that reads from `rosace-storage` on atom initialization and writes on every change, for atoms marked `permanent`. `reload`/`session` tiers stay in-process (an in-memory map surviving hot-reload/backgrounding, not hitting `rosace-storage` at all — cheaper, and correct per D008's own tiering: only `permanent` needs to survive a full process restart).

Exit: a real running app sets a `#[persist(permanent)]` atom's value, the app is fully quit and relaunched, and the value is observably restored — verified live, not just unit-tested.

**Landed 2026-07-15 (via D121 — read it first: persist re-homed onto
the HOOK model, not D008's field-attribute world, which has zero real
call sites).** `rosace-core/src/persist.rs`: `PersistBackend` trait +
first-install-wins global slot (`set_persist_backend`) so core/state
stay SQLite-free, and `PersistValue` (bytes round-trip; primitives +
`String` + `Vec<u8>`; stale bytes decode to `None` → default, never
panic; serde blanket impl = named deferral). `Context::state_permanent
(key, default)`: first init reads the backend (key absent/stale →
default), every later `set` writes through via `Atom::set_on_change`
(made `pub`; the slot was test-only — a persistent atom's slot is now
CLAIMED, documented). Keys are app-global by design (storage keys, not
hook slots). No backend installed = plain `ctx.state` (headless tests
unaffected). `App::launch` installs `rosace_storage::Storage` at the
platform app-data dir (`persist_db_path`: macOS Application Support /
`%APPDATA%` / XDG / iOS sandbox Documents; Android's files-dir needs
the JNI host — named deferral alongside Known Issue #16). Open failure
is NON-fatal (warning; app runs unpersisted). **Mobile entry wired
2026-07-15 (user question caught the gap)**: mobile apps enter via
`Engine::init` (D106 FFI), NOT `App::launch` — the backend now installs
there too for iOS (`$HOME/Documents/rosace.sqlite`; the sandbox
container IS the per-app namespace, and rides device backups); Android
waits on a `getFilesDir()` path through `nativeInit`, deferred with
#16. Live iOS verification folds into the next simulator session. `reload`/`session` tiers:
documented no-ops by construction (D121). Live exit bar: `persist_demo`
run three times with full process quits — screenshots show Launch #1 →
#2 → #3 and a note string written by a previous process restored, real
file at `~/Library/Application Support/Persist_Demo/rosace.sqlite`.

### Step 3 — `#[persist(permanent, encrypted)]` routes to the Phase 29 capability
Once Phase 29's FFI bridge has a secure-storage capability (Keychain/Keystore), wire the `encrypted` tier to call it instead of `rosace-storage`. If Phase 29 hasn't landed yet when this step is reached, this step waits — no plaintext fallback silently used for data marked `encrypted` (a real footgun: better to fail loudly/not compile than silently store secrets in plaintext SQLite).

Exit: a real running app stores a value via `#[persist(permanent, encrypted)]`, and it's confirmed (via the OS's own Keychain/Keystore inspection tool, not just "the app still shows it") to be in the platform secure store, not `rosace-storage`'s plain SQLite file.

## Sequencing

Step 1→2 sequential. Step 3 depends on Phase 29's secure-storage capability existing — if Phase 29 is sequenced after this phase, Step 3 slips until it's available; Steps 1-2 are independently valuable and don't need to wait.

## Migration Rule

Purely additive — `#[no_persist]` (D008's explicit opt-out) remains the default-equivalent behavior for atoms that don't declare a persistence tier; no existing atom's behavior changes.
