//! WebSocket client for TEZZERA — no async runtime, no tungstenite.
//!
//! Uses `std::net::TcpStream` with a hand-rolled RFC 6455 handshake.
//! `WsClient::recv()` is non-blocking via `set_nonblocking(true)` —
//! safe to call each frame without blocking the render loop.
//!
//! # Example
//! ```rust,ignore
//! use tezzera_ws::{WsClient, WsMessage};
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
pub mod frame;
pub mod handshake;
pub mod message;
pub mod stream;

pub use client::WsClient;
pub use error::WsError;
pub use message::WsMessage;
pub use stream::{WsState, WsStream};
