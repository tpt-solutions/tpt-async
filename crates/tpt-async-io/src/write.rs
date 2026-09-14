// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Async write trait and extension helpers.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::read::IoError;

/// Trait for types that can be written to asynchronously.
///
/// The design mirrors [`tokio::io::AsyncWrite`] closely so that bridging
/// adapters can be written with minimal glue.
///
/// Implementations must not return `Ok(0)` from [`poll_write`] unless `buf`
/// is empty; a zero-byte write with a non-empty buffer is treated as a
/// protocol error by the [`AsyncWriteExt`] helpers.
///
/// [`poll_write`]: AsyncWrite::poll_write
pub trait AsyncWrite {
    /// Attempts to write bytes from `buf` into this sink.
    ///
    /// On success, returns the number of bytes consumed from `buf`
    /// (which may be less than `buf.len()`).
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>>;

    /// Attempts to flush any buffered writes to the underlying sink.
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>>;

    /// Initiates or attempts to complete a graceful shutdown.
    ///
    /// After `poll_shutdown` returns `Ok(())`, no further writes may be
    /// issued.
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>>;
}

/// Blanket impl for pinned mutable references.
impl<T: AsyncWrite + Unpin + ?Sized> AsyncWrite for &mut T {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        Pin::new(&mut **Pin::into_inner(self)).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        Pin::new(&mut **Pin::into_inner(self)).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        Pin::new(&mut **Pin::into_inner(self)).poll_shutdown(cx)
    }
}

// ── AsyncWriteExt ─────────────────────────────────────────────────────────────

/// Extension helpers over [`AsyncWrite`], mirroring the ergonomics of
/// `futures::io::AsyncWriteExt` and `tokio::io::AsyncWriteExt`.
pub trait AsyncWriteExt: AsyncWrite {
    /// Writes the whole of `buf`, looping until every byte is consumed.
    ///
    /// Fails with [`IoError::write_zero`] if the sink reports a zero-byte
    /// write while bytes remain (which would otherwise spin forever).
    fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> WriteAll<'a, Self>
    where
        Self: Sized + Unpin,
    {
        WriteAll { writer: self, buf }
    }

    /// Flushes any buffered writes.
    fn flush(&mut self) -> Flush<'_, Self>
    where
        Self: Sized + Unpin,
    {
        Flush { writer: self }
    }

    /// Performs a graceful shutdown (e.g. TLS `close_notify`, TCP FIN).
    fn shutdown(&mut self) -> Shutdown<'_, Self>
    where
        Self: Sized + Unpin,
    {
        Shutdown { writer: self }
    }
}

impl<W: AsyncWrite + ?Sized> AsyncWriteExt for W {}

/// Future returned by [`AsyncWriteExt::write_all`].
pub struct WriteAll<'a, W: ?Sized> {
    writer: &'a mut W,
    buf: &'a [u8],
}

impl<W: AsyncWrite + Unpin + ?Sized> Future for WriteAll<'_, W> {
    type Output = Result<(), IoError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        while !this.buf.is_empty() {
            match Pin::new(&mut *this.writer).poll_write(cx, this.buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(0)) => return Poll::Ready(Err(IoError::write_zero())),
                Poll::Ready(Ok(n)) => this.buf = &this.buf[n..],
            }
        }
        Poll::Ready(Ok(()))
    }
}

/// Future returned by [`AsyncWriteExt::flush`].
pub struct Flush<'a, W: ?Sized> {
    writer: &'a mut W,
}

impl<W: AsyncWrite + Unpin + ?Sized> Future for Flush<'_, W> {
    type Output = Result<(), IoError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.get_mut().writer).poll_flush(cx)
    }
}

/// Future returned by [`AsyncWriteExt::shutdown`].
pub struct Shutdown<'a, W: ?Sized> {
    writer: &'a mut W,
}

impl<W: AsyncWrite + Unpin + ?Sized> Future for Shutdown<'_, W> {
    type Output = Result<(), IoError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.get_mut().writer).poll_shutdown(cx)
    }
}

// ── Vectored writes (std) ─────────────────────────────────────────────────────

/// Vectored-write extension over [`AsyncWrite`].
///
/// `std`-only because scatter/gather slices ([`std::io::IoSlice`]) are an
/// OS concept.  Implementations with real gather support override
/// [`poll_write_vectored`](AsyncWriteVectored::poll_write_vectored)
/// (e.g. tokio sockets via [`TokioCompat`](crate::TokioCompat)); everything
/// else falls back to writing the first non-empty buffer, so protocol
/// stacks can always emit head + body through one call.
#[cfg(feature = "std")]
pub trait AsyncWriteVectored: AsyncWrite {
    /// Write data from multiple buffers, returning the total bytes written.
    ///
    /// May write fewer bytes than the sum of the buffers; the caller
    /// advances by the returned count.
    fn poll_write_vectored(
        &mut self,
        cx: &mut Context<'_>,
        bufs: &mut [std::io::IoSlice<'_>],
    ) -> Poll<Result<usize, IoError>>;

    /// Returns `true` when [`poll_write_vectored`](AsyncWriteVectored::poll_write_vectored)
    /// gathers buffers natively (otherwise it is the single-buffer
    /// fallback).
    fn is_write_vectored(&self) -> bool;
}

/// Shared fallback: write the first non-empty buffer.
#[cfg(feature = "std")]
pub fn write_vectored_fallback<W: AsyncWrite + Unpin + ?Sized>(
    w: &mut W,
    cx: &mut Context<'_>,
    bufs: &mut [std::io::IoSlice<'_>],
) -> Poll<Result<usize, IoError>> {
    for slice in bufs.iter() {
        if !slice.is_empty() {
            return Pin::new(w).poll_write(cx, slice);
        }
    }
    Poll::Ready(Ok(0))
}

#[cfg(feature = "std")]
impl<W: std::io::Write + Unpin> AsyncWriteVectored for crate::adapters::std_compat::StdWriter<W> {
    fn poll_write_vectored(
        &mut self,
        cx: &mut Context<'_>,
        bufs: &mut [std::io::IoSlice<'_>],
    ) -> Poll<Result<usize, IoError>> {
        write_vectored_fallback(self, cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        false
    }
}

#[cfg(feature = "tokio")]
impl<T: tokio::io::AsyncWrite + Unpin> AsyncWriteVectored
    for crate::adapters::tokio_compat::TokioWriter<T>
{
    fn poll_write_vectored(
        &mut self,
        cx: &mut Context<'_>,
        bufs: &mut [std::io::IoSlice<'_>],
    ) -> Poll<Result<usize, IoError>> {
        let inner = Pin::new(&mut self.0);
        tokio::io::AsyncWrite::poll_write_vectored(inner, cx, bufs)
            .map(|r| r.map_err(IoError::from))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }
}

#[cfg(feature = "tokio")]
impl<T: tokio::io::AsyncWrite + Unpin> AsyncWriteVectored
    for crate::adapters::tokio_compat::TokioCompat<T>
{
    fn poll_write_vectored(
        &mut self,
        cx: &mut Context<'_>,
        bufs: &mut [std::io::IoSlice<'_>],
    ) -> Poll<Result<usize, IoError>> {
        let inner = Pin::new(&mut self.0);
        tokio::io::AsyncWrite::poll_write_vectored(inner, cx, bufs)
            .map(|r| r.map_err(IoError::from))
    }

    fn is_write_vectored(&self) -> bool {
        true
    }
}
