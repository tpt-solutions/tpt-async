// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal rustls 0.23 TLS wrapper for the `tpt-async` ecosystem.
//!
//! # Quick start
//!
//! ```no_run
//! use std::sync::Arc;
//! use rustls::pki_types::ServerName;
//! use tpt_net_tls::{TlsConnector, rustls_config};
//!
//! // Build a client connector using Mozilla's root certs (default feature).
//! let connector = TlsConnector::new(rustls_config().expect("trust store"));
//!
//! // Connect to a server (supply any AsyncRead + AsyncWrite + Unpin transport).
//! // let stream = …; // e.g. a TCP socket
//! // let server_name = ServerName::try_from("example.com").unwrap().to_owned();
//! // let tls = connector.connect(server_name, stream).await?;
//! ```
//!
//! # Feature flags
//!
//! | Flag            | Default | What it enables |
//! |-----------------|---------|-----------------|
//! | `webpki-roots`  | yes     | Mozilla root certs bundled via `webpki-roots` |
//! | `native-certs`  | no      | System trust store via `rustls-native-certs` |
//! | `tls12`         | no      | Opt-in TLS 1.2 support (TLS 1.3 only by default) |
//!
//! Handshake timeouts (`connect_timeout`/`accept_timeout`) are powered by the
//! timer crate's std driver.

pub mod acceptor;
pub mod config;
pub mod connector;
pub mod error;
pub mod pinning;
pub mod stream;

pub use acceptor::TlsAcceptor;
pub use config::{load_pem_certs, load_pem_key, rustls_config, server_config};
pub use connector::TlsConnector;
pub use error::TlsError;
pub use pinning::{pin_for_cert, pinned_connector, PinnedCertVerifier};
pub use stream::TlsStream;
