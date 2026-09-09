//! The [`Spawn`] and [`LocalSpawn`] traits.
//!
//! Implement these on your executor to make it compatible with the rest of the
//! `tpt-async` ecosystem.  The provided [`LocalExecutor`](crate::prelude)
//! from `tpt-async-executor` implements both.

use core::future::Future;

use crate::error::SpawnError;

#[cfg(feature = "alloc")]
use crate::task::JoinHandle;

/// Spawn `Send` futures onto a (possibly multi-threaded) executor.
///
/// Implement this trait on your runtime's handle to get interoperability with
/// the `tpt-async` ecosystem.
pub trait Spawn {
    /// Spawn `future` and return a [`JoinHandle`] that resolves to its output.
    #[cfg(feature = "alloc")]
    fn spawn<F>(&self, future: F) -> Result<JoinHandle<F::Output>, SpawnError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static;
}

/// Spawn `!Send` futures onto a single-threaded executor.
pub trait LocalSpawn {
    /// Spawn `future` on the current thread and return a handle to its output.
    #[cfg(feature = "alloc")]
    fn spawn_local<F>(&self, future: F) -> Result<JoinHandle<F::Output>, SpawnError>
    where
        F: Future + 'static,
        F::Output: 'static;
}
