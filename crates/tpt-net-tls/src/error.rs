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
    Io(tpt_async_io::IoError),
    /// The operation exceeded its configured timeout.
    TimedOut,
    /// The client trust store is empty: enable the `webpki-roots` (default)
    /// or `native-certs` feature, or supply roots manually.
    RootStoreEmpty,
    /// A PEM certificate/key could not be parsed.
    Pem(std::io::Error),
}

impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TlsError::Rustls(e) => write!(f, "TLS error: {e}"),
            TlsError::Io(e) => write!(f, "I/O error: {e}"),
            TlsError::TimedOut => write!(f, "TLS operation timed out"),
            TlsError::RootStoreEmpty => write!(
                f,
                "trust store is empty: enable the webpki-roots or \
                 native-certs feature, or supply roots manually"
            ),
            TlsError::Pem(e) => write!(f, "PEM parse error: {e}"),
        }
    }
}

impl std::error::Error for TlsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TlsError::Rustls(e) => Some(e),
            TlsError::Io(e) => Some(e),
            TlsError::Pem(e) => Some(e),
            TlsError::TimedOut | TlsError::RootStoreEmpty => None,
        }
    }
}

impl From<rustls::Error> for TlsError {
    fn from(e: rustls::Error) -> Self {
        TlsError::Rustls(e)
    }
}

impl From<tpt_async_io::IoError> for TlsError {
    fn from(e: tpt_async_io::IoError) -> Self {
        TlsError::Io(e)
    }
}
