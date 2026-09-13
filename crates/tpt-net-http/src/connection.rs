// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTP/1.1 connection framing shared by the client and the server.
//!
//! [`HttpConnection`] owns the transport and one read buffer.  A parsed head
//! is *frozen*: the buffer moves into an `Arc` and the parsed views are byte
//! ranges into it, so headers are never copied per-field while remaining
//! fully safe (the `Arc` keeps the storage alive).

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::pin::Pin;

use tpt_async_io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};

use crate::error::HttpError;
use crate::parse::{parse_request_ranges, parse_response_ranges, RequestHead, ResponseHead};

const READ_CHUNK: usize = 8192;

/// An HTTP/1.1 connection over any async transport.
pub struct HttpConnection<IO> {
    pub(crate) io: IO,
    /// Unconsumed bytes read from the transport (head + possibly early body
    /// or pipelined bytes).
    buf: Vec<u8>,
    /// Set once the peer has closed (a second read returning 0 is an error,
    /// not a clean close, in keep-alive framing).
    eof: bool,
}

impl<IO> HttpConnection<IO> {
    /// Wrap a transport.
    pub fn new(io: IO) -> Self {
        Self {
            io,
            buf: Vec::new(),
            eof: false,
        }
    }

    /// Unwrap the transport (drains any internal state first).
    pub fn into_inner(self) -> IO {
        self.io
    }
}

impl<IO: AsyncRead + Unpin> HttpConnection<IO> {
    /// Read some bytes into the internal buffer.  Returns bytes read; `0` on
    /// peer EOF.
    pub(crate) async fn fill(&mut self) -> Result<usize, HttpError> {
        let old_len = self.buf.len();
        self.buf.resize(old_len + READ_CHUNK, 0);
        let mut view = ReadBuf::new(&mut self.buf[old_len..]);
        let result = {
            let io = &mut self.io;
            std::future::poll_fn(|cx| AsyncRead::poll_read(Pin::new(io), cx, &mut view)).await
        };
        let n = view.filled().len();
        self.buf.truncate(old_len + n);
        match result {
            Ok(()) if n == 0 => {
                self.eof = true;
                Ok(0)
            }
            Ok(()) => Ok(n),
            Err(e) => Err(HttpError::Io(e)),
        }
    }

    /// Read raw bytes from the connection (buffered leftovers first, then
    /// the transport).  Public so protocol upgrades (e.g. WebSocket) built
    /// on this connection can keep using the framing buffer.
    pub async fn read_raw(&mut self, out: &mut [u8]) -> Result<usize, HttpError> {
        if !self.buf.is_empty() {
            let n = out.len().min(self.buf.len());
            out[..n].copy_from_slice(&self.buf[..n]);
            self.buf.drain(..n);
            return Ok(n);
        }
        // Buffer empty: read straight into the caller's slice.
        let mut view = ReadBuf::new(out);
        let result = {
            let io = &mut self.io;
            std::future::poll_fn(|cx| AsyncRead::poll_read(Pin::new(io), cx, &mut view)).await
        };
        result?;
        Ok(view.filled().len())
    }

    /// Read one CRLF-terminated line (returned without the CRLF).
    pub(crate) async fn read_line(&mut self) -> Result<Vec<u8>, HttpError> {
        let mut line = Vec::new();
        loop {
            if let Some(pos) = self.buf.windows(2).position(|w| w == b"\r\n") {
                line.extend_from_slice(&self.buf[..pos]);
                self.buf.drain(..pos + 2);
                return Ok(line);
            }
            if self.buf.is_empty() && self.eof {
                return Err(HttpError::ConnectionClosed);
            }
            if self.fill().await? == 0 {
                return Err(HttpError::ConnectionClosed);
            }
        }
    }

    /// Assert the next bytes are exactly `expected`.
    pub(crate) async fn expect_exact(&mut self, expected: &[u8]) -> Result<(), HttpError> {
        let mut got = vec![0u8; expected.len()];
        // Buffered-first read loop until we have the exact count.
        let mut filled = 0;
        while filled < expected.len() {
            let n = self.read_raw(&mut got[filled..]).await?;
            if n == 0 {
                return Err(HttpError::ConnectionClosed);
            }
            filled += n;
        }
        if got != expected {
            return Err(HttpError::Parse("invalid framing bytes"));
        }
        Ok(())
    }

    /// Read and freeze a request head.  `Ok(None)` = clean EOF before any
    /// byte (the polite end of a keep-alive connection).
    pub async fn read_request_head(&mut self) -> Result<Option<RequestHead>, HttpError> {
        loop {
            // Take the buffer so the frozen head can own it as an `Arc`.
            let mut vec = core::mem::take(&mut self.buf);
            match parse_request_ranges(&vec)? {
                Some(ranges) => {
                    // Anything after the head (early body / pipelined bytes)
                    // stays with the connection.  Split before moving `vec`
                    // into the `Arc`; `ranges` borrows nothing.
                    let consumed = ranges.consumed;
                    let tail = vec.split_off(consumed);
                    let head = RequestHead::from_ranges(Arc::new(vec), &ranges);
                    self.buf = tail;
                    return Ok(Some(head));
                }
                None => {
                    self.buf = vec;
                }
            }
            if self.fill().await? == 0 {
                if self.buf.is_empty() {
                    return Ok(None); // clean keep-alive close
                }
                return Err(HttpError::ConnectionClosed);
            }
        }
    }

    /// Read and freeze a response head.
    pub async fn read_response_head(&mut self) -> Result<ResponseHead, HttpError> {
        loop {
            let mut vec = core::mem::take(&mut self.buf);
            match parse_response_ranges(&vec)? {
                Some(ranges) => {
                    let consumed = ranges.consumed;
                    let tail = vec.split_off(consumed);
                    let head = ResponseHead::from_ranges(Arc::new(vec), &ranges);
                    self.buf = tail;
                    return Ok(head);
                }
                None => {
                    self.buf = vec;
                }
            }
            if self.fill().await? == 0 {
                return Err(HttpError::ConnectionClosed);
            }
        }
    }

    /// Decide the framing of a response body and build its reader.
    pub(crate) fn response_body<'c>(
        &'c mut self,
        head: &ResponseHead,
        is_head_request: bool,
    ) -> Result<crate::body::BodyReader<'c, IO>, HttpError> {
        let kind = crate::body::response_body_kind(head.headers(), head.status(), is_head_request)?;
        Ok(crate::body::BodyReader::new(self, kind))
    }
}

impl<IO: AsyncWrite + Unpin> HttpConnection<IO> {
    /// Write raw bytes (used by protocol upgrades, e.g. WebSocket).
    pub async fn write_raw(&mut self, bytes: &[u8]) -> Result<(), HttpError> {
        self.io.write_all(bytes).await.map_err(HttpError::Io)?;
        self.io.flush().await.map_err(HttpError::Io)?;
        Ok(())
    }

    /// Write a raw head + optional framed body in one go.
    pub async fn write_message(&mut self, head: &str, body: &[u8]) -> Result<(), HttpError> {
        self.io
            .write_all(head.as_bytes())
            .await
            .map_err(HttpError::Io)?;
        if !body.is_empty() {
            self.io.write_all(body).await.map_err(HttpError::Io)?;
        }
        self.io.flush().await.map_err(HttpError::Io)?;
        Ok(())
    }
}

/// Render a status line + headers into wire format.
pub(crate) fn render_head(
    status: u16,
    reason: &str,
    headers: &[(String, String)],
    include_content_length: bool,
    body_len: usize,
    extra_first_line: String,
) -> String {
    let mut head = extra_first_line;
    head.push_str("HTTP/1.1 ");
    head.push_str(&status.to_string());
    head.push(' ');
    head.push_str(reason);
    head.push_str("\r\n");
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    if include_content_length {
        head.push_str("content-length: ");
        head.push_str(&body_len.to_string());
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    head
}
