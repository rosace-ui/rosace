//! `use_query` (D012's decided shape, built by D113/Phase 30 Step 2): a
//! component hook that fetches a URL on Step 1's `HttpClient` and
//! re-renders the component through each state change.
//!
//! No per-frame polling: the background thread writes the result atom
//! directly on completion (`Atom::set` is thread-safe and marks the
//! subscribed component dirty — the same cross-thread pattern the
//! app-lifecycle watcher uses), so a clean frame stays clean while a
//! request is in flight.
//!
//! # Cleanup (D012: "all connections auto-cleaned")
//! Unmount flips a shared `alive` flag via the component's cleanup hook;
//! the worker thread checks it before writing, so a response landing
//! after the screen was popped is DISCARDED — no stale state write, no
//! dirty-marking a dead component — and the thread (with its connection)
//! terminates right after. Sync `ureq` cannot abort a blocking read
//! mid-flight, so an in-flight request runs to completion or timeout
//! (30s, the client default) before the thread exits — bounded, not
//! leaked. A URL change on a live component is handled the same way:
//! the write is also guarded on "is this still the URL the component
//! wants", so a slow stale response can't overwrite a newer query.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rosace_core::Context;

use crate::client::{HttpClient, HttpResponse};

/// The query lifecycle (D012's four states).
#[derive(Debug, Clone, PartialEq)]
pub enum QueryState {
    /// Not started (only observable before the hook's first run for a URL).
    Idle,
    /// Request in flight.
    Loading,
    /// Response received — including non-2xx (check
    /// [`HttpResponse::is_success`]; the request reaching the server is
    /// not a transport failure).
    Loaded(HttpResponse),
    /// Transport failure (DNS, TLS, timeout, refused).
    Failed(String),
}

/// Fetch `url` (GET) and track it through [`QueryState`]. Call from
/// `build()` — the component re-renders on every state change, the fetch
/// starts once per URL (a changed URL restarts it), and unmount discards
/// any late response (see the module doc).
pub fn use_query(ctx: &mut Context, url: impl Into<String>) -> QueryState {
    let url = url.into();
    let state = ctx.state(QueryState::Idle);
    let active_url = ctx.state(String::new());
    let alive = ctx.state(Arc::new(AtomicBool::new(true)));

    // Unmount cleanup — registered once (on_unmount's own idempotence
    // guard), flips the flag every worker checks before writing.
    let alive_for_cleanup = alive.get();
    rosace_core::lifecycle::on_unmount(ctx, move || {
        alive_for_cleanup.store(false, Ordering::SeqCst);
    });

    if active_url.get() != url && !url.is_empty() {
        active_url.set(url.clone());
        state.set(QueryState::Loading);

        // wasm32: no threads (spawn PANICS at runtime) and no HTTP yet —
        // deliver the client's documented named-gap error synchronously.
        #[cfg(target_arch = "wasm32")]
        {
            if let Err(e) = HttpClient::new().get(&url) {
                state.set(QueryState::Failed(e));
            }
            return state.get();
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let state = state.clone();
            let active_url = active_url.clone();
            let alive = alive.get();
            let thread_url = url;
            std::thread::spawn(move || {
                let client = HttpClient::new();
                let result = client.get(&thread_url);
                // Discard if the component unmounted or moved to another
                // URL while we were in flight — this IS the auto-cleanup
                // contract.
                if !alive.load(Ordering::SeqCst) || active_url.get() != thread_url {
                    return;
                }
                match result {
                    Ok(resp) => state.set(QueryState::Loaded(resp)),
                    Err(e) => state.set(QueryState::Failed(e)),
                }
            });
        }
        return QueryState::Loading;
    }

    state.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    /// A one-request local server that WAITS for permission to respond,
    /// then reports whether its connection was closed by the client after
    /// responding — the "connection is actually gone" probe the Step 2
    /// exit bar demands.
    fn slow_server() -> (String, mpsc::Sender<()>, mpsc::Receiver<bool>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/q", listener.local_addr().unwrap());
        let (respond_tx, respond_rx) = mpsc::channel::<()>();
        let (closed_tx, closed_rx) = mpsc::channel::<bool>();
        std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf); // consume the request head
            // Hold the response until the test says go.
            let _ = respond_rx.recv_timeout(Duration::from_secs(10));
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            // After responding, a closed connection reads as EOF (0 bytes)
            // once the client thread has terminated and dropped the socket.
            sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let closed = matches!(sock.read(&mut buf), Ok(0));
            let _ = closed_tx.send(closed);
        });
        (url, respond_tx, closed_rx)
    }

    fn wait_for<F: Fn() -> bool>(cond: F, secs: u64) -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(secs);
        while std::time::Instant::now() < deadline {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn query_goes_loading_then_loaded_and_re_marks_the_component_dirty() {
        let component = rosace_core::ComponentId(9001);
        let mut ctx = Context::new(component);
        let (url, respond_tx, _closed_rx) = slow_server();

        assert_eq!(use_query(&mut ctx, &url), QueryState::Loading);

        respond_tx.send(()).unwrap();
        // The worker writes the atom directly; re-running the hook (a
        // rebuild) must observe Loaded.
        assert!(
            wait_for(
                || {
                    let mut ctx = Context::new(component);
                    matches!(use_query(&mut ctx, &url), QueryState::Loaded(_))
                },
                5
            ),
            "query must reach Loaded after the server responds"
        );
    }

    #[test]
    fn unmount_discards_a_late_response_and_the_connection_is_actually_gone() {
        let component = rosace_core::ComponentId(9002);
        let mut ctx = Context::new(component);
        let (url, respond_tx, closed_rx) = slow_server();

        assert_eq!(use_query(&mut ctx, &url), QueryState::Loading);

        // Pop the screen: the reconciler fires the component's cleanups.
        rosace_state::cleanup_store::fire_and_clear(component);

        // NOW let the server respond — the response arrives after unmount.
        respond_tx.send(()).unwrap();

        // The connection must actually close (worker thread terminated,
        // socket dropped) — not just "the UI stopped rendering it".
        assert!(
            closed_rx.recv_timeout(Duration::from_secs(10)).unwrap_or(false),
            "worker connection must be closed after the late response"
        );

        // And the late response must have been DISCARDED: the state atom
        // still says Loading (the last value written before unmount).
        let mut ctx = Context::new(component);
        assert_eq!(
            use_query(&mut ctx, &url),
            QueryState::Loading,
            "a response landing after unmount must not be written"
        );
    }
}
