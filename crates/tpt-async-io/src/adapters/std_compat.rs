// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adapters from `std::io` traits to the `tpt-async-io` traits.
//!
//! # Blocking I/O caveat
//!
//! [`StdReader`] and [`StdWriter`] call the underlying `std::io` trait methods
//! synchronously inside `poll_*`.  This is correct **only** when the wrapped
//! type is non-blocking (e.g. a `TcpStream` set to non-blocking mode) or when
//! the caller can accept a blocking executor stall (e.g. in tests using an
//! in-memory `Cursor`).  For blocking file descriptors, offload the I/O to a
//! thread pool instead.  Note that a `Poll::Pending` returned here carries no
//! waker registration — whoever drives the poll loop is responsible for
//! re-polling.

use core::pin::Pin;
use core::task::{Context, Poll};

use crate::read::AsyncRead;
use crate::read_buf::ReadBuf;
use crate::write::AsyncWrite;
use crate::IoError;

// ---------------------------------------------------------------------------
// StdReader
// ---------------------------------------------------------------------------

/// Wraps a [`std::io::Read`] into an [`AsyncRead`].
///
/// See the module-level documentation for the non-blocking caveat.
pub struct StdReader<R>(pub R);

impl<R> StdReader<R> {
    /// Wraps `inner`.
    pub fn new(inner: R) -> Self {
        Self(inner)
    }

    /// Unwraps the inner reader.
    pub fn into_inner(self) -> R {
        self.0
    }

    /// Borrows the inner reader.
    pub fn get_ref(&self) -> &R {
        &self.0
    }

    /// Mutably borrows the inner reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.0
    }
}

impl<R: std::io::Read + Unpin> AsyncRead for StdReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        let this = self.get_mut();
        match this.0.read(buf.unfilled()) {
            Ok(n) => {
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
            Err(e) => Poll::Ready(Err(IoError::from(e))),
        }
    }
}

// ---------------------------------------------------------------------------
// StdWriter
// ---------------------------------------------------------------------------

/// Wraps a [`std::io::Write`] into an [`AsyncWrite`].
///
/// See the module-level documentation for the non-blocking caveat.
pub struct StdWriter<W>(pub W);

impl<W> StdWriter<W> {
    /// Wraps `inner`.
    pub fn new(inner: W) -> Self {
        Self(inner)
    }

    /// Unwraps the inner writer.
    pub fn into_inner(self) -> W {
        self.0
    }

    /// Borrows the inner writer.
    pub fn get_ref(&self) -> &W {
        &self.0
    }

    /// Mutably borrows the inner writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.0
    }
}

impl<W: std::io::Write + Unpin> AsyncWrite for StdWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let this = self.get_mut();
        match this.0.write(buf) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
            Err(e) => Poll::Ready(Err(IoError::from(e))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let this = self.get_mut();
        match this.0.flush() {
            Ok(()) => Poll::Ready(Ok(())),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
            Err(e) => Poll::Ready(Err(IoError::from(e))),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        // `std::io::Write` has no explicit close; flushing is the closest
        // equivalent.
        self.poll_flush(cx)
    }
}
