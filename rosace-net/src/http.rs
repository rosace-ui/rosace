use std::io::{Read, Write};
use std::net::TcpStream;

/// Parse a URL into (host, port, path). Supports http:// only.
pub fn parse_url(url: &str) -> Result<(String, u16, String), String> {
    let url = url.trim();
    let url = url.strip_prefix("http://").ok_or("only http:// URLs supported")?;
    let (host_part, path) = url.split_once('/').unwrap_or((url, ""));
    let path = format!("/{}", path);

    let (host, port) = if host_part.contains(':') {
        let mut parts = host_part.splitn(2, ':');
        let h = parts.next().unwrap_or("").to_string();
        let p = parts.next().unwrap_or("80").parse::<u16>().map_err(|_| "invalid port")?;
        (h, p)
    } else {
        (host_part.to_string(), 80)
    };

    Ok((host, port, path))
}

/// Perform a simple HTTP/1.0 GET request. Returns the response body bytes.
pub fn http_get(url: &str) -> Result<Vec<u8>, String> {
    let (host, port, path) = parse_url(url)?;
    let addr = format!("{}:{}", host, port);

    let mut stream = TcpStream::connect(&addr)
        .map_err(|e| format!("connect to {}: {}", addr, e))?;

    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .map_err(|e| format!("set timeout: {}", e))?;

    let request = format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host
    );
    stream.write_all(request.as_bytes())
        .map_err(|e| format!("write: {}", e))?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)
        .map_err(|e| format!("read: {}", e))?;

    // Split headers from body
    let sep = b"\r\n\r\n";
    let body_start = response.windows(4)
        .position(|w| w == sep)
        .ok_or("no header separator in response")?
        + 4;

    // Check status line
    let headers = &response[..body_start];
    let status_line = std::str::from_utf8(headers)
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("");
    if !status_line.contains("200") {
        return Err(format!("HTTP error: {}", status_line));
    }

    Ok(response[body_start..].to_vec())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_simple() {
        let (host, port, path) = parse_url("http://example.com").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/");
    }

    #[test]
    fn parse_url_with_port() {
        let (host, port, path) = parse_url("http://example.com:8080/img.png").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 8080);
        assert_eq!(path, "/img.png");
    }

    #[test]
    fn parse_url_with_path() {
        let (host, port, path) = parse_url("http://cdn.example.com/images/photo.jpg").unwrap();
        assert_eq!(host, "cdn.example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/images/photo.jpg");
    }

    #[test]
    fn parse_url_root_path() {
        let (host, port, path) = parse_url("http://example.com/").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/");
    }

    #[test]
    fn parse_url_rejects_https() {
        let result = parse_url("https://example.com/photo.png");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("http://"));
    }

    #[test]
    fn parse_url_no_slash_gives_root_path() {
        let (host, port, path) = parse_url("http://example.com").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/");
    }
}
