# Adapting a Rust Library

ROSACE doesn't wrap every crate in the ecosystem — nor should it. Most pure-Rust libraries need *zero* adapter work; a smaller set (anything blocking or CPU-heavy) need one small, well-understood pattern to become reactive. This chapter covers both, plus a complete real-world example.

## First question: does it even need an adapter?

If the library's function is fast (microseconds, not milliseconds — string parsing, hashing a short value, formatting, most pure computation) and doesn't block on I/O, **just call it directly inside `build()`**. No hook, no atom, no thread:

```rust
use rosace::prelude::*;

struct SlugPreview;

impl Component for SlugPreview {
    fn build(&self, ctx: &mut Context) -> Element {
        let title = ctx.state(String::new());
        // `slug::slugify` is a plain, fast, pure function — call it straight.
        let slug = slug::slugify(title.get());

        Scaffold::new(
            Column::new()
                .child(TextInput::new().value(title.get()).on_change({
                    let t = title.clone();
                    move |v| t.set(v)
                }))
                .child(Text::new(format!("URL: /posts/{slug}"))),
        )
        .into_element()
    }
}
```

`build()` re-runs on every relevant state change anyway, so a cheap pure function just becomes part of that computation. This covers the large majority of "how do I use crate X with ROSACE" questions — most crates are exactly this simple.

## When you DO need an adapter

If the library's call is **blocking** (file I/O, network, a decoder/encoder that takes real time on non-trivial input — image processing, video, compression of large data, cryptographic key derivation), calling it directly inside `build()` would freeze the UI for however long it takes. The fix is the same three-piece pattern every I/O-shaped thing in ROSACE already uses (`use_query` for HTTP, the Platform Channel bridge for native calls): **spawn a background thread, write the result into an `Atom` on completion, expose a hook that reads it.**

```
your blocking call ──spawn──▶ background thread ──when done──▶ Atom::set() ──▶ component re-renders
```

`Atom::set()` is thread-safe and automatically marks the subscribed component dirty and wakes the frame loop — that's the entire "reactivity" story. You never write a poll loop.

## Worked example: thumbnailing an image with the `image` crate

`image` is a real, widely-used pure-Rust crate with a synchronous, blocking API — decoding and resizing a non-trivial photo can take tens of milliseconds, long enough to visibly stutter a frame if called directly. This is exactly the shape that needs the adapter pattern.

```rust
//! thumbnail.rs — a `use_query`-shaped adapter around the `image` crate.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rosace::core::Context;
use rosace_state::Atom;

#[derive(Debug, Clone, PartialEq)]
pub enum ThumbnailState {
    Idle,
    Loading,
    /// Pre-multiplied RGBA bytes + dimensions — ready to hand to an image
    /// widget however your app draws raw pixels.
    Ready { rgba: Arc<Vec<u8>>, width: u32, height: u32 },
    Failed(String),
}

/// Load `path`, resize to fit `max_size`, and track the result through
/// `ThumbnailState`. Call from `build()` — mirrors `rosace_net::use_query`
/// exactly: fetch starts once per distinct `path`, unmount/path-change
/// discards a late result instead of writing into a dead or stale slot.
pub fn use_thumbnail(ctx: &mut Context, path: impl Into<PathBuf>, max_size: u32) -> ThumbnailState {
    let path = path.into();
    let state = ctx.state(ThumbnailState::Idle);
    let active_path = ctx.state(PathBuf::new());
    let alive = ctx.state(Arc::new(AtomicBool::new(true)));

    let alive_for_cleanup = alive.get();
    rosace::core::lifecycle::on_unmount(ctx, move || {
        alive_for_cleanup.store(false, Ordering::SeqCst);
    });

    if active_path.get() != path {
        active_path.set(path.clone());
        state.set(ThumbnailState::Loading);

        let state = state.clone();
        let active_path_check = active_path.clone();
        let alive = alive.get();
        let thread_path = path.clone();

        std::thread::spawn(move || {
            let result = image::open(&thread_path)
                .map(|img| img.thumbnail(max_size, max_size).to_rgba8());

            // Discard if the component unmounted or moved to a different
            // path while we were decoding — the same guard use_query uses.
            if !alive.load(Ordering::SeqCst) || active_path_check.get() != thread_path {
                return;
            }

            match result {
                Ok(buf) => {
                    let (width, height) = buf.dimensions();
                    state.set(ThumbnailState::Ready { rgba: Arc::new(buf.into_raw()), width, height });
                }
                Err(e) => state.set(ThumbnailState::Failed(e.to_string())),
            }
        });

        return ThumbnailState::Loading;
    }

    state.get()
}
```

Using it looks exactly like `use_query`:

```rust
impl Component for PhotoCard {
    fn build(&self, ctx: &mut Context) -> Element {
        match use_thumbnail(ctx, "/path/to/photo.jpg", 200) {
            ThumbnailState::Idle | ThumbnailState::Loading => Text::new("Loading…").into_element(),
            ThumbnailState::Ready { rgba, width, height } => {
                RawImage::from_rgba(rgba, width, height).into_element()
            }
            ThumbnailState::Failed(e) => Text::new(format!("Couldn't load: {e}")).into_element(),
        }
    }
}
```

## The recipe, generalized

Whatever library you're adapting, the shape is always the same four pieces:

1. **A state enum** naming every outcome (`Idle`/`Loading`/`Ready(T)`/`Failed(String)` — add more variants if the library has more distinct outcomes worth showing differently).
2. **A guard atom** tracking *what's currently being computed* (a path, a URL, an argument struct — anything `PartialEq`) so a changed input restarts the work and a stale result never overwrites a newer one.
3. **An `alive` flag**, flipped by `on_unmount`, checked before the background thread writes — so a slow result landing after the component is gone is silently discarded instead of corrupting unrelated state.
4. **A hook function** (`use_whatever(ctx, ...) -> WhateverState`) that ties the three together — this is the only thing calling code ever sees.

If the library already exposes a non-blocking/polling API of its own (some crates do), you don't need the background thread at all — just store its handle in `ctx.state` and poll it once per frame the same way `rosace-net`'s `HttpClient::fetch` handle works (see [Persistence & Networking](persistence-networking.md)).

## When the library needs a *native* API, not just time

If what you're wrapping isn't a Rust crate at all — a platform SDK, a native permission, anything that only exists as a Swift/Kotlin API — that's not a library-adapter problem, it's a [Platform Channel](platform-channel.md) problem: the same `invoke_method`/`Atom<ChannelCallState>` shape, but the "background worker" is native code across the FFI boundary instead of a Rust thread.

---

Next: [Multi-Platform & the rsc CLI](multi-platform.md).
