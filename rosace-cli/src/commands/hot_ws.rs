//! Minimal WebSocket server bits for the web hot-reload push (D103 Tier 1).
//!
//! `rsc dev --target web` serves the app over HTTP and, on a `.rs` edit, pushes
//! the edited source to the browser so it can hot-swap `view!` shapes. That
//! needs a WebSocket, and `rsc-cli` is kept dependency-light — so the handshake
//! (SHA-1 + base64 of the RFC 6455 accept key) and server→client text framing
//! are hand-rolled here. Both are well-defined and unit-tested against known
//! vectors (including RFC 6455's own handshake example).
//!
//! Only what the dev server needs: the handshake accept key and encoding an
//! unmasked text frame. We never READ client frames (the browser only
//! receives), so client-frame decoding is intentionally omitted.

/// The RFC 6455 magic GUID appended to the client key before hashing.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Compute the `Sec-WebSocket-Accept` value for a client's `Sec-WebSocket-Key`.
pub fn accept_key(client_key: &str) -> String {
    let digest = sha1(format!("{client_key}{WS_GUID}").as_bytes());
    base64_encode(&digest)
}

/// Encode a server→client **text** frame (FIN=1, opcode=0x1, unmasked — a
/// server must not mask). Handles the 7-bit / 16-bit / 64-bit length forms.
pub fn text_frame(payload: &str) -> Vec<u8> {
    let bytes = payload.as_bytes();
    let mut frame = Vec::with_capacity(bytes.len() + 10);
    frame.push(0x81); // FIN + text opcode
    let len = bytes.len();
    if len < 126 {
        frame.push(len as u8);
    } else if len <= u16::MAX as usize {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(bytes);
    frame
}

/// Pull the `Sec-WebSocket-Key` header value out of a raw HTTP request, if it
/// looks like a WebSocket upgrade.
pub fn websocket_key(request: &str) -> Option<String> {
    let is_upgrade = request
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("upgrade:") && l.to_ascii_lowercase().contains("websocket"));
    if !is_upgrade {
        return None;
    }
    request.lines().find_map(|l| {
        let lower = l.to_ascii_lowercase();
        if lower.starts_with("sec-websocket-key:") {
            Some(l[l.find(':')? + 1..].trim().to_string())
        } else {
            None
        }
    })
}

/// The full 101 Switching Protocols handshake response for a client key.
pub fn handshake_response(client_key: &str) -> String {
    format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(client_key)
    )
}

// ── SHA-1 (RFC 3174) — small, self-contained ────────────────────────────────

fn sha1(message: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

    // Pad: append 0x80, then zeros, then the 64-bit big-endian bit length.
    let bit_len = (message.len() as u64) * 8;
    let mut data = message.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in data.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

// ── base64 (standard alphabet) ──────────────────────────────────────────────

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_known_vectors() {
        // RFC 3174 / FIPS test vectors.
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
    }

    #[test]
    fn accept_key_matches_rfc6455_example() {
        // RFC 6455 §1.3: key "dGhlIHNhbXBsZSBub25jZQ==" → this exact accept.
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn text_frame_small_payload_is_fin_text_unmasked() {
        let f = text_frame("hi");
        assert_eq!(f[0], 0x81); // FIN + text
        assert_eq!(f[1], 2); // length 2, mask bit clear
        assert_eq!(&f[2..], b"hi");
    }

    #[test]
    fn text_frame_uses_16bit_length_over_125() {
        let payload = "x".repeat(200);
        let f = text_frame(&payload);
        assert_eq!(f[0], 0x81);
        assert_eq!(f[1], 126);
        assert_eq!(u16::from_be_bytes([f[2], f[3]]), 200);
        assert_eq!(f.len(), 4 + 200);
    }

    #[test]
    fn websocket_key_extracted_only_from_an_upgrade_request() {
        let req = "GET /__rosace_hot HTTP/1.1\r\nUpgrade: websocket\r\nSec-WebSocket-Key: abc123==\r\n\r\n";
        assert_eq!(websocket_key(req).as_deref(), Some("abc123=="));
        let plain = "GET /index.html HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(websocket_key(plain), None);
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
