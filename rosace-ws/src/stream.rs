use crate::client::WsClient;
use crate::error::WsError;
use crate::message::WsMessage;

/// Connection state of a `WsStream`.
#[derive(Debug, Clone, PartialEq)]
pub enum WsState {
    Connecting,
    Open,
    Closing,
    Closed,
    Error(String),
}

/// A poll-based wrapper around `WsClient` with a state machine.
/// Call `poll()` each frame to drain incoming messages into the inbox.
pub struct WsStream {
    client: Option<WsClient>,
    state: WsState,
    inbox: Vec<WsMessage>,
}

impl WsStream {
    pub fn new(client: WsClient) -> Self {
        Self { client: Some(client), state: WsState::Open, inbox: Vec::new() }
    }

    /// Create in a pre-failed state (e.g. connect returned an error).
    pub fn failed(err: WsError) -> Self {
        Self { client: None, state: WsState::Error(err.to_string()), inbox: Vec::new() }
    }

    /// Drain incoming messages into the inbox. Call once per frame.
    pub fn poll(&mut self) {
        let Some(client) = self.client.as_mut() else { return; };
        loop {
            match client.recv() {
                Some(Ok(msg)) => {
                    if msg.is_close() {
                        self.state = WsState::Closed;
                        self.client = None;
                        break;
                    }
                    self.inbox.push(msg);
                }
                Some(Err(WsError::Closed)) => {
                    self.state = WsState::Closed;
                    self.client = None;
                    break;
                }
                Some(Err(e)) => {
                    self.state = WsState::Error(e.to_string());
                    self.client = None;
                    break;
                }
                None => break,
            }
        }
    }

    /// Drain all queued inbox messages.
    pub fn drain(&mut self) -> Vec<WsMessage> { std::mem::take(&mut self.inbox) }

    pub fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
        self.client.as_mut().ok_or(WsError::Closed)?.send(msg)
    }

    pub fn state(&self) -> &WsState { &self.state }
    pub fn is_open(&self) -> bool { self.state == WsState::Open }
    pub fn pending_count(&self) -> usize { self.inbox.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_stream_failed_is_error_state() {
        let s = WsStream::failed(WsError::Closed);
        assert!(matches!(s.state(), WsState::Error(_)));
    }

    #[test]
    fn ws_stream_pending_count_zero_initially() {
        let s = WsStream::failed(WsError::Closed);
        assert_eq!(s.pending_count(), 0);
    }

    #[test]
    fn ws_stream_drain_empties_inbox() {
        let mut s = WsStream::failed(WsError::Closed);
        let msgs = s.drain();
        assert!(msgs.is_empty());
    }

    #[test]
    fn ws_stream_failed_not_open() {
        let s = WsStream::failed(WsError::Connect("no server".into()));
        assert!(!s.is_open());
    }
}
