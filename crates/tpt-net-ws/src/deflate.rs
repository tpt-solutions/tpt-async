// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `permessage-deflate` (RFC 7692) compression support.
//!
//! Each connection owns a [`Codec`] with one compressor and one decompressor,
//! so compression context carries across messages unless a
//! `no_context_takeover` parameter was negotiated for that direction.
//!
//! Wire format per RFC 7692 §7.2.1: the payload is a raw DEFLATE stream with
//! the trailing `00 00 FF FF` sync marker removed; the decompressor re-appends
//! it.  A compressed message spans all of its fragments, so it is decompressed
//! once, after assembly.

use alloc::vec::Vec;

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress};

use crate::error::WsError;

/// The extension name.
pub const EXTENSION_NAME: &str = "permessage-deflate";

/// The trailing block that RFC 7692 strips from every compressed message.
const SYNC_TAIL: [u8; 4] = [0x00, 0x00, 0xFF, 0xFF];

/// The offer a client sends when deflate is enabled.  Context takeover is
/// allowed in both directions, so the server decides what it commits to.
pub fn client_offer() -> &'static str {
    EXTENSION_NAME
}

/// Per-connection `permessage-deflate` state.
pub struct Codec {
    compressor: Compress,
    decompressor: Decompress,
    /// Start a fresh compressor for every message (`*_no_context_takeover`).
    reset_compressor: bool,
    /// Start a fresh decompressor for every message (peer committed to
    /// `no_context_takeover`).
    reset_decompressor: bool,
}

impl Codec {
    /// Create a codec with the default 15-bit window.
    pub fn new(reset_compressor: bool, reset_decompressor: bool) -> Self {
        Self {
            compressor: Compress::new(Compression::default(), false),
            decompressor: Decompress::new(false),
            reset_compressor,
            reset_decompressor,
        }
    }

    /// The server's codec once it has accepted an offer: both directions
    /// commit to `no_context_takeover` (see [`server_accept`]).
    pub fn server_default() -> Self {
        Self::new(true, true)
    }

    /// Compress one complete message payload.
    pub fn compress(&mut self, data: &[u8]) -> Result<Vec<u8>, WsError> {
        let start = self.compressor.total_in();
        let mut out = Vec::with_capacity(data.len() / 2 + 64);
        let mut consumed = 0usize;
        loop {
            out.reserve(256);
            self.compressor
                .compress_vec(&data[consumed..], &mut out, FlushCompress::Sync)
                .map_err(|_| WsError::Protocol("deflate compression failed"))?;
            consumed = (self.compressor.total_in() - start) as usize;
            // Sync flush is complete once all input is consumed and zlib left
            // room in the output buffer.
            if consumed == data.len() && out.len() < out.capacity() {
                break;
            }
        }
        if out.ends_with(&SYNC_TAIL) {
            out.truncate(out.len() - SYNC_TAIL.len());
        }
        if self.reset_compressor {
            self.compressor.reset();
        }
        Ok(out)
    }

    /// Decompress one complete message payload, refusing output above `max`.
    pub fn decompress(&mut self, data: &[u8], max: usize) -> Result<Vec<u8>, WsError> {
        let mut input = Vec::with_capacity(data.len() + SYNC_TAIL.len());
        input.extend_from_slice(data);
        input.extend_from_slice(&SYNC_TAIL);

        let start = self.decompressor.total_in();
        let mut out = Vec::new();
        let mut consumed = 0usize;
        loop {
            out.reserve(4096);
            let (before_in, before_out) = (consumed, out.len());
            let status = self
                .decompressor
                .decompress_vec(&input[consumed..], &mut out, FlushDecompress::Sync)
                .map_err(|_| WsError::Protocol("invalid deflate stream"))?;
            consumed = (self.decompressor.total_in() - start) as usize;
            if out.len() > max {
                return Err(WsError::Protocol(
                    "decompressed message exceeds 16 MiB limit",
                ));
            }
            if consumed == input.len() && out.len() < out.capacity() {
                break;
            }
            // No progress with input left over means the stream is corrupt.
            if consumed == before_in
                && out.len() == before_out
                && matches!(status, flate2::Status::BufError)
            {
                return Err(WsError::Protocol("invalid deflate stream"));
            }
        }
        if self.reset_decompressor {
            self.decompressor.reset(false);
        }
        Ok(out)
    }
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

/// Parse a server's `Sec-WebSocket-Extensions` response and build the
/// client's codec for it.  `Ok(None)` when the server did not accept
/// permessage-deflate.
///
/// A smaller compressor window (`client_max_window_bits` below 15) is refused:
/// the codec cannot honour it, and the client never offered it.
pub fn client_codec(response: &[u8]) -> Result<Option<Codec>, WsError> {
    for extension in response.split(|&b| b == b',') {
        let mut parts = extension.split(|&b| b == b';');
        let Some(name) = parts.next().map(trim_ascii) else {
            continue;
        };
        if !name.eq_ignore_ascii_case(EXTENSION_NAME.as_bytes()) {
            continue;
        }
        let (mut reset_compressor, mut reset_decompressor) = (false, false);
        for param in parts {
            let param = trim_ascii(param);
            let (key, value) = match param.iter().position(|&b| b == b'=') {
                Some(eq) => (trim_ascii(&param[..eq]), Some(trim_ascii(&param[eq + 1..]))),
                None => (param, None),
            };
            if key.eq_ignore_ascii_case(b"server_no_context_takeover") {
                reset_decompressor = true;
            } else if key.eq_ignore_ascii_case(b"client_no_context_takeover") {
                reset_compressor = true;
            } else if key.eq_ignore_ascii_case(b"client_max_window_bits") {
                let bits = value
                    .and_then(|v| core::str::from_utf8(v).ok())
                    .and_then(|v| v.parse::<u8>().ok());
                if matches!(bits, Some(b) if b < 15) {
                    return Err(WsError::Handshake(
                        "permessage-deflate client_max_window_bits below 15 is unsupported",
                    ));
                }
            }
        }
        return Ok(Some(Codec::new(reset_compressor, reset_decompressor)));
    }
    Ok(None)
}

/// Decide the `Sec-WebSocket-Extensions` response header for a server that
/// has deflate enabled.  The server always commits to
/// `no_context_takeover` in both directions, so its codec is
/// [`Codec::server_default`].
pub fn server_accept(offer_header: Option<&[u8]>) -> Option<alloc::string::String> {
    let offer = offer_header?;
    let offered = offer.split(|&b| b == b',').any(|ext| {
        ext.split(|&b| b == b';')
            .next()
            .map(|name| trim_ascii(name).eq_ignore_ascii_case(EXTENSION_NAME.as_bytes()))
            .unwrap_or(false)
    });
    if offered {
        Some(alloc::format!(
            "{EXTENSION_NAME}; client_no_context_takeover; server_no_context_takeover"
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = 16 * 1024 * 1024;

    fn pair() -> (Codec, Codec) {
        (Codec::new(false, false), Codec::new(false, false))
    }

    #[test]
    fn roundtrip_compressible_and_incompressible() {
        let (mut tx, mut rx) = pair();
        let data = vec![b'a'; 4096];
        let compressed = tx.compress(&data).unwrap();
        assert!(
            compressed.len() < 256,
            "run of 'a' should compress hard: {}",
            compressed.len()
        );
        assert_eq!(rx.decompress(&compressed, MAX).unwrap(), data);

        let data: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
        let compressed = tx.compress(&data).unwrap();
        assert_eq!(rx.decompress(&compressed, MAX).unwrap(), data);
    }

    #[test]
    fn context_carries_across_messages() {
        // The second message repeats the first, so with shared context it
        // must be far smaller than the first.
        let (mut tx, mut rx) = pair();
        let text = b"the quick brown fox jumps over the lazy dog ".repeat(40);
        let first = tx.compress(&text).unwrap();
        let second = tx.compress(&text).unwrap();
        assert!(
            second.len() < first.len() / 2,
            "{} vs {}",
            second.len(),
            first.len()
        );
        assert_eq!(rx.decompress(&first, MAX).unwrap(), text);
        assert_eq!(rx.decompress(&second, MAX).unwrap(), text);
    }

    #[test]
    fn no_context_takeover_resets_each_message() {
        let mut tx = Codec::new(true, false);
        let mut rx = Codec::new(false, true);
        let text = b"the quick brown fox jumps over the lazy dog ".repeat(40);
        let first = tx.compress(&text).unwrap();
        let second = tx.compress(&text).unwrap();
        assert_eq!(first, second, "each message compresses from a fresh state");
        assert_eq!(rx.decompress(&first, MAX).unwrap(), text);
        assert_eq!(rx.decompress(&second, MAX).unwrap(), text);
    }

    #[test]
    fn empty_message_roundtrips() {
        let (mut tx, mut rx) = pair();
        let compressed = tx.compress(&[]).unwrap();
        assert_eq!(rx.decompress(&compressed, MAX).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn garbage_is_rejected() {
        let (_, mut rx) = pair();
        assert!(rx.decompress(&[1, 2, 3, 4, 5, 6], MAX).is_err());
    }

    #[test]
    fn decompression_bomb_is_capped() {
        let (mut tx, mut rx) = pair();
        let bomb = tx.compress(&vec![0u8; 128 * 1024]).unwrap();
        assert!(rx.decompress(&bomb, 1024).is_err());
    }

    #[test]
    fn client_codec_reads_response_parameters() {
        assert!(client_codec(b"x-webkit-deflate-frame").unwrap().is_none());
        let plain = client_codec(b"permessage-deflate").unwrap().unwrap();
        assert!(!plain.reset_compressor && !plain.reset_decompressor);
        let strict = client_codec(b"permessage-deflate; server_no_context_takeover")
            .unwrap()
            .unwrap();
        assert!(strict.reset_decompressor && !strict.reset_compressor);
        let forced = client_codec(b"permessage-deflate; client_no_context_takeover")
            .unwrap()
            .unwrap();
        assert!(forced.reset_compressor && !forced.reset_decompressor);
    }

    #[test]
    fn client_codec_refuses_small_compressor_window() {
        assert!(client_codec(b"permessage-deflate; client_max_window_bits=10").is_err());
        assert!(
            client_codec(b"permessage-deflate; client_max_window_bits=15")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn server_accept_matches_offer() {
        let offer = b"permessage-deflate; client_no_context_takeover";
        assert!(server_accept(Some(offer)).is_some());
        assert!(server_accept(Some(b"")).is_none());
        assert!(server_accept(Some(b"x-webkit-deflate-frame")).is_none());
    }
}
