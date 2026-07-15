//! `WsClient` — `tungstenite`-backed since D113/Phase 30 Step 3 (the
//! hand-rolled RFC 6455 handshake/frame codec is gone; the sync crate, no
//! tokio). Public API unchanged: blocking `connect`/`send`, NON-blocking
//! `recv` (safe to call once per frame), `close`, `is_closed`.
//!
//! `wss://` works via `rustls` (same TLS story as `rosace-net`'s D113
//! HTTP client). On wasm32 `connect` returns the documented named-gap
//! error (browser-`WebSocket` backend is future work — `PHASE_30.md`).

use crate::error::WsError;
use crate::message::WsMessage;

#[cfg(not(target_arch = "wasm32"))]
use tungstenite::stream::MaybeTlsStream;

/// A synchronous WebSocket client. One connection per instance.
pub struct WsClient {
    #[cfg(not(target_arch = "wasm32"))]
    socket: tungstenite::WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    closed: bool,
}

impl WsClient {
    /// Connect (blocking — run on a background thread or at startup) and
    /// complete the handshake, then switch the stream to non-blocking so
    /// [`WsClient::recv`] never stalls a frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn connect(url: &str) -> Result<Self, WsError> {
        let (mut socket, _response) =
            tungstenite::connect(url).map_err(|e| WsError::Connect(e.to_string()))?;
        // Handshake ran blocking (clean); reads from here on must not block.
        match socket.get_mut() {
            MaybeTlsStream::Plain(s) => {
                s.set_nonblocking(true).map_err(|e| WsError::Connect(e.to_string()))?;
            }
            MaybeTlsStream::Rustls(tls) => {
                tls.get_mut()
                    .set_nonblocking(true)
                    .map_err(|e| WsError::Connect(e.to_string()))?;
            }
            _ => {}
        }
        Ok(Self { socket, closed: false })
    }

    /// wasm32: the named, documented gap — see the module doc.
    #[cfg(target_arch = "wasm32")]
    pub fn connect(_url: &str) -> Result<Self, WsError> {
        Err(WsError::Connect(
            "rosace-ws: WebSocket is not yet implemented on web (wasm32) — see PHASE_30.md's wasm constraint".to_string(),
        ))
    }

    /// Send a message. `WouldBlock` on a congested socket is surfaced as
    /// `WsError::Send` — callers on a per-frame cadence can retry.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
        if self.closed {
            return Err(WsError::Closed);
        }
        let out = match msg {
            WsMessage::Text(s) => tungstenite::Message::text(s),
            WsMessage::Binary(b) => tungstenite::Message::binary(b),
            WsMessage::Ping(b) => tungstenite::Message::Ping(b.into()),
            WsMessage::Pong(b) => tungstenite::Message::Pong(b.into()),
            WsMessage::Close(_) => {
                self.close();
                return Ok(());
            }
        };
        self.socket.send(out).map_err(|e| WsError::Send(e.to_string()))
    }

    #[cfg(target_arch = "wasm32")]
    pub fn send(&mut self, _msg: WsMessage) -> Result<(), WsError> {
        Err(WsError::Closed)
    }

    /// Non-blocking receive — `None` when no message is ready. Call once
    /// per frame. Ping/pong keepalive is handled by `tungstenite`
    /// internally during reads; pings still surface here too so callers
    /// can observe them (same behavior the hand-rolled client had).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        if self.closed {
            return None;
        }
        match self.socket.read() {
            Ok(tungstenite::Message::Text(s)) => Some(Ok(WsMessage::Text(s.to_string()))),
            Ok(tungstenite::Message::Binary(b)) => Some(Ok(WsMessage::Binary(b.to_vec()))),
            Ok(tungstenite::Message::Ping(b)) => Some(Ok(WsMessage::Ping(b.to_vec()))),
            Ok(tungstenite::Message::Pong(b)) => Some(Ok(WsMessage::Pong(b.to_vec()))),
            Ok(tungstenite::Message::Close(frame)) => {
                self.closed = true;
                Some(Ok(WsMessage::Close(frame.map(|f| u16::from(f.code)))))
            }
            Ok(tungstenite::Message::Frame(_)) => None, // raw frames never surface from read()
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => None,
            Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => {
                self.closed = true;
                Some(Ok(WsMessage::Close(None)))
            }
            Err(e) => {
                self.closed = true;
                Some(Err(WsError::Recv(e.to_string())))
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        None
    }

    /// Initiate a close handshake (best-effort) and mark the client closed.
    pub fn close(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = self.socket.close(None);
            let _ = self.socket.flush();
        }
        self.closed = true;
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }
}
