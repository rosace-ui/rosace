/// A WebSocket message.
#[derive(Debug, Clone, PartialEq)]
pub enum WsMessage {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<u16>), // optional status code
}

impl WsMessage {
    pub fn is_text(&self) -> bool { matches!(self, WsMessage::Text(_)) }
    pub fn is_binary(&self) -> bool { matches!(self, WsMessage::Binary(_)) }
    pub fn is_close(&self) -> bool { matches!(self, WsMessage::Close(_)) }
    pub fn is_ping(&self) -> bool { matches!(self, WsMessage::Ping(_)) }
    pub fn is_pong(&self) -> bool { matches!(self, WsMessage::Pong(_)) }

    pub fn as_text(&self) -> Option<&str> {
        if let WsMessage::Text(s) = self { Some(s) } else { None }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        if let WsMessage::Binary(b) = self { Some(b) } else { None }
    }

    /// Opcode per RFC 6455.
    pub fn opcode(&self) -> u8 {
        match self {
            WsMessage::Text(_)   => 0x1,
            WsMessage::Binary(_) => 0x2,
            WsMessage::Close(_)  => 0x8,
            WsMessage::Ping(_)   => 0x9,
            WsMessage::Pong(_)   => 0xA,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_message_text_is_text() {
        let m = WsMessage::Text("hello".into());
        assert!(m.is_text());
        assert!(!m.is_binary());
        assert!(!m.is_close());
        assert!(!m.is_ping());
        assert!(!m.is_pong());
    }

    #[test]
    fn ws_message_binary_is_binary() {
        let m = WsMessage::Binary(vec![1, 2, 3]);
        assert!(m.is_binary());
        assert!(!m.is_text());
    }

    #[test]
    fn ws_message_close_is_close() {
        let m = WsMessage::Close(Some(1000));
        assert!(m.is_close());
        assert!(!m.is_text());
    }

    #[test]
    fn ws_message_ping_is_ping() {
        let m = WsMessage::Ping(vec![]);
        assert!(m.is_ping());
        assert!(!m.is_pong());
    }

    #[test]
    fn ws_message_pong_is_pong() {
        let m = WsMessage::Pong(vec![]);
        assert!(m.is_pong());
        assert!(!m.is_ping());
    }

    #[test]
    fn ws_message_as_text() {
        let m = WsMessage::Text("world".into());
        assert_eq!(m.as_text(), Some("world"));
        let b = WsMessage::Binary(vec![]);
        assert_eq!(b.as_text(), None);
    }

    #[test]
    fn ws_message_as_bytes() {
        let m = WsMessage::Binary(vec![0xDE, 0xAD]);
        assert_eq!(m.as_bytes(), Some(&[0xDE, 0xAD][..]));
        let t = WsMessage::Text("hi".into());
        assert_eq!(t.as_bytes(), None);
    }

    #[test]
    fn ws_message_opcode_text() {
        assert_eq!(WsMessage::Text("x".into()).opcode(), 0x1);
    }

    #[test]
    fn ws_message_opcode_binary() {
        assert_eq!(WsMessage::Binary(vec![]).opcode(), 0x2);
    }

    #[test]
    fn ws_message_opcode_close() {
        assert_eq!(WsMessage::Close(None).opcode(), 0x8);
    }

    #[test]
    fn ws_message_opcode_ping() {
        assert_eq!(WsMessage::Ping(vec![]).opcode(), 0x9);
    }

    #[test]
    fn ws_message_opcode_pong() {
        assert_eq!(WsMessage::Pong(vec![]).opcode(), 0xA);
    }

    #[test]
    fn ws_message_clone_eq() {
        let m = WsMessage::Text("clone me".into());
        assert_eq!(m.clone(), m);
    }
}
