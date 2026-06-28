use std::io::{Read, Write};
use std::net::TcpStream;
use crate::error::WsError;
use crate::frame::{decode_frame, encode_frame};
use crate::handshake::{generate_key, parse_ws_url, upgrade_request, validate_upgrade_response};
use crate::message::WsMessage;

/// A WebSocket client connection backed by a `TcpStream`.
pub struct WsClient {
    stream: TcpStream,
    closed: bool,
    buf: Vec<u8>,
}

impl WsClient {
    /// Connect to a `ws://` server and perform the RFC 6455 handshake.
    pub fn connect(url: &str) -> Result<Self, WsError> {
        let (host, port, path) = parse_ws_url(url)?;
        let addr = format!("{host}:{port}");

        let stream = TcpStream::connect(&addr)
            .map_err(|e| WsError::Connect(format!("{addr}: {e}")))?;

        stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .map_err(|e| WsError::Connect(e.to_string()))?;

        let mut client = Self { stream, closed: false, buf: Vec::new() };

        let key = generate_key();
        let req = upgrade_request(&host, &path, &key);
        client.stream.write_all(req.as_bytes())
            .map_err(|e| WsError::Handshake(e.to_string()))?;

        let mut resp = [0u8; 1024];
        let n = client.stream.read(&mut resp)
            .map_err(|e| WsError::Handshake(e.to_string()))?;
        let resp_str = String::from_utf8_lossy(&resp[..n]);
        validate_upgrade_response(&resp_str)?;

        client.stream.set_nonblocking(true)
            .map_err(|e| WsError::Connect(e.to_string()))?;

        Ok(client)
    }

    /// Send a WebSocket message.
    pub fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
        if self.closed { return Err(WsError::Closed); }
        let frame = encode_frame(&msg);
        self.stream.write_all(&frame)
            .map_err(|e| WsError::Send(e.to_string()))
    }

    /// Non-blocking receive. Returns `None` if no data is available yet.
    pub fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        if self.closed { return Some(Err(WsError::Closed)); }
        let mut tmp = [0u8; 4096];
        match self.stream.read(&mut tmp) {
            Ok(0) => {
                self.closed = true;
                Some(Err(WsError::Closed))
            }
            Ok(n) => {
                self.buf.extend_from_slice(&tmp[..n]);
                match decode_frame(&self.buf) {
                    Ok((msg, consumed)) => {
                        self.buf.drain(..consumed);
                        if msg.is_close() { self.closed = true; }
                        Some(Ok(msg))
                    }
                    Err(WsError::InvalidFrame(ref s)) if s.contains("incomplete") => None,
                    Err(e) => Some(Err(e)),
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => None,
            Err(e) => Some(Err(WsError::Recv(e.to_string()))),
        }
    }

    /// Send a close frame and mark the connection closed.
    pub fn close(&mut self) {
        if !self.closed {
            let _ = self.send(WsMessage::Close(Some(1000)));
            self.closed = true;
        }
    }

    pub fn is_closed(&self) -> bool { self.closed }
}
