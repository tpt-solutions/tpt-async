// Copyright TPT Solutions. Licensed under MIT OR Apache-2.0.

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

/// Wraps a [`tokio::io::AsyncRead`] into a [`tpt_async_io::AsyncRead`].
///
/// The `ReadBuf` types are bridged by mapping from our `ReadBuf` to
/// `tokio::io::ReadBuf` over the same underlying slice, so no extra
/// allocation or copy occurs.
pub struct TokioReader<R>(pub R);

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
                Poll::Ready(Err(e)) => return Poll::Ready(Err(IoError::Std(e))),
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

/// Wraps a [`tokio::io::AsyncWrite`] into a [`tpt_async_io::AsyncWrite`].
pub struct TokioWriter<W>(pub W);

impl<W: tokio::io::AsyncWrite + Unpin> AsyncWrite for TokioWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        tokio::io::AsyncWrite::poll_write(inner, cx, buf)
            .map(|r| r.map_err(IoError::Std))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        tokio::io::AsyncWrite::poll_flush(inner, cx)
            .map(|r| r.map_err(IoError::Std))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut self.get_mut().0);
        // tokio uses `poll_shutdown` as the close/shutdown operation.
        tokio::io::AsyncWrite::poll_shutdown(inner, cx)
            .map(|r| r.map_err(IoError::Std))
    }
}
