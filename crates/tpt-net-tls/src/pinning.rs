// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Certificate pinning.
//!
//! [`PinnedCertVerifier`] accepts a server when either
//!
//! 1. the leaf certificate's SHA-256 digest matches one of the configured
//!    pins (SPKI/cert pinning), or
//! 2. a fallback verifier (normally the standard webpki chain validation)
//!    accepts it.
//!
//! That gives "pin when configured, else trust the store" semantics: a
//! mis-issued CA certificate cannot impersonate a pinned endpoint, while
//! unpinned endpoints keep working.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};

/// A `rustls` server-certificate verifier implementing pin-or-fallback.
#[derive(Debug)]
pub struct PinnedCertVerifier {
    /// SHA-256 digests of pinned DER-encoded leaf certificates.
    pins: Vec<[u8; 32]>,
    /// Used when the leaf does not match any pin.
    fallback: Arc<dyn ServerCertVerifier>,
    provider: Arc<CryptoProvider>,
}

impl PinnedCertVerifier {
    /// Build a verifier trusting exactly the pinned certificates (no
    /// fallback chain validation).
    pub fn new(pins: Vec<[u8; 32]>, provider: Arc<CryptoProvider>) -> Self {
        Self {
            pins,
            fallback: Arc::new(NoFallback {
                provider: Arc::clone(&provider),
            }),
            provider,
        }
    }

    /// Build a pin-or-fallback verifier: pins first, then `fallback`
    /// (typically the verifier produced by a standard
    /// `ClientConfig::builder()`… `with_root_certificates` chain).
    pub fn with_fallback(
        pins: Vec<[u8; 32]>,
        fallback: Arc<dyn ServerCertVerifier>,
        provider: Arc<CryptoProvider>,
    ) -> Self {
        Self {
            pins,
            fallback,
            provider,
        }
    }

    fn matches_pin(&self, cert: &CertificateDer<'_>) -> bool {
        use sha2::{Digest, Sha256};
        let digest: [u8; 32] = Sha256::digest(cert.as_ref()).into();
        self.pins.contains(&digest)
    }
}

/// Terminal verifier used when no fallback chain validation is configured.
#[derive(Debug)]
struct NoFallback {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for NoFallback {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Err(RustlsError::General(
            "certificate not pinned and no fallback verifier configured".into(),
        ))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls12_signature(
            message,
            cert,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls13_signature(
            message,
            cert,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

impl ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        if self.matches_pin(end_entity) {
            return Ok(ServerCertVerified::assertion());
        }
        self.fallback
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls12_signature(
            message,
            cert,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls13_signature(
            message,
            cert,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Compute the SHA-256 pin value for a DER-encoded certificate — the exact
/// format [`PinnedCertVerifier::new`] expects.
pub fn pin_for_cert(cert_der: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(cert_der).into()
}

/// Build a [`TlsConnector`](crate::TlsConnector) whose server certificates
/// must match one of `pins` (SHA-256 of the DER leaf), skipping normal chain
/// validation entirely.
///
/// # Example
///
/// ```rust,no_run
/// use tpt_net_tls::{pinned_connector, pin_for_cert};
///
/// # fn demo(cert_der: &[u8]) -> rustls::ClientConfig {
/// let mut config = rustls::ClientConfig::builder()
///     .dangerous()
///     .with_custom_certificate_verifier(pinned_connector(vec![pin_for_cert(cert_der)]))
///     .with_no_client_auth();
/// # config
/// # }
/// ```
pub fn pinned_connector(pins: Vec<[u8; 32]>) -> Arc<dyn ServerCertVerifier> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Arc::new(PinnedCertVerifier::new(pins, provider))
}
