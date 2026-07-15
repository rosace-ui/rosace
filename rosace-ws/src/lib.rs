//! WebSocket client for ROSACE — `tungstenite`-backed (sync, no async
//! runtime) since D113/Phase 30 Step 3; `wss://` works via `rustls`.
//! `WsClient::recv()` is non-blocking via `set_nonblocking(true)` —
//! safe to call each frame without blocking the render loop.
//!
//! # Example
//! ```rust,ignore
//! use rosace_ws::{WsClient, WsMessage};
//!
//! match WsClient::connect("ws://echo.websocket.org/") {
//!     Ok(mut client) => {
//!         client.send(WsMessage::Text("hello".into())).unwrap();
//!         if let Some(Ok(msg)) = client.recv() {
//!             println!("echo: {:?}", msg);
//!         }
//!     }
//!     Err(e) => eprintln!("connect failed: {e}"),
//! }
//! ```

pub mod client;
pub mod error;
pub mod hook;
pub mod message;

pub use client::WsClient;
pub use hook::{use_websocket, WsHandle, WsState};
pub use error::WsError;
pub use message::WsMessage;
