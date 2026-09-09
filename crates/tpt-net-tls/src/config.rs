// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Helpers for building rustls `ClientConfig` and `ServerConfig`.

use std::sync::Arc;

use rustls::{ClientConfig, RootCertStore, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use crate::error::TlsError;

/// Builds a locked-down [`ClientConfig`]:
///
/// - Ring crypto provider (no OpenSSL).
/// - TLS 1.3 only by default; TLS 1.2 enabled when the `tls12` feature is on.
/// - Root certificates sourced from `webpki-roots` (default feature) and/or
///   the system store (`native-certs` feature).
/// - No client certificate (mTLS is out of scope for 0.1).
pub fn rustls_config() -> ClientConfig {
    let mut root_store = RootCertStore::empty();

    #[cfg(feature = "webpki-roots")]
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    #[cfg(feature = "native-certs")]
    {
        for cert in rustls_native_certs::load_native_certs()
            .expect("could not load native certs")
        {
            root_store.add(cert).ok();
        }
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

    builder
        .with_root_certificates(root_store)
        .with_no_client_auth()
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
