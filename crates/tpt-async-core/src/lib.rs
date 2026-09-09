//! Executor-agnostic `Future` traits, `Waker` abstractions, and zero-cost
//! state-machine helpers for the `tpt-async` ecosystem.
//!
//! # Feature flags
//!
//! | Flag    | Default | What it enables |
//! |---------|---------|-----------------|
//! | `alloc` | yes     | [`JoinHandle`], [`Completer`], heap-backed shared task state |
//! | `std`   | no      | `std::error::Error` impls on error types |

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs, clippy::all)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod error;
pub mod spawn;
pub mod task;
pub mod waker;
pub mod prelude;
