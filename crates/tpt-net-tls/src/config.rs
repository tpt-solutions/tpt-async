// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Helpers for building rustls `ClientConfig` and `ServerConfig`.

use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

use crate::error::TlsError;

/// Builds a locked-down [`ClientConfig`]:
///
/// - Ring crypto provider (no OpenSSL).
/// - TLS 1.3 only by default; TLS 1.2 enabled when the `tls12` feature is on.
/// - Root certificates sourced from `webpki-roots` (default feature) and/or
///   the system store (`native-certs` feature).
/// - No client certificate (mTLS is out of scope for 0.1).
///
/// # Errors
///
/// Returns [`TlsError::RootStoreEmpty`] if neither certificate source is
/// enabled (or both produced zero usable certificates) — connecting with an
/// empty trust store can only fail, so it is rejected up front.
pub fn rustls_config() -> Result<ClientConfig, TlsError> {
    let mut root_store = RootCertStore::empty();

    #[cfg(feature = "webpki-roots")]
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    #[cfg(feature = "native-certs")]
    {
        let result = rustls_native_certs::load_native_certs();
        if result.certs.is_empty() {
            if let Some(err) = result.errors.first() {
                return Err(TlsError::Io(tpt_async_io::IoError::from(
                    std::io::Error::other(format!("could not load native certificates: {err}")),
                )));
            }
        }
        for cert in result.certs {
            // Malformed system certificates are skipped individually; if
            // *none* load, the empty-store check below catches it.
            let _ = root_store.add(cert);
        }
    }

    if root_store.is_empty() {
        return Err(TlsError::RootStoreEmpty);
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let builder = ClientConfig::builder_with_provider(provider);

    #[cfg(not(feature = "tls12"))]
    let builder = builder
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3 must be supported by the ring provider");

    #[cfg(feature = "tls12")]
    let builder = builder
        .with_safe_default_protocol_versions()
        .expect("safe default protocol versions must be supported");

    Ok(builder
        .with_root_certificates(root_store)
        .with_no_client_auth())
}

/// Builds a minimal [`ServerConfig`] from a DER-encoded certificate chain and
/// private key.
///
/// - Ring crypto provider (no OpenSSL).
/// - TLS 1.3 only by default; TLS 1.2 enabled when the `tls12` feature is on.
///
/// # Errors
///
/// Returns [`TlsError::Rustls`] if rustls rejects the certificate or key.
pub fn server_config(
    cert_chain: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
) -> Result<ServerConfig, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let builder = ServerConfig::builder_with_provider(provider);

    #[cfg(not(feature = "tls12"))]
    let builder = builder
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3 must be supported by the ring provider");

    #[cfg(feature = "tls12")]
    let builder = builder
        .with_safe_default_protocol_versions()
        .expect("safe default protocol versions must be supported");

    builder
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(TlsError::Rustls)
}

/// Loads PEM-encoded certificates from `reader` into a DER vector suitable
/// for [`server_config`].
///
/// # Errors
///
/// Returns [`TlsError::Pem`] if the stream is not valid PEM.
pub fn load_pem_certs(
    reader: &mut dyn std::io::BufRead,
) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let mut certs = Vec::new();
    for cert in rustls_pemfile::certs(reader) {
        certs.push(cert.map_err(|e| {
            TlsError::Pem(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid PEM certificate: {e}"),
            ))
        })?);
    }
    Ok(certs)
}

/// Loads a PEM-encoded private key (PKCS#8, PKCS#1, or SEC1) from `reader`.
///
/// # Errors
///
/// Returns [`TlsError::Pem`] if no key or an invalid key is found.
pub fn load_pem_key(reader: &mut dyn std::io::BufRead) -> Result<PrivateKeyDer<'static>, TlsError> {
    rustls_pemfile::private_key(reader)
        .map_err(|e| {
            TlsError::Pem(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid PEM private key: {e}"),
            ))
        })?
        .ok_or_else(|| {
            TlsError::Pem(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "no private key found in PEM stream",
            ))
        })
}
