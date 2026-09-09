//! [`JoinHandle`] and the shared state it relies on.

#![cfg(feature = "alloc")]

use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU8, Ordering};
use core::task::{Context, Poll, Waker};

use spin::Mutex;

use crate::error::Cancelled;

// State flags stored in `SharedState::flags`.
const PENDING: u8 = 0;
const READY: u8 = 1;
const CANCELLED: u8 = 2;

/// Shared backing store between a task and its [`JoinHandle`].
pub(crate) struct SharedState<T> {
    flags: AtomicU8,
    result: Mutex<Option<T>>,
    waker: Mutex<Option<Waker>>,
}

impl<T> SharedState<T> {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            flags: AtomicU8::new(PENDING),
            result: Mutex::new(None),
            waker: Mutex::new(None),
        })
    }

    /// Called by the executor when the task completes successfully.
    pub(crate) fn complete(&self, value: T) {
        *self.result.lock() = Some(value);
        self.flags.store(READY, Ordering::Release);
        if let Some(w) = self.waker.lock().take() {
            w.wake();
        }
    }

    /// Mark the task as cancelled (e.g. executor shut down).
    pub(crate) fn cancel(&self) {
        self.flags.store(CANCELLED, Ordering::Release);
        if let Some(w) = self.waker.lock().take() {
            w.wake();
        }
    }
}

/// A handle to a spawned task's eventual output.
///
/// `JoinHandle<T>` implements [`Future`]; awaiting it yields `Result<T, Cancelled>`.
pub struct JoinHandle<T> {
    shared: Arc<SharedState<T>>,
}

impl<T> JoinHandle<T> {
    pub(crate) fn new(shared: Arc<SharedState<T>>) -> Self {
        Self { shared }
    }

    /// Returns `true` if the task has already finished.
    pub fn is_finished(&self) -> bool {
        self.shared.flags.load(Ordering::Acquire) != PENDING
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, Cancelled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.shared.flags.load(Ordering::Acquire) {
            READY => Poll::Ready(Ok(self.shared.result.lock().take().expect("result missing"))),
            CANCELLED => Poll::Ready(Err(Cancelled)),
            _ => {
                // Register waker before re-checking to avoid a race.
                *self.shared.waker.lock() = Some(cx.waker().clone());
                match self.shared.flags.load(Ordering::Acquire) {
                    READY => Poll::Ready(Ok(self.shared.result.lock().take().expect("result missing"))),
                    CANCELLED => Poll::Ready(Err(Cancelled)),
                    _ => Poll::Pending,
                }
            }
        }
    }
}

/// The producer end of a [`JoinHandle`] — given to the executor.
///
/// The executor calls [`Completer::complete`] when the task finishes.
pub struct Completer<T> {
    shared: Arc<SharedState<T>>,
}

impl<T> Completer<T> {
    /// Allocate a linked `(JoinHandle, Completer)` pair.
    pub fn new() -> (JoinHandle<T>, Self) {
        let shared = SharedState::new();
        (JoinHandle::new(Arc::clone(&shared)), Self { shared })
    }

    /// Deliver `value` to the waiting [`JoinHandle`].
    pub fn complete(self, value: T) {
        self.shared.complete(value);
    }
}

impl<T> Drop for Completer<T> {
    fn drop(&mut self) {
        // If the completer is dropped without completing, cancel the handle.
        if self.shared.flags.load(Ordering::Acquire) == PENDING {
            self.shared.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_resolves_handle() {
        let (handle, completer) = Completer::<u32>::new();
        assert!(!handle.is_finished());
        completer.complete(42);
        assert!(handle.is_finished());
    }

    #[test]
    fn drop_completer_cancels_handle() {
        let (handle, completer) = Completer::<u32>::new();
        drop(completer);
        assert!(handle.is_finished());
    }
}
