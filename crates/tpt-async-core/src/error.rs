//! Error types for `tpt-async-core`.

use core::fmt;

/// The task was cancelled before it produced a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("task was cancelled")
    }
}

#[cfg(feature = "std")]
extern crate std;
#[cfg(feature = "std")]
impl std::error::Error for Cancelled {}

/// Returned when a spawn call fails, e.g. because the executor has shut down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnError;

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to spawn task: executor unavailable")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SpawnError {}
