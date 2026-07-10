use crate::error::WsError;
use crate::message::WsMessage;

/// Encode a client→server frame (always masked per RFC 6455).
pub fn encode_frame(msg: &WsMessage) -> Vec<u8> {
    let payload: Vec<u8> = match msg {
        WsMessage::Text(s)     => s.as_bytes().to_vec(),
        WsMessage::Binary(b)   => b.clone(),
        WsMessage::Ping(b)     => b.clone(),
        WsMessage::Pong(b)     => b.clone(),
        WsMessage::Close(code) => code.map(|c| vec![(c >> 8) as u8, c as u8]).unwrap_or_default(),
    };

    let opcode = msg.opcode();
    let payload_len = payload.len();
    let mask: [u8; 4] = [0x37, 0xfa, 0x21, 0x3d]; // fixed mask for simplicity

    let mut frame = vec![0x80 | opcode]; // FIN + opcode

    if payload_len < 126 {
        frame.push(0x80 | payload_len as u8);
    } else if payload_len < 65536 {
        frame.push(0x80 | 126);
        frame.push((payload_len >> 8) as u8);
        frame.push(payload_len as u8);
    } else {
        frame.push(0x80 | 127);
        for i in (0..8).rev() {
            frame.push((payload_len >> (i * 8)) as u8);
        }
    }

    frame.extend_from_slice(&mask);

    for (i, &b) in payload.iter().enumerate() {
        frame.push(b ^ mask[i % 4]);
    }

    frame
}

/// Decode a server→client frame (unmasked).
/// Returns `(WsMessage, bytes_consumed)`.
pub fn decode_frame(data: &[u8]) -> Result<(WsMessage, usize), WsError> {
    if data.len() < 2 {
        return Err(WsError::InvalidFrame("too short".into()));
    }

    let opcode   = data[0] & 0x0F;
    let masked   = (data[1] & 0x80) != 0;
    let len_byte = (data[1] & 0x7F) as usize;

    let (payload_len, header_size): (usize, usize) = if len_byte < 126 {
        (len_byte, 2)
    } else if len_byte == 126 {
        if data.len() < 4 {
            return Err(WsError::InvalidFrame("short 16-bit len".into()));
        }
        let len = ((data[2] as usize) << 8) | data[3] as usize;
        (len, 4)
    } else {
        if data.len() < 10 {
            return Err(WsError::InvalidFrame("short 64-bit len".into()));
        }
        let mut len = 0usize;
        for i in 0..8 {
            len = (len << 8) | data[2 + i] as usize;
        }
        (len, 10)
    };

    let mask_offset = if masked { header_size + 4 } else { header_size };
    let total = mask_offset + payload_len;
    if data.len() < total {
        return Err(WsError::InvalidFrame("incomplete payload".into()));
    }

    let raw = &data[mask_offset..total];
    let payload: Vec<u8> = if masked {
        let mask = &data[header_size..header_size + 4];
        raw.iter().enumerate().map(|(i, &b)| b ^ mask[i % 4]).collect()
    } else {
        raw.to_vec()
    };

    let msg = match opcode {
        0x1 => WsMessage::Text(String::from_utf8_lossy(&payload).into_owned()),
        0x2 => WsMessage::Binary(payload),
        0x8 => WsMessage::Close(if payload.len() >= 2 {
            Some(((payload[0] as u16) << 8) | payload[1] as u16)
        } else {
            None
        }),
        0x9 => WsMessage::Ping(payload),
        0xA => WsMessage::Pong(payload),
        _   => return Err(WsError::InvalidFrame(format!("unknown opcode 0x{opcode:02x}"))),
    };

    Ok((msg, total))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_text_frame_starts_with_fin_text() {
        let frame = encode_frame(&WsMessage::Text("hi".into()));
        // First byte: FIN(1) + RSV(0,0,0) + opcode(0x1) = 0x81
        assert_eq!(frame[0], 0x81);
        // Second byte: MASK(1) + len(2) = 0x82
        assert_eq!(frame[1], 0x82);
    }

    #[test]
    fn encode_binary_frame() {
        let data = vec![0x01, 0x02, 0x03];
        let frame = encode_frame(&WsMessage::Binary(data.clone()));
        // First byte: FIN + opcode 0x2 = 0x82
        assert_eq!(frame[0], 0x82);
        // MASK bit set in second byte
        assert!(frame[1] & 0x80 != 0);
    }

    #[test]
    fn encode_close_frame_with_code() {
        let frame = encode_frame(&WsMessage::Close(Some(1000)));
        assert_eq!(frame[0], 0x88); // FIN + opcode 0x8
        // payload = [0x03, 0xE8] (1000 in big-endian), masked
    }

    #[test]
    fn encode_close_frame_no_code() {
        let frame = encode_frame(&WsMessage::Close(None));
        assert_eq!(frame[0], 0x88);
        // len byte (masked): 0x80 | 0 = 0x80
        assert_eq!(frame[1], 0x80);
    }

    #[test]
    fn decode_text_frame_roundtrip() {
        // Build an unmasked server→client text frame for "hello"
        let payload = b"hello";
        let mut data = vec![0x81u8, payload.len() as u8];
        data.extend_from_slice(payload);

        let (msg, consumed) = decode_frame(&data).unwrap();
        assert_eq!(msg, WsMessage::Text("hello".into()));
        assert_eq!(consumed, data.len());
    }

    #[test]
    fn decode_binary_frame_roundtrip() {
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let mut data = vec![0x82u8, payload.len() as u8];
        data.extend_from_slice(&payload);

        let (msg, consumed) = decode_frame(&data).unwrap();
        assert_eq!(msg, WsMessage::Binary(payload));
        assert_eq!(consumed, data.len());
    }

    #[test]
    fn decode_ping_frame() {
        let payload = b"ping-data";
        let mut data = vec![0x89u8, payload.len() as u8];
        data.extend_from_slice(payload);

        let (msg, _) = decode_frame(&data).unwrap();
        assert_eq!(msg, WsMessage::Ping(b"ping-data".to_vec()));
    }

    #[test]
    fn decode_pong_frame() {
        let mut data = vec![0x8Au8, 0x00u8];
        let (msg, _) = decode_frame(&data).unwrap();
        assert_eq!(msg, WsMessage::Pong(vec![]));

        // with payload
        let payload = b"pong";
        data = vec![0x8Au8, payload.len() as u8];
        data.extend_from_slice(payload);
        let (msg2, _) = decode_frame(&data).unwrap();
        assert_eq!(msg2, WsMessage::Pong(b"pong".to_vec()));
    }

    #[test]
    fn decode_close_frame_with_code() {
        let data = vec![0x88u8, 0x02u8, 0x03u8, 0xE8u8]; // code 1000
        let (msg, consumed) = decode_frame(&data).unwrap();
        assert_eq!(msg, WsMessage::Close(Some(1000)));
        assert_eq!(consumed, 4);
    }

    #[test]
    fn decode_close_frame_no_payload() {
        let data = vec![0x88u8, 0x00u8];
        let (msg, _) = decode_frame(&data).unwrap();
        assert_eq!(msg, WsMessage::Close(None));
    }

    #[test]
    fn decode_short_data_errors() {
        let data = vec![0x81u8]; // only 1 byte — too short
        let result = decode_frame(&data);
        assert!(matches!(result, Err(WsError::InvalidFrame(_))));
    }

    #[test]
    fn decode_empty_errors() {
        let result = decode_frame(&[]);
        assert!(matches!(result, Err(WsError::InvalidFrame(_))));
    }

    #[test]
    fn decode_unknown_opcode_errors() {
        let data = vec![0x83u8, 0x00u8]; // opcode 0x3 — reserved/unknown
        let result = decode_frame(&data);
        assert!(matches!(result, Err(WsError::InvalidFrame(_))));
    }

    #[test]
    fn decode_incomplete_payload_errors() {
        // Says payload is 10 bytes but only provides 2
        let data = vec![0x81u8, 0x0Au8, 0x41u8, 0x42u8];
        let result = decode_frame(&data);
        assert!(matches!(result, Err(WsError::InvalidFrame(ref s)) if s.contains("incomplete")));
    }

    #[test]
    fn decode_16bit_length_frame() {
        // Build a frame with 16-bit extended length
        let payload = vec![0x42u8; 200];
        let mut data = vec![0x82u8, 126u8, 0x00u8, 200u8];
        data.extend_from_slice(&payload);

        let (msg, consumed) = decode_frame(&data).unwrap();
        assert_eq!(msg, WsMessage::Binary(payload));
        assert_eq!(consumed, data.len());
    }

    #[test]
    fn encode_masked_payload_differs_from_original() {
        let msg = WsMessage::Text("test".into());
        let frame = encode_frame(&msg);
        // mask starts at byte 2 (header=2), payload at byte 6
        let payload_start = 6;
        let raw_bytes = b"test";
        // masked bytes should differ from raw (mask is non-zero)
        let differs = frame[payload_start..]
            .iter()
            .zip(raw_bytes.iter())
            .any(|(masked, &orig)| *masked != orig);
        assert!(differs, "masked payload should differ from plaintext");
    }
}
