// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`TlsStream`]: a rustls `Connection` layered over any async I/O transport.

use std::io::{self, Read as StdRead};
use std::pin::Pin;
use std::task::{Context, Poll};

use rustls::Connection;
use tpt_async_io::read::{AsyncRead, IoError};
use tpt_async_io::read_buf::ReadBuf;
use tpt_async_io::write::AsyncWrite;

use crate::error::TlsError;

/// Maps a crate error to the [`IoError`] the trait signatures require.
/// rustls errors ride along as InvalidData; the typed error remains available
/// on the handshake path (`Result<_, TlsError>`).
fn into_io(e: TlsError) -> IoError {
    match e {
        TlsError::Io(io) => io,
        TlsError::Rustls(e) => IoError::from(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )),
        TlsError::Pem(e) => IoError::from(e),
        other => IoError::from(std::io::Error::other(other.to_string())),
    }
}

// ── TlsStream ────────────────────────────────────────────────────────────────

/// A TLS stream layered over any [`AsyncRead`] + [`AsyncWrite`] transport.
///
/// Obtain one via [`TlsConnector::connect`] or [`TlsAcceptor::accept`]; both
/// run the handshake before returning.  The stream itself then implements
/// [`AsyncRead`] and [`AsyncWrite`] for exchanging plaintext.
///
/// # Flush semantics
///
/// [`poll_write`](AsyncWrite::poll_write) accepts plaintext into rustls and
/// returns `Ok(n)` as soon as the bytes are *accepted*; the resulting
/// ciphertext is flushed to the transport opportunistically — from
/// [`poll_write`](AsyncWrite::poll_write), [`poll_flush`](AsyncWrite::poll_flush),
/// **and** [`poll_read`](AsyncRead::poll_read) — so a write that is still in
/// flight always drains once the transport becomes writable, even if the
/// task immediately goes back to awaiting a response.  Call
/// [`poll_flush`](AsyncWrite::poll_flush) (or `AsyncWriteExt::flush`) when
/// you need a hard guarantee that bytes have left the process.
///
/// [`TlsConnector::connect`]: crate::TlsConnector::connect
/// [`TlsAcceptor::accept`]: crate::TlsAcceptor::accept
pub struct TlsStream<IO> {
    /// Underlying transport (e.g. a TCP socket or in-memory pipe).
    io: IO,
    /// The rustls connection state machine (client or server).
    conn: Connection,
    /// TLS ciphertext bytes read from `io` but not yet parsed by rustls.
    read_buf: Vec<u8>,
    /// TLS record bytes produced by rustls, pending write to `io`.
    write_buf: Vec<u8>,
    /// How many bytes at the front of `write_buf` have already been sent.
    write_pos: usize,
}

impl<IO: AsyncRead + AsyncWrite + Unpin> TlsStream<IO> {
    /// Wraps `io` and `conn` into a new, un-handshaked stream.
    pub(crate) fn new(io: IO, conn: Connection) -> Self {
        Self {
            io,
            conn,
            read_buf: Vec::new(),
            write_buf: Vec::new(),
            write_pos: 0,
        }
    }

    // ── internal helpers ─────────────────────────────────────────────────────

    /// Drains any TLS records queued by rustls into `write_buf`.
    fn pull_tls_records(&mut self) -> Result<(), TlsError> {
        // Append newly-generated TLS records to whatever is still pending.
        self.conn
            .write_tls(&mut self.write_buf)
            .map_err(IoError::from)
            .map_err(TlsError::Io)?;
        Ok(())
    }

    /// Tries to flush `write_buf` to the underlying IO.
    ///
    /// Returns `Poll::Pending` if the IO is not ready for writing (the waker
    /// is registered by the transport), or `Poll::Ready(Ok(()))` once the
    /// buffer is fully drained.
    fn poll_flush_write_buf(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), TlsError>> {
        while self.write_pos < self.write_buf.len() {
            let pending_slice = &self.write_buf[self.write_pos..];
            let n = match Pin::new(&mut self.io).poll_write(cx, pending_slice) {
                Poll::Ready(Ok(n)) => n,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(TlsError::Io(e))),
                Poll::Pending => return Poll::Pending,
            };
            if n == 0 {
                return Poll::Ready(Err(TlsError::Io(IoError::write_zero())));
            }
            self.write_pos += n;
        }
        // All bytes sent; reclaim the allocation.
        self.write_buf.clear();
        self.write_pos = 0;
        Poll::Ready(Ok(()))
    }

    /// If ciphertext is pending, try to push it to the transport.
    ///
    /// This is the liveness primitive: *every* poll path (read, write, flush)
    /// drives pending writes, so buffered records always drain as soon as the
    /// transport becomes writable, regardless of which direction the task is
    /// currently awaiting.  Records rustls generates post-handshake (e.g.
    /// session tickets) are pulled here as well.
    fn poll_drive_pending_writes(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), TlsError>> {
        if self.conn.wants_write() {
            self.pull_tls_records()?;
        }
        if self.write_pos >= self.write_buf.len() {
            return Poll::Ready(Ok(()));
        }
        self.poll_flush_write_buf(cx)
    }

    /// Feeds bytes from `read_buf` into rustls and processes any newly
    /// decrypted data.  Returns how many bytes of `read_buf` were consumed.
    fn feed_incoming_to_rustls(&mut self) -> Result<usize, TlsError> {
        if self.read_buf.is_empty() {
            return Ok(0);
        }
        // Wrap the buffered ciphertext in a Cursor so rustls can call Read on it.
        let mut cursor = io::Cursor::new(self.read_buf.as_slice());
        let consumed = self
            .conn
            .read_tls(&mut cursor)
            .map_err(IoError::from)
            .map_err(TlsError::Io)?;

        // Remove consumed bytes from the front of read_buf.
        if consumed > 0 {
            self.read_buf.drain(..consumed);
        }

        // Decrypt and authenticate.
        self.conn.process_new_packets().map_err(TlsError::Rustls)?;

        Ok(consumed)
    }

    // ── handshake ────────────────────────────────────────────────────────────

    /// Runs the TLS handshake to completion.
    ///
    /// This must be awaited once before any plaintext read/write.  Both
    /// [`TlsConnector::connect`] and [`TlsAcceptor::accept`] call it
    /// automatically, so users of those APIs do not need to call it manually.
    ///
    /// The loop follows the rustls-recommended pattern: drive `wants_read`
    /// (fetch + process) and `wants_write` (pull + send) until the handshake
    /// is done *and* every record rustls produced has been flushed.  Checking
    /// only `is_handshaking` would race the final flight: the record that
    /// completes the handshake can itself enqueue the peer's awaited bytes.
    ///
    /// [`TlsConnector::connect`]: crate::TlsConnector::connect
    /// [`TlsAcceptor::accept`]: crate::TlsAcceptor::accept
    pub async fn handshake(&mut self) -> Result<(), TlsError> {
        loop {
            let mut progress = false;

            // ── 1. Process any buffered ciphertext. ───────────────────────
            if !self.read_buf.is_empty() {
                let consumed = self.feed_incoming_to_rustls()?;
                if consumed > 0 {
                    progress = true;
                }
            }

            // ── 2. Drain everything rustls wants to send. ─────────────────
            if self.conn.wants_write() {
                self.pull_tls_records()?;
            }
            if !self.write_buf.is_empty() {
                let total = self.write_buf.len();
                let mut written = self.write_pos;
                while written < total {
                    let slice = &self.write_buf[written..];
                    let n =
                        core::future::poll_fn(|cx| Pin::new(&mut self.io).poll_write(cx, slice))
                            .await
                            .map_err(TlsError::Io)?;
                    if n == 0 {
                        return Err(TlsError::Io(IoError::write_zero()));
                    }
                    written += n;
                }
                self.write_buf.clear();
                self.write_pos = 0;

                // Flush so the peer actually receives the bytes.
                core::future::poll_fn(|cx| Pin::new(&mut self.io).poll_flush(cx))
                    .await
                    .map_err(TlsError::Io)?;
                progress = true;
            }

            // ── 3. Done only when the handshake is complete AND every record
            //      rustls produced has reached the transport. ──────────────
            if !self.conn.is_handshaking()
                && !self.conn.wants_write()
                && self.write_pos >= self.write_buf.len()
            {
                return Ok(());
            }

            // ── 4. Fetch more ciphertext when rustls wants it, or when this
            //      iteration made no progress (avoid a hot spin). ──────────
            if self.conn.wants_read() || !progress {
                let mut tmp = [0u8; 4096];
                let mut io_buf = ReadBuf::new(&mut tmp);
                core::future::poll_fn(|cx| Pin::new(&mut self.io).poll_read(cx, &mut io_buf))
                    .await
                    .map_err(TlsError::Io)?;

                let filled = io_buf.filled();
                if filled.is_empty() {
                    return Err(TlsError::Io(IoError::from(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "peer closed connection during TLS handshake",
                    ))));
                }
                self.read_buf.extend_from_slice(filled);
                self.feed_incoming_to_rustls()?;
            }
        }
    }
}

// ── AsyncRead impl ────────────────────────────────────────────────────────────

impl<IO: AsyncRead + AsyncWrite + Unpin> AsyncRead for TlsStream<IO> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        let this = Pin::into_inner(self);

        loop {
            // ── Opportunistically drain pending ciphertext ─────────────────
            // Keeps a write accepted moments ago flowing even when the task
            // immediately went back to awaiting a response.
            match this.poll_drive_pending_writes(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(into_io(e))),
                Poll::Ready(Ok(())) => {}
            }

            // ── Try to drain plaintext rustls already has decrypted ────────
            {
                let mut reader = this.conn.reader();
                match reader.read(buf.unfilled()) {
                    Ok(0) => {
                        // EOF: peer closed the TLS session cleanly.
                        return Poll::Ready(Ok(()));
                    }
                    Ok(n) => {
                        buf.advance(n);
                        return Poll::Ready(Ok(()));
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // No plaintext ready yet; fall through to fetch more TLS data.
                    }
                    Err(e) => return Poll::Ready(Err(IoError::from(e))),
                }
            } // `reader` borrow ends here

            // ── Feed any buffered ciphertext into rustls ───────────────────
            if !this.read_buf.is_empty() {
                this.feed_incoming_to_rustls().map_err(into_io)?;
                // Go back to the top and try to read plaintext again.
                continue;
            }

            // ── Poll the transport for more ciphertext ────────────────────
            let mut tmp = [0u8; 4096];
            let mut io_buf = ReadBuf::new(&mut tmp);
            match Pin::new(&mut this.io).poll_read(cx, &mut io_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {}
            }

            let filled = io_buf.filled();
            if filled.is_empty() {
                // Transport EOF before a clean TLS close-notify: surface as an error.
                return Poll::Ready(Err(IoError::from(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "TLS peer closed the transport unexpectedly",
                ))));
            }
            this.read_buf.extend_from_slice(filled);
            // Loop again: feed the new bytes into rustls and retry plaintext read.
        }
    }
}

// ── AsyncWrite impl ───────────────────────────────────────────────────────────

impl<IO: AsyncRead + AsyncWrite + Unpin> AsyncWrite for TlsStream<IO> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let this = Pin::into_inner(self);

        // First, finish any ciphertext still pending from a previous write.
        // Until it drains we must not accept new plaintext: returning
        // `Pending` here is correct because this poll has not consumed `buf`
        // yet, so the caller will simply retry with the same bytes.
        match this.poll_flush_write_buf(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(into_io(e))),
            Poll::Ready(Ok(())) => {}
        }

        // Give the plaintext to rustls; it appends TLS records internally.
        {
            let mut writer = this.conn.writer();
            let n = std::io::Write::write(&mut writer, buf).map_err(IoError::from)?;
            if n == 0 {
                return Poll::Ready(Err(IoError::write_zero()));
            }
        } // writer borrow ends

        // Pull the freshly-generated TLS records into write_buf.
        if let Err(e) = this.pull_tls_records() {
            return Poll::Ready(Err(into_io(e)));
        }

        // Try to send them right away.
        match this.poll_flush_write_buf(cx) {
            // Bytes are accepted by rustls; ciphertext stays buffered and is
            // drained opportunistically by poll_read/poll_write/poll_flush
            // (see the type docs).  Reporting acceptance is therefore safe.
            Poll::Pending => Poll::Ready(Ok(buf.len())),
            Poll::Ready(Ok(())) => Poll::Ready(Ok(buf.len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(into_io(e))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let this = Pin::into_inner(self);

        // Flush any TLS records still in write_buf to the transport.
        match this.poll_flush_write_buf(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(into_io(e))),
            Poll::Ready(Ok(())) => {}
        }

        // Flush the underlying transport.
        Pin::new(&mut this.io).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let this = Pin::into_inner(self);

        // Send a TLS close_notify alert.
        this.conn.send_close_notify();

        // Drain any records rustls produced for the close_notify.
        if let Err(e) = this.pull_tls_records() {
            return Poll::Ready(Err(into_io(e)));
        }

        // Write them out.
        match this.poll_flush_write_buf(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(into_io(e))),
            Poll::Ready(Ok(())) => {}
        }

        // Shut down the underlying transport.
        Pin::new(&mut this.io).poll_shutdown(cx)
    }
}
