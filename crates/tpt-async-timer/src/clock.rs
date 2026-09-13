//! Monotonic tick source abstraction.

/// A monotonic tick source.
///
/// Implement this trait for your platform to drive a
/// [`TimerWheel`](crate::wheel::TimerWheel): read `now_ticks()` each loop
/// iteration and call [`advance_to`](crate::wheel::TimerWheel::advance_to).
/// On `std`, the built-in [`driver`](crate::driver) module does this for you.
pub trait Clock {
    /// Return the current tick count.
    fn now_ticks(&self) -> u64;
}

/// A clock backed by [`std::time::Instant`], with a 1 ms tick resolution.
///
/// Only available with the **`std`** feature.
#[cfg(feature = "std")]
pub struct StdClock {
    start: std::time::Instant,
}

#[cfg(feature = "std")]
impl StdClock {
    /// Create a new `StdClock`.  The tick count starts at 0 from this moment.
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

#[cfg(feature = "std")]
impl Default for StdClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "std")]
impl Clock for StdClock {
    /// Returns elapsed milliseconds since construction.
    fn now_ticks(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

/// A clock that always reads a fixed tick count.
///
/// Useful for deterministic tests and for bridging a hardware tick counter
/// you advance manually.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now_ticks(&self) -> u64 {
        self.0
    }
}
