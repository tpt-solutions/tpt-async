// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Zero-copy `AsyncRead` / `AsyncWrite` traits abstracting over std, tokio,
//! async-std, and bare-metal hardware.
//!
//! # Feature flags
//!
//! | Flag        | Default | What it enables |
//! |-------------|---------|-----------------|
//! | `alloc`     | yes     | heap-backed impls |
//! | `std`       | yes     | wraps `std::io::Error` in `IoError`, enables `std::error::Error` |
//! | `tokio`     | no      | blanket `AsyncRead`/`AsyncWrite` impls for tokio types |
//! | `async-std` | no      | blanket impls for async-std types |

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs, clippy::all)]
#![forbid(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod read_buf;
pub mod read;
pub mod write;

pub use read_buf::ReadBuf;
pub use read::{AsyncRead, IoError};
pub use write::AsyncWrite;
