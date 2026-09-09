//! Monotonic tick source abstraction.

/// A monotonic tick source.
///
/// Implement this trait for your platform to drive a [`TimerWheel`](crate::wheel::TimerWheel).
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

    /// Number of milliseconds per tick (always 1).
    pub fn millis_per_tick() -> u64 {
        1
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
