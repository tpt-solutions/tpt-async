// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adapters between `tokio::io` traits and the `tpt-async-io` traits.

use core::pin::Pin;
use core::task::{Context, Poll};

use crate::read::AsyncRead;
use crate::read_buf::ReadBuf;
use crate::write::AsyncWrite;
use crate::IoError;

// ---------------------------------------------------------------------------
// TokioReader
// ---------------------------------------------------------------------------

/// Wraps a [`tokio::io::AsyncRead`] into an [`AsyncRead`].
///
/// The `ReadBuf` types are bridged by mapping from our `ReadBuf` to
/// `tokio::io::ReadBuf` over the same underlying slice, so no extra
/// allocation or copy occurs.
pub struct TokioReader<R>(pub R);

impl<R> TokioReader<R> {
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

impl<R: tokio::io::AsyncRead + Unpin> AsyncRead for TokioReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        // Map our ReadBuf's unfilled slice to a tokio ReadBuf.
        // tokio::io::ReadBuf::new treats the slice as fully initialized,
        // which matches our ReadBuf contract (buf is always &mut [u8]).
        let n = {
            let unfilled = buf.unfilled();
            let mut tokio_buf = tokio::io::ReadBuf::new(unfilled);
            match tokio::io::AsyncRead::poll_read(inner, cx, &mut tokio_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(IoError::from(e))),
                Poll::Ready(Ok(())) => tokio_buf.filled().len(),
            }
        };
        buf.advance(n);
        Poll::Ready(Ok(()))
    }
}

// ---------------------------------------------------------------------------
// TokioWriter
// ---------------------------------------------------------------------------

/// Wraps a [`tokio::io::AsyncWrite`] into an [`AsyncWrite`].
pub struct TokioWriter<W>(pub W);

impl<W> TokioWriter<W> {
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

impl<W: tokio::io::AsyncWrite + Unpin> AsyncWrite for TokioWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        tokio::io::AsyncWrite::poll_write(inner, cx, buf).map(|r| r.map_err(IoError::from))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        tokio::io::AsyncWrite::poll_flush(inner, cx).map(|r| r.map_err(IoError::from))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        // tokio uses `poll_shutdown` as the close/shutdown operation.
        tokio::io::AsyncWrite::poll_shutdown(inner, cx).map(|r| r.map_err(IoError::from))
    }
}

// ---------------------------------------------------------------------------
// TokioCompat (combined)
// ---------------------------------------------------------------------------

/// Wraps a tokio stream (implementing both [`tokio::io::AsyncRead`] and
/// [`tokio::io::AsyncWrite`]) into a type implementing **both**
/// [`AsyncRead`] and [`AsyncWrite`] — the
/// adapter to reach for when a protocol stack needs one full-duplex object
/// (TLS, HTTP, WebSocket connections).
pub struct TokioCompat<T>(pub T);

impl<T> TokioCompat<T> {
    /// Wraps `inner`.
    pub fn new(inner: T) -> Self {
        Self(inner)
    }

    /// Unwraps the inner stream.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Borrows the inner stream.
    pub fn get_ref(&self) -> &T {
        &self.0
    }

    /// Mutably borrows the inner stream.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: tokio::io::AsyncRead + Unpin> AsyncRead for TokioCompat<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        // Scope the borrow of `buf` so `advance` can run after the poll.
        let n = {
            let unfilled = buf.unfilled();
            let mut tokio_buf = tokio::io::ReadBuf::new(unfilled);
            match tokio::io::AsyncRead::poll_read(inner, cx, &mut tokio_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(IoError::from(e))),
                Poll::Ready(Ok(())) => tokio_buf.filled().len(),
            }
        };
        buf.advance(n);
        Poll::Ready(Ok(()))
    }
}

impl<T: tokio::io::AsyncWrite + Unpin> AsyncWrite for TokioCompat<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        tokio::io::AsyncWrite::poll_write(inner, cx, buf).map(|r| r.map_err(IoError::from))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        tokio::io::AsyncWrite::poll_flush(inner, cx).map(|r| r.map_err(IoError::from))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        tokio::io::AsyncWrite::poll_shutdown(inner, cx).map(|r| r.map_err(IoError::from))
    }
}
