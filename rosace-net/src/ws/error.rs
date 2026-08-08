#[derive(Debug, Clone, PartialEq)]
pub enum WsError {
    Connect(String),
    Handshake(String),
    Send(String),
    Recv(String),
    InvalidFrame(String),
    Closed,
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WsError::Connect(e)      => write!(f, "WS connect error: {e}"),
            WsError::Handshake(e)    => write!(f, "WS handshake error: {e}"),
            WsError::Send(e)         => write!(f, "WS send error: {e}"),
            WsError::Recv(e)         => write!(f, "WS recv error: {e}"),
            WsError::InvalidFrame(e) => write!(f, "WS invalid frame: {e}"),
            WsError::Closed          => write!(f, "WS connection closed"),
        }
    }
}

impl std::error::Error for WsError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_error_display_connect() {
        let e = WsError::Connect("refused".into());
        assert_eq!(e.to_string(), "WS connect error: refused");
    }

    #[test]
    fn ws_error_display_handshake() {
        let e = WsError::Handshake("bad response".into());
        assert_eq!(e.to_string(), "WS handshake error: bad response");
    }

    #[test]
    fn ws_error_display_send() {
        let e = WsError::Send("broken pipe".into());
        assert_eq!(e.to_string(), "WS send error: broken pipe");
    }

    #[test]
    fn ws_error_display_recv() {
        let e = WsError::Recv("timeout".into());
        assert_eq!(e.to_string(), "WS recv error: timeout");
    }

    #[test]
    fn ws_error_display_invalid_frame() {
        let e = WsError::InvalidFrame("too short".into());
        assert_eq!(e.to_string(), "WS invalid frame: too short");
    }

    #[test]
    fn ws_error_display_closed() {
        let e = WsError::Closed;
        assert_eq!(e.to_string(), "WS connection closed");
    }

    #[test]
    fn ws_error_clone() {
        let e = WsError::Connect("x".into());
        let e2 = e.clone();
        assert_eq!(e, e2);
    }

    #[test]
    fn ws_error_eq() {
        assert_eq!(WsError::Closed, WsError::Closed);
        assert_ne!(WsError::Closed, WsError::Send("x".into()));
    }

    #[test]
    fn ws_error_is_error_trait() {
        let e: Box<dyn std::error::Error> = Box::new(WsError::Closed);
        assert_eq!(e.to_string(), "WS connection closed");
    }
}
