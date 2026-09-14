// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adapters from `embedded-io-async` traits to the `tpt-async-io` traits.
//!
//! Bridges embedded async I/O (embassy-style implementations of
//! `embedded_io_async::Read`/`Write`) into the `tpt-async` trait world so
//! higher layers work unchanged on microcontrollers.
//!
//! # Requirements
//!
//! - The `alloc` feature: `embedded-io-async` traits are RPITIT-based
//!   (async fn), so bridging to poll-based traits boxes the futures.
//! - No-alloc bare-metal targets should implement `tpt_async_io::AsyncRead`
//!   / `AsyncWrite` directly on the hardware driver (the embassy pattern) —
//!   it is three small methods and costs nothing.

use alloc::boxed::Box;

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::read::AsyncRead;
use crate::read_buf::ReadBuf;
use crate::write::AsyncWrite;
use crate::IoError;

/// Wraps an `embedded_io_async` stream into a full-duplex
/// [`AsyncRead`] + [`AsyncWrite`].
///
/// HAL errors are mapped to [`IoError`] with their `Debug` rendering when
/// the `std` feature is on, and a generic `Other` kind otherwise.
pub struct EmbeddedIo<S> {
    inner: S,
}

impl<S> EmbeddedIo<S> {
    /// Wraps `inner`.
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    /// Unwraps the inner stream.
    pub fn into_inner(self) -> S {
        self.inner
    }

    /// Borrows the inner stream.
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Mutably borrows the inner stream.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}

fn hal_error() -> IoError {
    #[cfg(feature = "std")]
    {
        IoError::new(std::io::ErrorKind::Other, "embedded-io error")
    }
    #[cfg(not(feature = "std"))]
    {
        IoError::new(crate::read::IoErrorKind::Other, "embedded-io error")
    }
}

impl<S> AsyncRead for EmbeddedIo<S>
where
    S: embedded_io_async::Read + Unpin + 'static,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let n = {
            let unfilled = buf.unfilled();
            let mut fut = Box::pin(self.get_mut().inner.read(unfilled));
            match fut.as_mut().poll(cx) {
                Poll::Ready(Ok(n)) => n,
                Poll::Ready(Err(_)) => return Poll::Ready(Err(hal_error())),
                Poll::Pending => return Poll::Pending,
            }
        };
        if n > 0 {
            buf.advance(n);
        }
        Poll::Ready(Ok(()))
    }
}

impl<S> AsyncWrite for EmbeddedIo<S>
where
    S: embedded_io_async::Write + Unpin + 'static,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let mut fut = Box::pin(self.get_mut().inner.write(buf));
        match fut.as_mut().poll(cx) {
            Poll::Ready(Ok(n)) => Poll::Ready(Ok(n)),
            Poll::Ready(Err(_)) => Poll::Ready(Err(hal_error())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let mut fut = Box::pin(self.get_mut().inner.flush());
        match fut.as_mut().poll(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(_)) => Poll::Ready(Err(hal_error())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let mut fut = Box::pin(self.get_mut().inner.flush());
        match fut.as_mut().poll(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(_)) => Poll::Ready(Err(hal_error())),
            Poll::Pending => Poll::Pending,
        }
    }
}
