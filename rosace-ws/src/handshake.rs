use crate::error::WsError;

/// Parse ws:// URL into (host, port, path). wss:// is rejected (no TLS).
pub fn parse_ws_url(url: &str) -> Result<(String, u16, String), WsError> {
    let url = url.trim();
    let rest = if let Some(r) = url.strip_prefix("ws://") {
        r
    } else if url.starts_with("wss://") {
        return Err(WsError::Connect("wss:// (TLS) not supported — use ws://".into()));
    } else {
        return Err(WsError::Connect("URL must start with ws://".into()));
    };

    let (host_part, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = format!("/{}", path);

    let (host, port) = if let Some((h, p)) = host_part.split_once(':') {
        let p = p.parse::<u16>().map_err(|_| WsError::Connect("invalid port".into()))?;
        (h.to_string(), p)
    } else {
        (host_part.to_string(), 80)
    };

    Ok((host, port, path))
}

/// Build the HTTP Upgrade request.
pub fn upgrade_request(host: &str, path: &str, key: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n"
    )
}

/// Validate that the server responded with 101 Switching Protocols.
pub fn validate_upgrade_response(response: &str) -> Result<(), WsError> {
    let first_line = response.lines().next().unwrap_or("");
    if !first_line.contains("101") {
        return Err(WsError::Handshake(format!("expected 101, got: {first_line}")));
    }
    Ok(())
}

/// A static WS key (structurally valid base64; deterministic for testing).
pub fn generate_key() -> String {
    "dGhlIHNhbXBsZSBub25jZQ==".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ws_url_simple() {
        let (host, port, path) = parse_ws_url("ws://example.com").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/");
    }

    #[test]
    fn parse_ws_url_with_port() {
        let (host, port, _) = parse_ws_url("ws://localhost:9001").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 9001);
    }

    #[test]
    fn parse_ws_url_with_path() {
        let (_, _, path) = parse_ws_url("ws://example.com/chat/room").unwrap();
        assert_eq!(path, "/chat/room");
    }

    #[test]
    fn parse_ws_url_rejects_wss() {
        let err = parse_ws_url("wss://example.com").unwrap_err();
        assert!(matches!(err, WsError::Connect(_)));
    }

    #[test]
    fn parse_ws_url_rejects_http() {
        let err = parse_ws_url("http://example.com").unwrap_err();
        assert!(matches!(err, WsError::Connect(_)));
    }

    #[test]
    fn upgrade_request_contains_host() {
        let req = upgrade_request("example.com", "/", "key123");
        assert!(req.contains("Host: example.com"));
        assert!(req.contains("Upgrade: websocket"));
        assert!(req.contains("key123"));
    }

    #[test]
    fn validate_upgrade_response_ok() {
        let resp = "HTTP/1.1 101 Switching Protocols\r\n\r\n";
        assert!(validate_upgrade_response(resp).is_ok());
    }

    #[test]
    fn validate_upgrade_response_fails_on_non_101() {
        let resp = "HTTP/1.1 400 Bad Request\r\n\r\n";
        assert!(validate_upgrade_response(resp).is_err());
    }
}
