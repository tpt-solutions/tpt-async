// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! TLS server acceptor.

use std::sync::Arc;

use rustls::{Connection, ServerConfig, ServerConnection};
use tpt_async_io::read::AsyncRead;
use tpt_async_io::write::AsyncWrite;

use crate::error::TlsError;
use crate::stream::TlsStream;

/// Accepts incoming TLS connections on the server side.
///
/// Build a [`TlsAcceptor`] once (it is cheaply cloneable via its inner
/// `Arc`), then call [`accept`] for each incoming connection.
///
/// [`accept`]: TlsAcceptor::accept
#[derive(Clone)]
pub struct TlsAcceptor {
    config: Arc<ServerConfig>,
}

impl TlsAcceptor {
    /// Creates a new `TlsAcceptor` from a [`ServerConfig`].
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Creates a new `TlsAcceptor` from a pre-shared [`Arc<ServerConfig>`].
    pub fn from_config(config: Arc<ServerConfig>) -> Self {
        Self { config }
    }

    /// Performs a TLS server handshake over `stream`.
    ///
    /// Returns a [`TlsStream`] that is ready for plaintext I/O once the
    /// future resolves.
    ///
    /// # Errors
    ///
    /// Returns a [`TlsError`] if the handshake fails (bad client hello,
    /// I/O error, protocol error, etc.).
    pub async fn accept<IO>(&self, stream: IO) -> Result<TlsStream<IO>, TlsError>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        let conn = ServerConnection::new(Arc::clone(&self.config))
            .map_err(TlsError::Rustls)?;

        let mut tls = TlsStream::new(stream, Connection::Server(conn));
        tls.handshake().await?;
        Ok(tls)
    }
}
