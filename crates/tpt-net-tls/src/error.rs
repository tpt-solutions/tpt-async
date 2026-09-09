// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Error type for `tpt-net-tls`.

use core::fmt;

/// All errors that can occur in this crate.
#[derive(Debug)]
pub enum TlsError {
    /// A rustls protocol or certificate error.
    Rustls(rustls::Error),
    /// An I/O error on the underlying transport.
    Io(tpt_async_io::read::IoError),
    /// An application attempted to use the stream before the handshake
    /// completed.
    HandshakeNotComplete,
}

impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TlsError::Rustls(e) => write!(f, "TLS error: {e}"),
            TlsError::Io(e) => write!(f, "I/O error: {e}"),
            TlsError::HandshakeNotComplete => {
                write!(f, "TLS handshake has not completed yet")
            }
        }
    }
}

impl std::error::Error for TlsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TlsError::Rustls(e) => Some(e),
            TlsError::Io(e) => Some(e),
            TlsError::HandshakeNotComplete => None,
        }
    }
}

impl From<rustls::Error> for TlsError {
    fn from(e: rustls::Error) -> Self {
        TlsError::Rustls(e)
    }
}

impl From<tpt_async_io::read::IoError> for TlsError {
    fn from(e: tpt_async_io::read::IoError) -> Self {
        TlsError::Io(e)
    }
}
