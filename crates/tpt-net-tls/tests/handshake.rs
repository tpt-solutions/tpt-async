// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration test: TLS handshake + plaintext round-trip over an in-memory pipe.

use std::pin::Pin;
use std::task::{Context, Poll};

use tpt_async_io::read::{AsyncRead, IoError};
use tpt_async_io::read_buf::ReadBuf;
use tpt_async_io::write::AsyncWrite;
use tpt_net_tls::{TlsAcceptor, TlsConnector};

// ── MemPipe ──────────────────────────────────────────────────────────────────
//
// A thin newtype around tokio's `DuplexStream` that implements the
// `tpt-async-io` AsyncRead / AsyncWrite traits so it can serve as the
// transport for TlsStream in this test.

struct MemPipe(tokio::io::DuplexStream);

impl AsyncRead for MemPipe {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        // SAFETY: MemPipe is Unpin (DuplexStream is Unpin).
        let inner = Pin::new(&mut Pin::into_inner(self).0);
        let mut tbuf = tokio::io::ReadBuf::new(buf.unfilled());
        match tokio::io::AsyncRead::poll_read(inner, cx, &mut tbuf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(IoError::from(e))),
            Poll::Ready(Ok(())) => {
                let filled = tbuf.filled().len();
                buf.advance(filled);
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl AsyncWrite for MemPipe {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        let inner = Pin::new(&mut Pin::into_inner(self).0);
        tokio::io::AsyncWrite::poll_write(inner, cx, data).map_err(IoError::from)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut Pin::into_inner(self).0);
        tokio::io::AsyncWrite::poll_flush(inner, cx).map_err(IoError::from)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        let inner = Pin::new(&mut Pin::into_inner(self).0);
        tokio::io::AsyncWrite::poll_shutdown(inner, cx).map_err(IoError::from)
    }
}

// ── Test ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tls_handshake_and_roundtrip() {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
    use std::sync::Arc;

    // ── 1. Generate a self-signed certificate and key with rcgen ─────────────
    let subject_alt_names = vec!["localhost".to_string()];
    let certified_key =
        rcgen::generate_simple_self_signed(subject_alt_names).expect("rcgen failed");

    let cert_der: CertificateDer<'static> = certified_key.cert.der().clone();
    let key_bytes = certified_key.key_pair.serialize_der();
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_bytes));

    // ── 2. Build server TlsAcceptor ───────────────────────────────────────────
    let server_cfg = tpt_net_tls::config::server_config(vec![cert_der.clone()], private_key)
        .expect("server_config failed");
    let acceptor = TlsAcceptor::new(server_cfg);

    // ── 3. Build client TlsConnector with custom root (the self-signed cert) ──
    let mut root_store = rustls::RootCertStore::empty();
    root_store
        .add(cert_der)
        .expect("failed to add cert to root store");

    let client_cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .expect("TLS 1.3 must be supported")
    .with_root_certificates(root_store)
    .with_no_client_auth();

    let connector = TlsConnector::new(client_cfg);

    // ── 4. Create an in-memory pipe ───────────────────────────────────────────
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let client_pipe = MemPipe(client_io);
    let server_pipe = MemPipe(server_io);

    // ── 5. Run client + server handshakes concurrently ────────────────────────
    let server_name = ServerName::try_from("localhost")
        .expect("valid server name")
        .to_owned();

    let (client_result, server_result) = tokio::join!(
        connector.connect(server_name, client_pipe),
        acceptor.accept(server_pipe),
    );

    let mut client_tls = client_result.expect("client handshake failed");
    let mut server_tls = server_result.expect("server handshake failed");

    // ── 6. Client sends one byte ──────────────────────────────────────────────
    let byte_sent: u8 = 42;

    core::future::poll_fn(|cx| Pin::new(&mut client_tls).poll_write(cx, &[byte_sent]))
        .await
        .expect("client write failed");

    core::future::poll_fn(|cx| Pin::new(&mut client_tls).poll_flush(cx))
        .await
        .expect("client flush failed");

    // ── 7. Server reads it back ───────────────────────────────────────────────
    let mut recv_buf = [0u8; 1];
    let mut read_buf = ReadBuf::new(&mut recv_buf);

    core::future::poll_fn(|cx| Pin::new(&mut server_tls).poll_read(cx, &mut read_buf))
        .await
        .expect("server read failed");

    // ── 8. Assert the byte round-tripped correctly ────────────────────────────
    assert_eq!(
        read_buf.filled(),
        &[byte_sent],
        "byte did not survive the TLS round trip"
    );
}
