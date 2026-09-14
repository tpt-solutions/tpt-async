// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal, spec-compliant HTTP/1.1 client and server for `tpt-async`.
//!
//! # Design
//!
//! - **Zero-copy headers**: request/response heads are parsed straight out
//!   of the connection's read buffer; the buffer moves into an `Arc` and
//!   header names/values are byte *ranges* into it.  No per-header
//!   allocation, and the API stays fully safe.
//! - **Runtime-agnostic**: everything is written against
//!   `tpt_async_io::AsyncRead`/`AsyncWrite` — bring any transport (tokio
//!   sockets via adapters, TLS via `tpt-net-tls`, in-memory pipes).
//! - **Strict framing**: duplicate or conflicting `Content-Length` values,
//!   `Transfer-Encoding` + `Content-Length` together, and obsolete line
//!   folding are rejected (request-smuggling hygiene).
//!
//! # Example (client against a server on the same stack)
//!
//! ```rust,no_run
//! use tpt_net_http::{HttpClient, ClientConnection, Request};
//!
//! # async fn demo(io: impl tpt_async_io::AsyncRead + tpt_async_io::AsyncWrite + Unpin) -> Result<(), tpt_net_http::HttpError> {
//! // Any transport works; here `io` is an established connection.
//! let mut conn = ClientConnection::new(io);
//!
//! let request = Request::new("GET", "/health");
//! let mut response = conn.send(&request, "example.com").await?;
//!
//! assert_eq!(response.status(), 200);
//! let body = response.body_bytes(64 * 1024).await?;
//! # drop(body);
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs, clippy::all)]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

pub mod body;
pub mod client;
pub mod connection;
pub mod error;
pub mod parse;
pub mod prelude;
pub mod server;

pub use client::{ClientConnection, Connector, HttpClient, OwnedResponse, Pool, Request, Response};
pub use error::HttpError;
pub use parse::{HeaderBlock, RequestHead, ResponseHead, Version};
pub use server::{headers, serve_connection, Handler, ResponseData, ServerRequest};

/// The error type of `Timeout` futures reused here for uniform conversions.
pub use tpt_async_timer::timeout::TimedOut as Timeout;
