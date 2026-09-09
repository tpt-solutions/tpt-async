// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Async read trait and I/O error type.

use core::pin::Pin;
use core::task::{Context, Poll};

use crate::read_buf::ReadBuf;

// ── IoError ──────────────────────────────────────────────────────────────────

/// A portable I/O error type used throughout the `tpt-async` ecosystem.
///
/// Under the `std` feature this is a thin wrapper around [`std::io::Error`],
/// providing lossless round-trips.  Without `std` it carries an
/// [`IoErrorKind`] tag and an optional static message.
#[derive(Debug)]
pub struct IoError {
    #[cfg(feature = "std")]
    inner: std::io::Error,

    #[cfg(not(feature = "std"))]
    kind: IoErrorKind,
    #[cfg(not(feature = "std"))]
    msg: &'static str,
}

impl IoError {
    /// Creates an `IoError` with the given kind and a static message.
    #[cfg(feature = "std")]
    pub fn new(kind: std::io::ErrorKind, msg: &'static str) -> Self {
        Self {
            inner: std::io::Error::new(kind, msg),
        }
    }

    /// Returns the OS / kind classification of this error.
    #[cfg(feature = "std")]
    pub fn kind(&self) -> std::io::ErrorKind {
        self.inner.kind()
    }

    /// Shorthand: unexpected end of stream.
    #[cfg(feature = "std")]
    pub fn unexpected_eof() -> Self {
        Self::new(std::io::ErrorKind::UnexpectedEof, "unexpected end of stream")
    }

    /// Shorthand: write returned zero bytes.
    #[cfg(feature = "std")]
    pub fn write_zero() -> Self {
        Self::new(std::io::ErrorKind::WriteZero, "write returned 0 bytes")
    }
}

#[cfg(not(feature = "std"))]
impl IoError {
    /// Creates an `IoError` without std.
    pub fn new(kind: IoErrorKind, msg: &'static str) -> Self {
        Self { kind, msg }
    }

    /// Returns the kind of this error.
    pub fn kind(&self) -> IoErrorKind {
        self.kind
    }

    /// Shorthand: unexpected end of stream.
    pub fn unexpected_eof() -> Self {
        Self::new(IoErrorKind::UnexpectedEof, "unexpected end of stream")
    }

    /// Shorthand: write returned zero bytes.
    pub fn write_zero() -> Self {
        Self::new(IoErrorKind::WriteZero, "write returned 0 bytes")
    }
}

#[cfg(feature = "std")]
impl core::fmt::Display for IoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.inner.fmt(f)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source()
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        Self { inner: e }
    }
}

#[cfg(feature = "std")]
impl From<IoError> for std::io::Error {
    fn from(e: IoError) -> Self {
        e.inner
    }
}

// ── IoErrorKind (no_std) ─────────────────────────────────────────────────────

/// Error classification for `IoError` in `no_std` environments.
#[cfg(not(feature = "std"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoErrorKind {
    UnexpectedEof,
    WriteZero,
    WouldBlock,
    BrokenPipe,
    ConnectionReset,
    ConnectionAborted,
    TimedOut,
    Other,
}

// ── AsyncRead ─────────────────────────────────────────────────────────────────

/// Trait for types that can be read from asynchronously.
///
/// The design mirrors [`tokio::io::AsyncRead`]: the implementor writes into
/// the *unfilled* portion of `buf` and calls [`ReadBuf::advance`] to record
/// how many bytes were filled.  On success the future resolves to `Ok(())`
/// regardless of how many bytes were filled; callers check
/// [`ReadBuf::filled`] to discover the count.
///
/// A result of `Ok(())` with zero bytes filled signals end-of-stream.
pub trait AsyncRead {
    /// Attempt to read bytes from this source into `buf`.
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>>;
}

/// Blanket impl for pinned mutable references.
impl<T: AsyncRead + Unpin + ?Sized> AsyncRead for &mut T {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        Pin::new(&mut **Pin::into_inner(self)).poll_read(cx, buf)
    }
}

// ── tokio bridge (optional) ───────────────────────────────────────────────────

#[cfg(feature = "tokio")]
mod tokio_bridge_read {
    use super::*;

    /// Blanket impl: any `tokio::io::AsyncRead + Unpin` type becomes an
    /// `AsyncRead`.
    impl<T> AsyncRead for T
    where
        T: tokio::io::AsyncRead + Unpin,
    {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<Result<(), IoError>> {
            let mut tbuf = tokio::io::ReadBuf::new(buf.unfilled());
            let result = tokio::io::AsyncRead::poll_read(self, cx, &mut tbuf);
            let filled = tbuf.filled().len();
            match result {
                Poll::Ready(Ok(())) => {
                    buf.advance(filled);
                    Poll::Ready(Ok(()))
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(IoError::from(e))),
                Poll::Pending => Poll::Pending,
            }
        }
    }
}
