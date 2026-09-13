// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adapters between async-std's I/O traits and the `tpt-async-io` traits.

use core::pin::Pin;
use core::task::{Context, Poll};

use crate::read::AsyncRead;
use crate::read_buf::ReadBuf;
use crate::write::AsyncWrite;
use crate::IoError;

// ---------------------------------------------------------------------------
// AsyncStdReader
// ---------------------------------------------------------------------------

/// Wraps an [`async_std::io::Read`] into an [`AsyncRead`].
pub struct AsyncStdReader<R>(pub R);

impl<R> AsyncStdReader<R> {
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

impl<R: async_std::io::Read + Unpin> AsyncRead for AsyncStdReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        match async_std::io::Read::poll_read(inner, cx, buf.unfilled()) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(0)) => Poll::Ready(Ok(())), // EOF: nothing filled
            Poll::Ready(Ok(n)) => {
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(IoError::from(e))),
        }
    }
}

// ---------------------------------------------------------------------------
// AsyncStdWriter
// ---------------------------------------------------------------------------

/// Wraps an [`async_std::io::Write`] into an [`AsyncWrite`].
pub struct AsyncStdWriter<W>(pub W);

impl<W> AsyncStdWriter<W> {
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

impl<W: async_std::io::Write + Unpin> AsyncWrite for AsyncStdWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        async_std::io::Write::poll_write(inner, cx, buf).map(|r| r.map_err(IoError::from))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        async_std::io::Write::poll_flush(inner, cx).map(|r| r.map_err(IoError::from))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        // async-std uses `poll_close` as the shutdown operation.
        async_std::io::Write::poll_close(inner, cx).map(|r| r.map_err(IoError::from))
    }
}
