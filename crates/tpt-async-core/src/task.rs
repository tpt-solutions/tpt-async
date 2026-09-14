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

    /// Returns `true` if the task has already finished (successfully or by
    /// cancellation).
    pub fn is_finished(&self) -> bool {
        self.shared.flags.load(Ordering::Acquire) != PENDING
    }

    /// Cancel the task: the handle resolves to `Err(Cancelled)` and the
    /// executor will discard the result if the task later completes.
    ///
    /// This is advisory — the underlying future is dropped by the executor,
    /// not by this call.
    pub fn cancel(&self) {
        self.shared.cancel();
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, Cancelled>;

    /// Polls the handle.  The first `Ready` poll consumes the result; a
    /// subsequent poll after `Ready(Ok(_))` panics (the result is gone).
    ///
    /// # Panics
    /// Panics if polled again after having returned `Ready(Ok(_))` — the
    /// result value has already been handed out and cannot be produced a
    /// second time.  `Ready(Err(Cancelled))` is repeatable.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.shared.flags.load(Ordering::Acquire) {
            READY => {
                let taken = self.shared.result.lock().take();
                debug_assert!(taken.is_some(), "JoinHandle polled after completion");
                Poll::Ready(Ok(
                    taken.expect("JoinHandle polled after already yielding Ready(Ok(_))")
                ))
            }
            CANCELLED => Poll::Ready(Err(Cancelled)),
            _ => {
                // Register waker before re-checking to avoid a race.
                let mut slot = self.shared.waker.lock();
                if !slot.as_ref().is_some_and(|w| w.will_wake(cx.waker())) {
                    *slot = Some(cx.waker().clone());
                }
                drop(slot);
                match self.shared.flags.load(Ordering::Acquire) {
                    READY => {
                        let taken = self.shared.result.lock().take();
                        debug_assert!(taken.is_some(), "JoinHandle polled after completion");
                        Poll::Ready(Ok(
                            taken.expect("JoinHandle polled after already yielding Ready(Ok(_))")
                        ))
                    }
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

/// A spawned-task handle with **cancel-on-drop** semantics.
///
/// `Task<T>` behaves like [`JoinHandle<T>`] except that dropping it cancels
/// the task instead of detaching it — the mirror image of tokio's
/// `JoinHandle` (detach-on-drop) and the analogue of `smol::Task`.
///
/// Cancellation is *handle-level*: the handle resolves to
/// [`Err(Cancelled)`](Cancelled) and executors built on this crate stop
/// reporting results.  Dropping the underlying future remains the
/// executor's decision.
///
/// Call [`detach`](Task::detach) to convert a `Task` back into a plain
/// [`JoinHandle`] (fire-and-forget).
pub struct Task<T> {
    inner: JoinHandle<T>,
}

impl<T> Task<T> {
    /// Wrap a [] into a cancel-on-drop [].
    ///
    /// Executors call this after spawning; not intended for end users
    /// (use [] pairs or an executor spawn API instead).
    pub fn from_join_handle(inner: JoinHandle<T>) -> Self {
        Self { inner }
    }

    /// Detach: the task keeps running and its result is discarded.
    pub fn detach(self) -> JoinHandle<T> {
        // Take `inner` without running `Task`'s Drop (which would cancel) —
        // exactly the detach contract.
        let this = core::mem::ManuallyDrop::new(self);
        // SAFETY: `inner` is moved out exactly once and the ManuallyDrop
        // wrapper guarantees `Task::drop` never runs afterwards.
        unsafe { core::ptr::read(&this.inner) }
    }

    /// Cancel the task early (also happens on drop).
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Returns `true` if the task has finished (or been cancelled).
    pub fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }
}

impl<T> Future for Task<T> {
    type Output = Result<T, Cancelled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Task is a transparent wrapper; JoinHandle is Unpin.
        Pin::new(&mut self.get_mut().inner).poll(cx)
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        if !self.inner.is_finished() {
            self.inner.cancel();
        }
    }
}

impl<T> core::fmt::Debug for Task<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Task")
            .field("finished", &self.is_finished())
            .finish()
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

    #[test]
    fn task_cancel_marks_finished() {
        let (handle, _completer) = Completer::<u32>::new();
        let task = Task::from_join_handle(handle);
        assert!(!task.is_finished());
        task.cancel();
        assert!(task.is_finished());
        // Completing after cancellation must not resurrect the task.
        _completer.complete(7);
        assert!(task.is_finished());
    }

    #[test]
    fn task_detach_preserves_completion() {
        let (handle, completer) = Completer::<u32>::new();
        let task = Task::from_join_handle(handle);
        let join = task.detach(); // detach must NOT cancel
        completer.complete(7);
        assert!(join.is_finished());
    }

    #[test]
    fn task_poll_returns_cancelled_after_cancel() {
        use core::pin::pin;
        use core::task::{Context, Poll};

        let (handle, _completer) = Completer::<u32>::new();
        let mut task = pin!(Task::from_join_handle(handle));
        let waker = crate::waker::noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert!(task.as_mut().poll(&mut cx).is_pending());
        task.as_mut().cancel();
        assert!(matches!(
            task.as_mut().poll(&mut cx),
            Poll::Ready(Err(Cancelled))
        ));
    }
}
