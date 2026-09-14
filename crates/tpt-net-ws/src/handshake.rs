// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! RFC 6455 opening handshake (client key generation, server accept
//! computation via SHA-1 + base64).

use alloc::string::String;
use alloc::vec::Vec;

/// The fixed WS GUID from RFC 6455 §1.3.
pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Generate a random 16-byte client key (raw; base64 it before sending).
pub fn generate_client_key() -> [u8; 16] {
    let mut key = [0u8; 16];
    getrandom::getrandom(&mut key).expect("system RNG unavailable");
    key
}

/// Compute `Sec-WebSocket-Accept` for a `Sec-WebSocket-Key` value:
/// base64(SHA1(key + WS_GUID)).
pub fn accept_key(client_key_b64: &[u8]) -> String {
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(client_key_b64);
    hasher.update(WS_GUID.as_bytes());
    let digest = hasher.finalize();
    base64_encode(&digest)
}

/// Minimal standard base64 encoder (RFC 4648, with padding) — only used for
/// the 28-character accept header, so no streaming/line-wrapping needed.
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Validate an upgrade request's `Sec-WebSocket-Key` and produce the
/// `Sec-WebSocket-Accept` response value.
pub fn validate_and_accept(key_header: &[u8]) -> Result<String, crate::error::WsError> {
    if key_header.len() < 16 {
        return Err(crate::error::WsError::Handshake(
            "missing Sec-WebSocket-Key",
        ));
    }
    Ok(accept_key(key_header))
}

/// Build the raw bytes of a `101 Switching Protocols` response.
pub fn render_accept_response(accept: &str) -> Vec<u8> {
    render_accept_response_with_ext(accept, None)
}

/// Build the raw bytes of a `101 Switching Protocols` response with an
/// optional `Sec-WebSocket-Extensions` header line (used by
/// `permessage-deflate` negotiation).
pub fn render_accept_response_with_ext(accept: &str, extensions: Option<&str>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"HTTP/1.1 101 Switching Protocols\r\n");
    out.extend_from_slice(b"upgrade: websocket\r\n");
    out.extend_from_slice(b"connection: Upgrade\r\n");
    out.extend_from_slice(b"sec-websocket-accept: ");
    out.extend_from_slice(accept.as_bytes());
    out.extend_from_slice(b"\r\n");
    if let Some(ext) = extensions {
        out.extend_from_slice(b"sec-websocket-extensions: ");
        out.extend_from_slice(ext.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"\r\n");
    out
}

/// Build the raw bytes of a client's `GET … Upgrade` handshake request.
pub fn render_client_request(path: &str, host: &str, key_b64: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"GET ");
    out.extend_from_slice(path.as_bytes());
    out.extend_from_slice(b" HTTP/1.1\r\n");
    out.extend_from_slice(b"host: ");
    out.extend_from_slice(host.as_bytes());
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(b"upgrade: websocket\r\n");
    out.extend_from_slice(b"connection: Upgrade\r\n");
    out.extend_from_slice(b"sec-websocket-key: ");
    out.extend_from_slice(key_b64.as_bytes());
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(b"sec-websocket-version: 13\r\n\r\n");
    out
}
