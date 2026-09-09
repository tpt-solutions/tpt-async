// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Async write trait.

use core::pin::Pin;
use core::task::{Context, Poll};

use crate::read::IoError;

/// Trait for types that can be written to asynchronously.
///
/// The design mirrors [`tokio::io::AsyncWrite`] closely so that bridging
/// adapters can be written with minimal glue.
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
    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>>;

    /// Initiates or attempts to complete a graceful shutdown.
    ///
    /// After `poll_shutdown` returns `Ok(())`, no further writes may be
    /// issued.
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>>;
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

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        Pin::new(&mut **Pin::into_inner(self)).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        Pin::new(&mut **Pin::into_inner(self)).poll_shutdown(cx)
    }
}

// ── tokio bridge (optional) ───────────────────────────────────────────────────

#[cfg(feature = "tokio")]
mod tokio_bridge_write {
    use super::*;

    /// Blanket impl: any `tokio::io::AsyncWrite + Unpin` type becomes an
    /// `AsyncWrite`.
    impl<T> AsyncWrite for T
    where
        T: tokio::io::AsyncWrite + Unpin,
    {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, IoError>> {
            tokio::io::AsyncWrite::poll_write(self, cx, buf)
                .map_err(IoError::from)
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), IoError>> {
            tokio::io::AsyncWrite::poll_flush(self, cx)
                .map_err(IoError::from)
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), IoError>> {
            tokio::io::AsyncWrite::poll_shutdown(self, cx)
                .map_err(IoError::from)
        }
    }
}
