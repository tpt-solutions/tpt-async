// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure-Rust WebSocket (RFC 6455) client and server for `tpt-async`.
//!
//! Built directly on `tpt-net-http` framing (the opening handshake is real
//! HTTP) and the `tpt-async-io` traits — runtime-agnostic, no OpenSSL, no
//! callbacks.
//!
//! # Example (client + server on an in-memory pipe)
//!
//! ```rust,no_run
//! use tpt_net_ws::{Message, WebSocketStream};
//!
//! # async fn demo(server_io: impl tpt_async_io::AsyncRead + tpt_async_io::AsyncWrite + Unpin + Send,
//! #               client_io: impl tpt_async_io::AsyncRead + tpt_async_io::AsyncWrite + Unpin + Send) -> Result<(), tpt_net_ws::WsError> {
//! // Server side:
//! let mut ws_server = WebSocketStream::accept(server_io).await?;
//!
//! // Client side:
//! let mut ws_client = WebSocketStream::connect(client_io, "/ws", "localhost").await?;
//! ws_client.send(Message::Text("hello".into())).await?;
//!
//! // Server echoes:
//! let msg = ws_server.recv().await?.unwrap();
//! ws_server.send(msg).await?;
//!
//! assert_eq!(ws_client.recv().await?, Some(Message::Text("hello".into())));
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs, clippy::all)]

extern crate alloc;

pub mod error;
pub mod frame;
pub mod handshake;
pub mod prelude;
pub mod stream;

pub use error::WsError;
pub use frame::Opcode;
pub use stream::{Message, WebSocketStream};
