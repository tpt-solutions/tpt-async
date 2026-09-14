// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Zero-copy `AsyncRead` / `AsyncWrite` traits abstracting over std, tokio,
//! async-std, and bare-metal hardware.
//!
//! # Feature flags
//!
//! | Flag        | Default | What it enables |
//! |-------------|---------|-----------------|
//! | `alloc`     | yes     | heap-backed impls (e.g. `AsyncReadExt::read_to_end`) |
//! | `std`       | yes     | wraps `std::io::Error` in `IoError`, enables `std::error::Error` |
//! | `tokio`     | no      | [`TokioReader`]/[`TokioWriter`] adapters for tokio types |
//! | `async-std` | no      | [`AsyncStdReader`]/[`AsyncStdWriter`] adapters for async-std types |
//! | `embedded-io` | no   | [`EmbeddedIo`] adapter for `embedded-io-async` streams (embassy) |
//!
//! Tokio and async-std types do **not** implement our traits via blanket
//! impls; wrap them explicitly (`TokioReader::new(stream)`), which keeps
//! coherence clean and lets downstream crates implement our traits for their
//! own tokio-compatible types without conflicts.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs, clippy::all)]
#![forbid(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod adapters;
/// Async counterpart of `std::io::BufRead` for zero-copy parsing.
pub mod buf;
pub mod prelude;
pub mod read;
pub mod read_buf;
pub mod write;

pub use buf::AsyncBufRead;
#[cfg(not(feature = "std"))]
pub use read::IoErrorKind;
pub use read::{AsyncRead, AsyncReadExt, IoError};
pub use read_buf::ReadBuf;
#[cfg(feature = "std")]
pub use write::AsyncWriteVectored;
pub use write::{AsyncWrite, AsyncWriteExt};

#[cfg(feature = "async-std")]
#[cfg_attr(docsrs, doc(cfg(feature = "async-std")))]
pub use adapters::async_std_compat::{AsyncStdReader, AsyncStdWriter};
#[cfg(feature = "embedded-io")]
#[cfg_attr(docsrs, doc(cfg(feature = "embedded-io")))]
pub use adapters::embedded_compat::EmbeddedIo;
#[cfg(feature = "std")]
pub use adapters::std_compat::{StdReader, StdWriter};
#[cfg(feature = "tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
pub use adapters::tokio_compat::{TokioCompat, TokioReader, TokioWriter};
