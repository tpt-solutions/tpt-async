// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Async read trait, I/O error type, and extension helpers.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::read_buf::ReadBuf;

// ── IoError ──────────────────────────────────────────────────────────────────

/// A portable I/O error type used throughout the `tpt-async` ecosystem.
///
/// Under the `std` feature this is a thin wrapper around [`std::io::Error`],
/// providing lossless round-trips.  Without `std` it carries an
/// `IoErrorKind` tag and an optional static message.
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
        Self::new(
            std::io::ErrorKind::UnexpectedEof,
            "unexpected end of stream",
        )
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
    /// Unexpected end of stream.
    UnexpectedEof,
    /// Write returned zero bytes.
    WriteZero,
    /// The operation would block.
    WouldBlock,
    /// The write side of a pipe/connection is closed.
    BrokenPipe,
    /// The connection was reset by the peer.
    ConnectionReset,
    /// The connection was aborted locally.
    ConnectionAborted,
    /// The operation timed out.
    TimedOut,
    /// Anything else.
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

// ── AsyncReadExt ──────────────────────────────────────────────────────────────

/// Extension helpers over [`AsyncRead`], mirroring the ergonomics of
/// `futures::io::AsyncReadExt` and `tokio::io::AsyncReadExt`.
///
/// The extension futures are allocation-free except for
/// [`read_to_end`](AsyncReadExt::read_to_end), which requires the `alloc`
/// feature.
pub trait AsyncReadExt: AsyncRead {
    /// Reads whatever bytes are currently available into `buf`, returning
    /// the number of bytes read *this call*.
    ///
    /// Returns `Ok(0)` on end-of-stream.
    ///
    /// The buffer is taken by value: the future reports the byte count
    /// directly (callers index their slice afterwards), and reader/buffer
    /// lifetimes stay independent so both can live in a loop.
    fn read<'a, 'b>(&'a mut self, buf: ReadBuf<'b>) -> Read<'a, 'b, Self>
    where
        Self: Sized + Unpin,
    {
        Read { reader: self, buf }
    }

    /// Reads exactly `buf.len()` bytes into `buf`, or fails with
    /// [`IoError::unexpected_eof`] if the stream ends first.
    fn read_exact<'a, 'b>(&'a mut self, buf: &'b mut [u8]) -> ReadExact<'a, 'b, Self>
    where
        Self: Sized + Unpin,
    {
        ReadExact {
            reader: self,
            buf,
            pos: 0,
        }
    }

    /// Reads until end-of-stream, appending to `out`.
    ///
    /// Requires the `alloc` feature.
    #[cfg(feature = "alloc")]
    fn read_to_end<'a, 'b>(
        &'a mut self,
        out: &'b mut alloc::vec::Vec<u8>,
    ) -> ReadToEnd<'a, 'b, Self>
    where
        Self: Sized + Unpin,
    {
        ReadToEnd { reader: self, out }
    }
}

impl<R: AsyncRead + ?Sized> AsyncReadExt for R {}

/// Future returned by [`AsyncReadExt::read`].
pub struct Read<'a, 'b, R: ?Sized> {
    reader: &'a mut R,
    buf: ReadBuf<'b>,
}

impl<R: AsyncRead + Unpin + ?Sized> Future for Read<'_, '_, R> {
    type Output = Result<usize, IoError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match Pin::new(&mut *this.reader).poll_read(cx, &mut this.buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            // `buf` was created fresh by the caller, so `filled()` is exactly
            // the count for this read; zero bytes means end-of-stream.
            Poll::Ready(Ok(())) => Poll::Ready(Ok(this.buf.filled().len())),
        }
    }
}

/// Future returned by [`AsyncReadExt::read_exact`].
pub struct ReadExact<'a, 'b, R: ?Sized> {
    reader: &'a mut R,
    buf: &'b mut [u8],
    pos: usize,
}

impl<R: AsyncRead + Unpin + ?Sized> Future for ReadExact<'_, '_, R> {
    type Output = Result<(), IoError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            if this.pos >= this.buf.len() {
                return Poll::Ready(Ok(()));
            }
            let mut rb = ReadBuf::new(&mut this.buf[this.pos..]);
            match Pin::new(&mut *this.reader).poll_read(cx, &mut rb) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {
                    let n = rb.filled().len();
                    if n == 0 {
                        return Poll::Ready(Err(IoError::unexpected_eof()));
                    }
                    this.pos += n;
                }
            }
        }
    }
}

/// Future returned by [`AsyncReadExt::read_to_end`].
#[cfg(feature = "alloc")]
pub struct ReadToEnd<'a, 'b, R: ?Sized> {
    reader: &'a mut R,
    out: &'b mut alloc::vec::Vec<u8>,
}

#[cfg(feature = "alloc")]
impl<R: AsyncRead + Unpin + ?Sized> Future for ReadToEnd<'_, '_, R> {
    type Output = Result<(), IoError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        const CHUNK: usize = 4096;
        let this = self.get_mut();
        loop {
            let base = this.out.len();
            this.out.resize(base + CHUNK, 0);
            let mut rb = ReadBuf::new(&mut this.out[base..]);
            match Pin::new(&mut *this.reader).poll_read(cx, &mut rb) {
                Poll::Pending => {
                    this.out.truncate(base);
                    return Poll::Pending;
                }
                Poll::Ready(Err(e)) => {
                    this.out.truncate(base);
                    return Poll::Ready(Err(e));
                }
                Poll::Ready(Ok(())) => {
                    let n = rb.filled().len();
                    this.out.truncate(base + n);
                    if n == 0 {
                        return Poll::Ready(Ok(())); // EOF
                    }
                }
            }
        }
    }
}

// ── legacy tokio blanket bridge (removed) ─────────────────────────────────────
//
// A blanket `impl<T: tokio::io::AsyncRead> AsyncRead for T` conflicts with the
// `&mut T` impl above (E0119) and locks out downstream manual impls. Use the
// explicit [`crate::TokioReader`] wrapper instead.
