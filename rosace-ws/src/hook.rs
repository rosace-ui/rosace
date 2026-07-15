//! `use_websocket` (D012's decided shape, built by D113/Phase 30 Step 3)
//! — a component hook maintaining one live WebSocket connection, matching
//! `use_query`'s design: NO per-frame polling (the worker thread writes
//! the state atom, marking the subscribed component dirty cross-thread),
//! auto-cleanup on unmount via the shared `alive` flag (the worker closes
//! the connection and exits within one poll tick — 30ms).
//!
//! wasm32: `connect` reports the client's documented named-gap error as
//! `WsState::Closed(..)` without spawning a thread (see `PHASE_30.md`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use rosace_core::Context;

use crate::client::WsClient;
use crate::message::WsMessage;

/// How many received messages the hook retains (newest last) — a display
/// buffer, not a durable log; an app that must keep every message should
/// drain them into its own state as they arrive.
const MESSAGE_BUFFER: usize = 50;

/// The connection lifecycle, `use_query`-shaped.
#[derive(Debug, Clone, PartialEq)]
pub enum WsState {
    /// Handshake in progress on the worker thread.
    Connecting,
    /// Live; `messages` holds the most recent [`MESSAGE_BUFFER`] received
    /// messages, newest last.
    Open { messages: Vec<WsMessage> },
    /// Gone — connect failure, receive error, server close, or unmount.
    Closed(String),
}

/// What [`use_websocket`] returns: the current state plus a sender for
/// outgoing messages (clonable, thread-safe — usable from `on_press`
/// closures).
#[derive(Clone)]
pub struct WsHandle {
    pub state: WsState,
    sender: Option<Sender<WsMessage>>,
}

impl WsHandle {
    /// Queue a message for the worker to send on its next tick. Returns
    /// `false` if the connection is gone (message dropped).
    pub fn send(&self, msg: WsMessage) -> bool {
        match &self.sender {
            Some(tx) => tx.send(msg).is_ok(),
            None => false,
        }
    }
}

/// Maintain a WebSocket connection to `url` from a component. Connects
/// once per URL, re-renders the component on every state change, and
/// closes the connection on unmount (D012: "all connections auto-cleaned").
pub fn use_websocket(ctx: &mut Context, url: impl Into<String>) -> WsHandle {
    let url = url.into();
    let state = ctx.state(WsState::Connecting);
    let active_url = ctx.state(String::new());
    let alive = ctx.state(Arc::new(AtomicBool::new(true)));
    let sender_slot = ctx.state(None::<Sender<WsMessage>>);

    let alive_for_cleanup = alive.get();
    rosace_core::lifecycle::on_unmount(ctx, move || {
        alive_for_cleanup.store(false, Ordering::SeqCst);
    });

    if active_url.get() != url && !url.is_empty() {
        active_url.set(url.clone());
        state.set(WsState::Connecting);

        #[cfg(target_arch = "wasm32")]
        {
            // No threads on wasm — surface the client's named-gap error.
            let err = WsClient::connect(&url).err().map(|e| e.to_string());
            state.set(WsState::Closed(err.unwrap_or_default()));
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel::<WsMessage>();
            sender_slot.set(Some(tx));

            let state = state.clone();
            let alive = alive.get();
            let thread_url = url;
            std::thread::spawn(move || {
                let mut client = match WsClient::connect(&thread_url) {
                    Ok(c) => c,
                    Err(e) => {
                        if alive.load(Ordering::SeqCst) {
                            state.set(WsState::Closed(e.to_string()));
                        }
                        return;
                    }
                };
                if alive.load(Ordering::SeqCst) {
                    state.set(WsState::Open { messages: Vec::new() });
                }
                let mut messages: Vec<WsMessage> = Vec::new();
                loop {
                    if !alive.load(Ordering::SeqCst) {
                        client.close(); // the auto-cleanup contract
                        return;
                    }
                    // Outgoing queue → wire.
                    while let Ok(out) = rx.try_recv() {
                        if let Err(e) = client.send(out) {
                            state.set(WsState::Closed(e.to_string()));
                            return;
                        }
                    }
                    // Wire → state atom (dirty-marks the component).
                    match client.recv() {
                        Some(Ok(WsMessage::Close(code))) => {
                            state.set(WsState::Closed(format!(
                                "closed by server{}",
                                code.map(|c| format!(" ({c})")).unwrap_or_default()
                            )));
                            return;
                        }
                        Some(Ok(msg)) => {
                            messages.push(msg);
                            if messages.len() > MESSAGE_BUFFER {
                                messages.remove(0);
                            }
                            state.set(WsState::Open { messages: messages.clone() });
                        }
                        Some(Err(e)) => {
                            state.set(WsState::Closed(e.to_string()));
                            return;
                        }
                        None => std::thread::sleep(std::time::Duration::from_millis(30)),
                    }
                }
            });
        }
    }

    WsHandle { state: state.get(), sender: sender_slot.get() }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    /// A real local WebSocket echo server (tungstenite accept) for one
    /// connection; reports when its socket ends (close or EOF).
    fn echo_server() -> (String, mpsc::Receiver<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (ended_tx, ended_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(sock).unwrap();
            loop {
                match ws.read() {
                    Ok(msg) if msg.is_text() || msg.is_binary() => {
                        let _ = ws.send(msg);
                    }
                    Ok(tungstenite::Message::Close(_)) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            let _ = ended_tx.send(());
        });
        (url, ended_rx)
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
    fn connects_echoes_a_real_message_and_unmount_closes_the_connection() {
        let component = rosace_core::ComponentId(9101);
        let (url, ended_rx) = echo_server();

        let mut ctx = Context::new(component);
        let handle = use_websocket(&mut ctx, &url);
        assert_eq!(handle.state, WsState::Connecting);

        // Rebuilds observe Open once the worker's handshake completes.
        assert!(
            wait_for(
                || {
                    let mut ctx = Context::new(component);
                    matches!(use_websocket(&mut ctx, &url).state, WsState::Open { .. })
                },
                5
            ),
            "must reach Open against a real local server"
        );

        // Send through the handle; the echo must land in the state.
        let mut ctx = Context::new(component);
        let handle = use_websocket(&mut ctx, &url);
        assert!(handle.send(WsMessage::Text("hello-ws".into())));
        assert!(
            wait_for(
                || {
                    let mut ctx = Context::new(component);
                    match use_websocket(&mut ctx, &url).state {
                        WsState::Open { messages } => messages
                            .iter()
                            .any(|m| m.as_text() == Some("hello-ws")),
                        _ => false,
                    }
                },
                5
            ),
            "the echoed message must arrive through the hook state"
        );

        // Unmount: the worker must CLOSE the connection — observed from
        // the server side, not inferred from UI state.
        rosace_state::cleanup_store::fire_and_clear(component);
        assert!(
            ended_rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "server must see the connection end after unmount"
        );
    }
}
