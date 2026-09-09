//! Heapless const-generic hierarchical timer wheel.
//!
//! # Feature flags
//!
//! | Flag    | Default | What it enables |
//! |---------|---------|-----------------|
//! | `alloc` | yes (via std) | [`timeout::Timeout`] |
//! | `std`   | yes     | [`clock::StdClock`] |

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs, clippy::all)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod clock;
pub mod interval;
pub mod prelude;
pub mod sleep;
pub mod timeout;
pub mod wheel;
