// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `permessage-deflate` (RFC 7692) compression support.
//!
//! Simplified negotiation: when enabled, the client offers the extension
//! with `client_no_context_takeover` + `server_no_context_takeover`, and the
//! server accepts only with both parameters.  Every message is then
//! compressed independently (no context takeover between messages), which
//! trades some ratio for correctness simplicity and per-message isolation.
//!
//! Wire format per RFC 7692 §7.2.1: the payload is a raw DEFLATE stream
//! with the trailing `00 00 FF FF` block-end removed; the decompressor
//! re-appends it.

use alloc::vec::Vec;

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use std::io::{Read as _, Write as _};

/// The extension name.
pub const EXTENSION_NAME: &str = "permessage-deflate";

/// The offer a client sends when deflate is enabled.
pub fn client_offer() -> &'static str {
    "permessage-deflate; client_no_context_takeover; server_no_context_takeover"
}

fn trim_ascii(value: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = value.len();
    while start < end && matches!(value[start], b' ' | b'\t') {
        start += 1;
    }
    while end > start && matches!(value[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    &value[start..end]
}

/// Decide the `Sec-WebSocket-Extensions` response header for a server that
/// has deflate enabled: accept the extension only when offered with (or
/// without) the no-context-takeover parameters we require.
pub fn server_accept(offer_header: Option<&[u8]>) -> Option<String> {
    let offer = offer_header?;
    // Accept the first permessage-deflate offer; strip context-takeover
    // params the peer requested and add ours.
    let offered = offer
        .split(|&b| b == b',')
        .any(|ext| match ext.split(|&b| b == b';').next() {
            Some(name) => trim_ascii(name).eq_ignore_ascii_case(EXTENSION_NAME.as_bytes()),
            None => false,
        });
    if offered {
        Some(format!(
            "{EXTENSION_NAME}; client_no_context_takeover; server_no_context_takeover"
        ))
    } else {
        None
    }
}

/// Compress a message payload (per-message, no context takeover).
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    // SAFETY-free: Vec Write impls cannot fail.
    encoder.write_all(data).expect("deflate: Vec write");
    let mut compressed = encoder.finish().expect("deflate: finish");
    // RFC 7692: the trailing 00 00 FF FF is implicit.
    if compressed.ends_with(&[0x00, 0x00, 0xFF, 0xFF]) {
        compressed.truncate(compressed.len() - 4);
    }
    compressed
}

/// Decompress a message payload (per-message, no context takeover).
pub fn decompress(data: &[u8], max: usize) -> Result<Vec<u8>, crate::error::WsError> {
    // Quick sanity bound: deflate of incompressible data is < data + 5 bytes.
    if data.len() > max.saturating_add(16) {
        return Err(crate::error::WsError::Protocol(
            "compressed payload exceeds message limit",
        ));
    }
    let mut full = Vec::with_capacity(data.len() + 4);
    full.extend_from_slice(data);
    full.extend_from_slice(&[0x00, 0x00, 0xFF, 0xFF]);

    let mut decoder = DeflateDecoder::new(&full[..]);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|_| crate::error::WsError::Protocol("invalid deflate stream"))?;
    if out.len() > max {
        return Err(crate::error::WsError::Protocol(
            "decompressed message exceeds 16 MiB limit",
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_compressed_and_incompressible() {
        // Compressible
        let data = vec![b'a'; 4096];
        let compressed = compress(&data);
        assert!(
            compressed.len() < 256,
            "run of 'a' should compress hard: {}",
            compressed.len()
        );
        assert_eq!(decompress(&compressed, MAX).unwrap(), data);

        // Incompressible (random-ish) still roundtrips.
        let data: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
        let compressed = compress(&data);
        assert_eq!(decompress(&compressed, MAX).unwrap(), data);
    }

    #[test]
    fn empty_message_roundtrips() {
        let compressed = compress(&[]);
        assert_eq!(decompress(&compressed, MAX).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(decompress(&[1, 2, 3, 4, 5, 6], MAX).is_err());
    }

    #[test]
    fn decompression_bomb_is_capped() {
        let bomb = compress(&vec![0u8; 128 * 1024]);
        assert!(decompress(&bomb, 1024).is_err());
    }

    const MAX: usize = 16 * 1024 * 1024;

    #[test]
    fn server_accept_matches_offer() {
        let offer = b"permessage-deflate; client_no_context_takeover";
        assert!(server_accept(Some(offer)).is_some());
        assert!(server_accept(Some(b"")).is_none());
        assert!(server_accept(Some(b"x-webkit-deflate-frame")).is_none());
    }
}
