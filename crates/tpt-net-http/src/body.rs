// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Message body handling: `Content-Length`, `Transfer-Encoding: chunked`,
//! and read-until-EOF bodies.

use alloc::vec::Vec;

use tpt_async_io::AsyncRead;

use crate::error::HttpError;
use crate::parse::HeaderBlock;

/// How the body length is framed, decided from the head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// No body at all (e.g. responses to HEAD, 204/304, most GET requests).
    Empty,
    /// Exactly `n` bytes.
    ContentLength(u64),
    /// `Transfer-Encoding: chunked`.
    Chunked,
    /// Read until the peer closes (HTTP/1.0-style responses only).
    UntilEof,
}

/// Decide the body framing for a *request* head (RFC 9112 §6.3).
pub fn request_body_kind(headers: &HeaderBlock) -> Result<BodyKind, HttpError> {
    if let Some(te) = headers.get(b"transfer-encoding") {
        // Both TE and CL present is a classic request-smuggling vector:
        // reject instead of applying precedence rules.
        if headers.get(b"content-length").is_some() {
            return Err(HttpError::ConflictingHeaders(
                "both Transfer-Encoding and Content-Length",
            ));
        }
        return if te.eq_ignore_ascii_case(b"chunked") {
            Ok(BodyKind::Chunked)
        } else {
            Err(HttpError::Parse("unsupported Transfer-Encoding"))
        };
    }
    content_length_kind(headers)
}

/// Decide the body framing for a *response* head.
pub fn response_body_kind(
    headers: &HeaderBlock,
    status: u16,
    is_head_request: bool,
) -> Result<BodyKind, HttpError> {
    if status == 204 || status == 304 || (100..200).contains(&status) {
        return Ok(BodyKind::Empty);
    }
    if is_head_request {
        return Ok(BodyKind::Empty);
    }
    if let Some(te) = headers.get(b"transfer-encoding") {
        return if te.eq_ignore_ascii_case(b"chunked") {
            Ok(BodyKind::Chunked)
        } else {
            Err(HttpError::Parse("unsupported Transfer-Encoding"))
        };
    }
    match content_length_kind(headers)? {
        BodyKind::Empty => {
            // No Content-Length: a response body is framed by EOF (the
            // HTTP/1.0 behaviour).
            Ok(BodyKind::UntilEof)
        }
        other => Ok(other),
    }
}

/// Shared `Content-Length` handling: absent → `Empty`; conflicting values →
/// hard error (request-smuggling vector).
fn content_length_kind(headers: &HeaderBlock) -> Result<BodyKind, HttpError> {
    match headers.get(b"content-length") {
        Some(value) => {
            let text = core::str::from_utf8(value)
                .map_err(|_| HttpError::Parse("invalid Content-Length"))?;
            let trimmed = text.trim();
            if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
                return Err(HttpError::Parse("invalid Content-Length"));
            }
            let n: u64 = trimmed
                .parse()
                .map_err(|_| HttpError::Parse("invalid Content-Length"))?;
            Ok(BodyKind::ContentLength(n))
        }
        None => Ok(BodyKind::Empty),
    }
}

/// A streaming reader over a message body.
///
/// It reads from the connection's transport and keeps any bytes that belong
/// to the *next* pipelined message, so keep-alive framing stays intact.
pub struct BodyReader<'c, IO> {
    conn: &'c mut crate::connection::HttpConnection<IO>,
    kind: BodyKind,
    remaining: u64,
    /// Remaining bytes of the current chunk (chunked framing only).
    chunk_remaining: u64,
    finished: bool,
}

impl<'c, IO: AsyncRead + Unpin> BodyReader<'c, IO> {
    pub(crate) fn new(conn: &'c mut crate::connection::HttpConnection<IO>, kind: BodyKind) -> Self {
        let remaining = match kind {
            BodyKind::ContentLength(n) => n,
            _ => 0,
        };
        Self {
            conn,
            kind,
            remaining,
            chunk_remaining: 0,
            finished: matches!(kind, BodyKind::Empty),
        }
    }

    /// Read the next chunk of body bytes into `out`, returning how many were
    /// read.  `Ok(0)` means the body is complete.
    pub async fn read(&mut self, out: &mut [u8]) -> Result<usize, HttpError> {
        if self.finished || out.is_empty() {
            return Ok(0);
        }
        let n = match self.kind {
            BodyKind::Empty => 0,
            BodyKind::ContentLength(_) => self.read_content_length(out).await?,
            BodyKind::Chunked => self.read_chunked(out).await?,
            BodyKind::UntilEof => self.conn.read_raw(out).await?,
        };
        if n == 0 {
            self.finished = true;
        }
        Ok(n)
    }

    /// Read the whole body into a `Vec`.  Convenient for request handlers;
    /// size-capped so a peer cannot exhaust memory.
    pub async fn read_to_vec(&mut self, max: usize) -> Result<Vec<u8>, HttpError> {
        let mut out = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = self.read(&mut chunk).await?;
            if n == 0 {
                return Ok(core::mem::take(&mut out));
            }
            if out.len() + n > max {
                return Err(HttpError::Parse("body exceeds limit"));
            }
            out.extend_from_slice(&chunk[..n]);
        }
    }

    /// `true` once the body has been fully consumed.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    async fn read_content_length(&mut self, out: &mut [u8]) -> Result<usize, HttpError> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let want = (out.len() as u64).min(self.remaining) as usize;
        let n = self.conn.read_raw(&mut out[..want]).await?;
        if n == 0 {
            return Err(HttpError::ConnectionClosed);
        }
        self.remaining -= n as u64;
        Ok(n)
    }

    async fn read_chunked(&mut self, out: &mut [u8]) -> Result<usize, HttpError> {
        loop {
            if self.chunk_remaining > 0 {
                let want = (out.len() as u64).min(self.chunk_remaining) as usize;
                let n = self.conn.read_raw(&mut out[..want]).await?;
                if n == 0 {
                    return Err(HttpError::ConnectionClosed);
                }
                self.chunk_remaining -= n as u64;
                if self.chunk_remaining == 0 {
                    // Consume the CRLF that terminates every chunk.
                    self.conn.expect_exact(b"\r\n").await?;
                }
                return Ok(n);
            }
            // Start of a new chunk: read its size line.
            let line = self.conn.read_line().await?;
            let size_part = line.split(|&b| b == b';').next().unwrap_or(&line);
            let text = core::str::from_utf8(size_part)
                .map_err(|_| HttpError::Parse("invalid chunk size"))?
                .trim();
            let size = u64::from_str_radix(text, 16)
                .map_err(|_| HttpError::Parse("invalid chunk size"))?;
            if size == 0 {
                // Trailer section: lines until a blank line (commonly empty).
                loop {
                    let trailer = self.conn.read_line().await?;
                    if trailer.is_empty() {
                        break;
                    }
                }
                self.finished = true;
                return Ok(0);
            }
            self.chunk_remaining = size;
        }
    }
}
