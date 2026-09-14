//! Convenience re-exports for the most common types in `tpt-async-timer`.

pub use crate::clock::{Clock, FixedClock};
pub use crate::interval::{Interval, MissedTickBehavior, Tick};
pub use crate::sleep::Sleep;
pub use crate::timeout::{TimedOut, Timeout};
pub use crate::wheel::TimerWheel;

#[cfg(feature = "std")]
pub use crate::clock::StdClock;
#[cfg(feature = "std")]
pub use crate::driver::{interval, sleep, timeout};
#[cfg(feature = "std")]
pub use crate::retry::{retry, RetryPolicy};
