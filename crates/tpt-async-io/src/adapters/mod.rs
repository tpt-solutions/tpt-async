// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adapter types bridging other async I/O ecosystems to the `tpt-async-io`
//! traits.
//!
//! These are explicit wrapper types rather than blanket impls: coherence
//! stays clean and downstream crates can still implement
//! [`AsyncRead`](crate::AsyncRead) / [`AsyncWrite`](crate::AsyncWrite)
//! manually for their own types.

#[cfg(feature = "std")]
pub mod std_compat;

#[cfg(feature = "tokio")]
pub mod tokio_compat;

#[cfg(feature = "async-std")]
pub mod async_std_compat;
