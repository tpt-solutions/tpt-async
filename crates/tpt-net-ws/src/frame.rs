// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! RFC 6455 frame codec with strict bounds checks.

use alloc::vec::Vec;

use crate::error::WsError;

/// Maximum payload this crate accepts (default 16 MiB per the roadmap).
pub const MAX_PAYLOAD: usize = 16 * 1024 * 1024;

/// RFC 6455 §5.2 opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    /// Continuation of a fragmented data message.
    Continuation,
    /// UTF-8 text data.
    Text,
    /// Binary data.
    Binary,
    /// Connection close handshake.
    Close,
    /// Keepalive probe.
    Ping,
    /// Keepalive reply.
    Pong,
}

impl Opcode {
    fn from_u8(v: u8) -> Result<Self, WsError> {
        Ok(match v {
            0x0 => Opcode::Continuation,
            0x1 => Opcode::Text,
            0x2 => Opcode::Binary,
            0x8 => Opcode::Close,
            0x9 => Opcode::Ping,
            0xA => Opcode::Pong,
            _ => return Err(WsError::Protocol("reserved opcode")),
        })
    }

    fn to_u8(self) -> u8 {
        match self {
            Opcode::Continuation => 0x0,
            Opcode::Text => 0x1,
            Opcode::Binary => 0x2,
            Opcode::Close => 0x8,
            Opcode::Ping => 0x9,
            Opcode::Pong => 0xA,
        }
    }

    /// Control frames may not be fragmented and carry ≤ 125 bytes.
    pub fn is_control(self) -> bool {
        matches!(self, Opcode::Close | Opcode::Ping | Opcode::Pong)
    }
}

/// A decoded frame header plus its (possibly masked) payload bytes.
#[derive(Debug, Clone)]
pub struct Frame {
    /// FIN bit: this frame is the last of its message.
    pub fin: bool,
    /// RSV1 bit: set on the first data frame of a message compressed with
    /// the negotiated `permessage-deflate` extension.
    pub rsv1: bool,
    /// Frame opcode.
    pub opcode: Opcode,
    /// Unmasked payload.
    pub payload: Vec<u8>,
}

/// Encode a frame onto `out` (client frames are masked with `mask`;
/// server frames pass `None`).
pub fn encode(frame: &Frame, mask: Option<[u8; 4]>, out: &mut Vec<u8>) {
    let len = frame.payload.len();
    let bits = (u8::from(frame.fin) << 7) | (u8::from(frame.rsv1) << 6) | frame.opcode.to_u8();
    let mask_bit = if mask.is_some() { 0x80 } else { 0x00 };

    out.push(bits);
    if len < 126 {
        out.push(mask_bit | len as u8);
    } else if len <= u16::MAX as usize {
        out.push(mask_bit | 126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(mask_bit | 127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }

    match mask {
        Some(key) => {
            out.extend_from_slice(&key);
            let start = out.len();
            out.extend_from_slice(&frame.payload);
            apply_mask(&mut out[start..], key);
        }
        None => out.extend_from_slice(&frame.payload),
    }
}

/// Which side of the connection we are decoding for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// We are the client: server frames arrive unmasked.
    Client,
    /// We are the server: client frames arrive masked (RFC 6455 §5.1).
    Server,
}

/// Decode one frame from the front of `buf`, returning the frame and the
/// number of bytes consumed.  `Ok(None)` = need more bytes.
///
/// Masking is validated against the role: client frames MUST be masked,
/// server frames MUST NOT be (RFC 6455 §5.1).  Frames exceeding
/// [`MAX_PAYLOAD`] are rejected before allocation.
pub fn decode(buf: &[u8], role: Role) -> Result<Option<(Frame, usize)>, WsError> {
    if buf.len() < 2 {
        return Ok(None);
    }
    let fin = buf[0] & 0x80 != 0;
    let rsv1 = buf[0] & 0x40 != 0;
    // RSV2/RSV3 are only settable by extensions we never negotiate.
    if buf[0] & 0x30 != 0 {
        return Err(WsError::Protocol("RSV2/RSV3 bits set"));
    }
    let opcode = Opcode::from_u8(buf[0] & 0x0F)?;

    let masked = buf[1] & 0x80 != 0;
    let len7 = (buf[1] & 0x7F) as usize;

    let mut pos = 2;
    let payload_len = match len7 {
        126 => {
            if buf.len() < pos + 2 {
                return Ok(None);
            }
            let n = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
            pos += 2;
            n
        }
        127 => {
            if buf.len() < pos + 8 {
                return Ok(None);
            }
            let n = u64::from_be_bytes([
                buf[pos],
                buf[pos + 1],
                buf[pos + 2],
                buf[pos + 3],
                buf[pos + 4],
                buf[pos + 5],
                buf[pos + 6],
                buf[pos + 7],
            ]);
            pos += 8;
            // High bit must be 0 and length must fit in usize.
            if n > MAX_PAYLOAD as u64 || n > usize::MAX as u64 {
                return Err(WsError::Protocol("frame exceeds 16 MiB limit"));
            }
            n as usize
        }
        n => n,
    };

    if payload_len > MAX_PAYLOAD {
        return Err(WsError::Protocol("frame exceeds 16 MiB limit"));
    }
    // Control frames are capped at 125 bytes by the RFC.
    if opcode.is_control() && payload_len > 125 {
        return Err(WsError::Protocol("control frame larger than 125 bytes"));
    }

    let mask = if masked {
        if buf.len() < pos + 4 {
            return Ok(None);
        }
        let key = [buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]];
        pos += 4;
        Some(key)
    } else {
        None
    };

    match (role, masked) {
        (Role::Server, false) => return Err(WsError::Protocol("client frame is not masked")),
        (Role::Client, true) => return Err(WsError::Protocol("server frame is masked")),
        _ => {}
    }

    if buf.len() < pos + payload_len {
        return Ok(None);
    }
    let mut payload = buf[pos..pos + payload_len].to_vec();
    if let Some(key) = mask {
        apply_mask(&mut payload, key);
    }
    pos += payload_len;

    Ok(Some((
        Frame {
            fin,
            rsv1,
            opcode,
            payload,
        },
        pos,
    )))
}

/// RFC 6455 §5.3 masking transform (symmetric).
pub fn apply_mask(data: &mut [u8], key: [u8; 4]) {
    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= key[i % 4];
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handshake::accept_key;

    #[test]
    fn rfc6455_accept_key_vector() {
        // RFC 6455 §1.3 example.
        let accept = accept_key(b"dGhlIHNhbXBsZSBub25jZQ==");
        assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(crate::handshake::base64_encode(b""), "");
        assert_eq!(crate::handshake::base64_encode(b"f"), "Zg==");
        assert_eq!(crate::handshake::base64_encode(b"fo"), "Zm8=");
        assert_eq!(crate::handshake::base64_encode(b"foo"), "Zm9v");
        assert_eq!(crate::handshake::base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(crate::handshake::base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(crate::handshake::base64_encode(b"foobar"), "Zm9vYmFy");
    }

    fn mask_key() -> [u8; 4] {
        [0x12, 0x34, 0x56, 0x78]
    }

    fn sample(op: Opcode, len: usize) -> (Frame, Vec<u8>) {
        let frame = Frame {
            fin: true,
            rsv1: false,
            opcode: op,
            payload: vec![0xAB; len],
        };
        let mut buf = Vec::new();
        encode(&frame, Some(mask_key()), &mut buf);
        (frame, buf)
    }

    #[test]
    fn frame_roundtrip_masked_and_unmasked() {
        let payload = b"the quick brown fox".to_vec();

        for mask in [Some(mask_key()), None] {
            let frame = Frame {
                fin: true,
                rsv1: false,
                opcode: Opcode::Text,
                payload: payload.clone(),
            };
            let mut buf = Vec::new();
            encode(&frame, mask, &mut buf);

            let role = if mask.is_some() {
                Role::Server
            } else {
                Role::Client
            };
            let (decoded, consumed) = decode(&buf, role).unwrap().unwrap();
            assert_eq!(consumed, buf.len());
            assert!(decoded.fin);
            assert_eq!(decoded.opcode, Opcode::Text);
            assert_eq!(decoded.payload, payload);
        }
    }

    #[test]
    fn small_and_extended_lengths() {
        for len in [125usize, 126, 65535, 65536] {
            let (frame, buf) = sample(Opcode::Binary, len);
            let (decoded, _) = decode(&buf, Role::Server).unwrap().unwrap();
            assert_eq!(decoded.payload.len(), frame.payload.len());
        }
    }

    #[test]
    fn partial_buffer_needs_more_bytes() {
        let (_, buf) = sample(Opcode::Binary, 300);
        for cut in [1, 2, 4, 10, buf.len() - 1] {
            assert!(
                decode(&buf[..cut], Role::Server).unwrap().is_none(),
                "expected Partial at {cut}"
            );
        }
    }

    #[test]
    fn unmasked_client_frame_is_rejected() {
        let frame = Frame {
            fin: true,
            rsv1: false,
            opcode: Opcode::Text,
            payload: b"hi".to_vec(),
        };
        let mut buf = Vec::new();
        encode(&frame, None, &mut buf); // server-style (unmasked)
        assert!(
            decode(&buf, Role::Server).is_err(),
            "client frames must be masked"
        );
    }

    #[test]
    fn oversized_control_frame_is_rejected() {
        let frame = Frame {
            fin: true,
            rsv1: false,
            opcode: Opcode::Ping,
            payload: vec![0; 200],
        };
        let mut buf = Vec::new();
        encode(&frame, Some(mask_key()), &mut buf);
        assert!(
            decode(&buf, Role::Server).is_err(),
            "control frames cap at 125"
        );
    }
}
