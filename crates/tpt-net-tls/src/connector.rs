// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! TLS client connector.

use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, Connection};
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

    /// Like [`connect`](TlsConnector::connect), but fails with
    /// [`TlsError::TimedOut`] if the handshake takes longer than `timeout`.
    ///
    /// The deadline is enforced with the timer crate's std driver, so this
    /// works under any executor that parks on wake (including
    /// `LocalExecutor::block_on`).
    ///
    /// # Errors
    ///
    /// [`TlsError::TimedOut`] on timeout; otherwise as for [`connect`].
    ///
    /// [`connect`]: TlsConnector::connect
    pub async fn connect_timeout<IO>(
        &self,
        server_name: ServerName<'static>,
        stream: IO,
        timeout: core::time::Duration,
    ) -> Result<TlsStream<IO>, TlsError>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
    {
        match tpt_async_timer::driver::timeout(timeout, self.connect(server_name, stream)).await {
            Ok(result) => result,
            Err(_timed_out) => Err(TlsError::TimedOut),
        }
    }
}
