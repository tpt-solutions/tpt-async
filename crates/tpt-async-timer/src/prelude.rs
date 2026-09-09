//! Convenience re-exports for the most common types in `tpt-async-timer`.

pub use crate::clock::Clock;
pub use crate::interval::Interval;
pub use crate::sleep::Sleep;
pub use crate::timeout::TimedOut;
pub use crate::wheel::{TimerEntry, TimerWheel};

#[cfg(feature = "alloc")]
pub use crate::timeout::Timeout;

#[cfg(feature = "std")]
pub use crate::clock::StdClock;
