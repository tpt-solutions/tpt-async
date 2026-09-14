//! Heapless const-generic hierarchical timer wheel.
//!
//! The wheel is the scheduling core of `tpt-async`: O(1) insertion/removal,
//! zero heap allocation, usable on bare metal.  All wheel methods take
//! `&self` — the wheel can be shared with a driver thread via plain shared
//! references.
//!
//! # Feature flags
//!
//! | Flag    | Default | What it enables |
//! |---------|---------|-----------------|
//! | `alloc` | yes (via `std`) | reserved (no heap use is currently required) |
//! | `std`   | yes     | the [`driver`] module: background ticking thread plus [`driver::sleep`], [`driver::timeout`], [`driver::interval`] |
//!
//! # Driving the wheel
//!
//! On `std`, the free functions are all you need:
//!
//! ```rust,no_run
//! use tpt_async_timer::driver::{sleep, timeout};
//! use core::time::Duration;
//!
//! # async fn demo() -> Result<(), tpt_async_timer::timeout::TimedOut> {
//! sleep(Duration::from_millis(25)).await;
//! timeout(Duration::from_secs(1), async { /* work */ }).await?;
//! # Ok(())
//! # }
//! ```
//!
//! On bare metal, construct a wheel (a static works — all methods take
//! `&self`), register [`Sleep`](sleep::Sleep)/[`Timeout`](timeout::Timeout)/
//! [`Interval`](interval::Interval) futures against it, and advance it from
//! your tick interrupt or main loop with
//! [`advance_to`](wheel::TimerWheel::advance_to).

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs, clippy::all)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod clock;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod driver;
pub mod interval;
pub mod prelude;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod retry;
pub mod sleep;
pub mod timeout;
pub mod wheel;
