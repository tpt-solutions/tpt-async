// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! TLS client connector.

use std::sync::Arc;

use rustls::{ClientConfig, ClientConnection, Connection};
use rustls::pki_types::ServerName;
use tpt_async_io::read::AsyncRead;
use tpt_async_io::write::AsyncWrite;

use crate::error::TlsError;
use crate::stream::TlsStream;

/// Initiates TLS client connections.
///
/// Build a [`TlsConnector`] once (it is cheaply cloneable via its inner
/// `Arc`), then call [`connect`] for each new connection.
///
/// [`connect`]: TlsConnector::connect
#[derive(Clone)]
pub struct TlsConnector {
    config: Arc<ClientConfig>,
}

impl TlsConnector {
    /// Creates a new `TlsConnector` from a [`ClientConfig`].
    ///
    /// The config is wrapped in an [`Arc`] internally.
    pub fn new(config: ClientConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Creates a new `TlsConnector` from a pre-shared [`Arc<ClientConfig>`].
    pub fn from_config(config: Arc<ClientConfig>) -> Self {
        Self { config }
    }

    /// Performs a TLS client handshake over `stream`, verifying `server_name`.
    ///
    /// Returns a [`TlsStream`] that is ready for plaintext I/O once the
    /// future resolves.
    ///
    /// # Errors
    ///
    /// Returns a [`TlsError`] if the handshake fails (invalid certificate,
    /// I/O error, protocol error, etc.).
    pub async fn connect<IO>(
        &self,
        server_name: ServerName<'static>,
        stream: IO,
    ) -> Result<TlsStream<IO>, TlsError>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        let conn = ClientConnection::new(Arc::clone(&self.config), server_name)
            .map_err(TlsError::Rustls)?;

        let mut tls = TlsStream::new(stream, Connection::Client(conn));
        tls.handshake().await?;
        Ok(tls)
    }
}
