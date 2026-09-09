// Copyright TPT Solutions. Dual-licensed under MIT OR Apache-2.0.

//! Single-threaded cooperative executor for the `tpt-async` ecosystem.
//!
//! [`LocalExecutor`] drives `!Send` (and `Send`) futures on the calling thread.
//! It implements both [`Spawn`] and [`LocalSpawn`] from `tpt-async-core`.
//!
//! # Example
//!
//! ```rust
//! use tpt_async_executor::LocalExecutor;
//! use tpt_async_core::spawn::LocalSpawn as _;
//!
//! let executor = LocalExecutor::new();
//! let ex = executor.clone();
//! let answer = executor.block_on(async move {
//!     let handle = ex.spawn_local(async { 6 * 7u32 }).unwrap();
//!     handle.await.unwrap()
//! });
//! assert_eq!(answer, 42);
//! ```

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::{pin, Pin};
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use tpt_async_core::error::SpawnError;
use tpt_async_core::spawn::{LocalSpawn, Spawn};
use tpt_async_core::task::{Completer, JoinHandle};

// ---------------------------------------------------------------------------
// Executor internals
// ---------------------------------------------------------------------------

/// Shared state owned by every clone of a [`LocalExecutor`].
struct Inner {
    queue: RefCell<VecDeque<Rc<Task>>>,
}

/// A single runnable unit: a type-erased `()` future plus a back-reference to
/// the shared run queue.
struct Task {
    future: RefCell<Pin<Box<dyn Future<Output = ()>>>>,
    inner: Rc<Inner>,
}

impl Task {
    /// Build a [`Waker`] that re-enqueues this task when invoked.
    ///
    /// The waker takes ownership of one Rc strong-count increment.
    fn waker(self: &Rc<Self>) -> Waker {
        // SAFETY: `self` is a live Rc<Task>; we increment its strong count to
        // give the returned Waker its own independent ownership of the Task.
        unsafe { Rc::increment_strong_count(Rc::as_ptr(self)) };
        let raw = RawWaker::new(Rc::as_ptr(self) as *const (), &TASK_VTABLE);
        // SAFETY: `raw` holds a valid Rc<Task> raw pointer; TASK_VTABLE
        // correctly implements the RawWaker contract (clone/wake/drop are each
        // documented at their definition site below).
        unsafe { Waker::from_raw(raw) }
    }

    /// Poll the task's future once, using a waker that re-enqueues on wake.
    fn poll_once(self: &Rc<Self>) {
        let waker = self.waker();
        let mut cx = Context::from_waker(&waker);
        // Discard Poll::Pending — the waker will re-enqueue the task when its
        // dependencies become ready.
        let _ = self.future.borrow_mut().as_mut().poll(&mut cx);
    }
}

// ── Task waker vtable ────────────────────────────────────────────────────────
//
// The data pointer stored inside every task RawWaker is a raw pointer obtained
// from `Rc::as_ptr` / `Rc::into_raw`.  The strong count of the Rc is always
// exactly one higher than the number of *live* Wakers that hold this pointer.

unsafe fn task_clone(ptr: *const ()) -> RawWaker {
    // SAFETY: `ptr` is a valid Rc<Task> raw pointer with at least one live
    // reference.  Incrementing the strong count produces a new independent
    // reference that the clone can own.
    unsafe { Rc::<Task>::increment_strong_count(ptr as *const Task) };
    RawWaker::new(ptr, &TASK_VTABLE)
}

unsafe fn task_wake(ptr: *const ()) {
    // SAFETY: `ptr` was produced by `Rc::into_raw` / `Rc::as_ptr`; the caller
    // (the Waker machinery) transfers ownership to this function, so we
    // reconstruct the Rc and transfer it into the run queue without adjusting
    // the strong count.
    let rc = unsafe { Rc::from_raw(ptr as *const Task) };
    let inner = Rc::clone(&rc.inner);
    inner.queue.borrow_mut().push_back(rc);
}

unsafe fn task_wake_by_ref(ptr: *const ()) {
    // SAFETY: `ptr` is valid for the duration of this call but is NOT
    // consumed.  We increment the strong count to produce a *new* Rc that we
    // then push onto the queue, leaving the original waker's reference intact.
    unsafe { Rc::<Task>::increment_strong_count(ptr as *const Task) };
    let rc = unsafe { Rc::from_raw(ptr as *const Task) };
    let inner = Rc::clone(&rc.inner);
    inner.queue.borrow_mut().push_back(rc);
}

unsafe fn task_drop(ptr: *const ()) {
    // SAFETY: `ptr` was produced by `Rc::into_raw` / `Rc::as_ptr`; dropping
    // the reconstructed Rc decrements the strong count and frees the
    // allocation when it reaches zero.
    drop(unsafe { Rc::from_raw(ptr as *const Task) });
}

static TASK_VTABLE: RawWakerVTable =
    RawWakerVTable::new(task_clone, task_wake, task_wake_by_ref, task_drop);

// ── Notify waker (used for the main future inside `block_on`) ────────────────
//
// The data pointer is a raw pointer to an `Rc<Cell<bool>>`.  When woken, the
// waker simply sets the boolean flag to `true`; the `block_on` loop checks the
// flag to know when to re-poll the main future.

unsafe fn notify_clone(ptr: *const ()) -> RawWaker {
    // SAFETY: `ptr` is a valid Rc<Cell<bool>> raw pointer; incrementing its
    // strong count gives the clone its own independent reference.
    unsafe { Rc::<Cell<bool>>::increment_strong_count(ptr as *const Cell<bool>) };
    RawWaker::new(ptr, &NOTIFY_VTABLE)
}

unsafe fn notify_wake(ptr: *const ()) {
    // SAFETY: `ptr` is owned by this waker; we reconstruct the Rc, set the
    // flag, and then let the Rc drop (decrementing the strong count).
    let rc = unsafe { Rc::from_raw(ptr as *const Cell<bool>) };
    rc.set(true);
    // rc drops here.
}

unsafe fn notify_wake_by_ref(ptr: *const ()) {
    // SAFETY: `ptr` is valid for this call but NOT consumed; we borrow the
    // underlying Cell, set the flag, then forget the Rc so the refcount stays
    // unchanged.
    let rc = unsafe { Rc::from_raw(ptr as *const Cell<bool>) };
    rc.set(true);
    core::mem::forget(rc);
}

unsafe fn notify_drop(ptr: *const ()) {
    // SAFETY: `ptr` was produced by `Rc::into_raw` / `Rc::as_ptr`; drop
    // decrements the strong count.
    drop(unsafe { Rc::from_raw(ptr as *const Cell<bool>) });
}

static NOTIFY_VTABLE: RawWakerVTable =
    RawWakerVTable::new(notify_clone, notify_wake, notify_wake_by_ref, notify_drop);

/// Create a [`Waker`] that sets `flag` to `true` when woken.
fn make_notify_waker(flag: &Rc<Cell<bool>>) -> Waker {
    // SAFETY: `flag` is a live Rc<Cell<bool>>; we increment its strong count
    // so the Waker owns its own independent reference.
    unsafe { Rc::increment_strong_count(Rc::as_ptr(flag)) };
    let raw = RawWaker::new(Rc::as_ptr(flag) as *const (), &NOTIFY_VTABLE);
    // SAFETY: `raw` satisfies the RawWaker contract; NOTIFY_VTABLE manages the
    // Rc<Cell<bool>> lifetime correctly (see each function above).
    unsafe { Waker::from_raw(raw) }
}

// ---------------------------------------------------------------------------
// LocalExecutor — public API
// ---------------------------------------------------------------------------

/// A single-threaded cooperative executor.
///
/// All futures are polled on the thread that calls [`block_on`] or
/// [`run_until_stalled`].  Both `Send` and `!Send` futures are accepted.
///
/// `LocalExecutor` is cheaply cloneable: every clone shares the same run
/// queue, so handles can be moved into spawned futures to allow nested
/// spawning.
///
/// # WASM
///
/// `LocalExecutor` compiles and runs correctly on `wasm32-unknown-unknown`.
///
/// [`block_on`]: LocalExecutor::block_on
/// [`run_until_stalled`]: LocalExecutor::run_until_stalled
#[derive(Clone)]
pub struct LocalExecutor {
    inner: Rc<Inner>,
}

impl LocalExecutor {
    /// Create a new, empty executor.
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                queue: RefCell::new(VecDeque::new()),
            }),
        }
    }

    /// Run `future` to completion, driving all concurrently-spawned tasks.
    ///
    /// The executor polls `future` and any tasks in the run queue in a loop
    /// until `future` returns [`Poll::Ready`].  Spawned tasks are polled in
    /// FIFO order.
    ///
    /// # Panics
    ///
    /// Panics if the executor stalls — i.e. the main future is pending, its
    /// waker was not fired, and the run queue is empty.  This indicates a
    /// deadlock or a future that never registers a waker.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        // The notify flag starts `true` so we poll the main future on the
        // very first iteration without waiting for an external wake.
        let notified = Rc::new(Cell::new(true));
        let waker = make_notify_waker(&notified);
        let mut future = pin!(future);

        loop {
            // ── 1. Poll the main future if its waker fired. ──────────────
            if notified.replace(false) {
                let mut cx = Context::from_waker(&waker);
                if let Poll::Ready(val) = future.as_mut().poll(&mut cx) {
                    return val;
                }
            }

            // ── 2. Drain the run queue (FIFO). ───────────────────────────
            // Tasks added to the queue while draining (e.g. from nested
            // spawns or waker callbacks) are picked up in subsequent pops.
            loop {
                let task = self.inner.queue.borrow_mut().pop_front();
                match task {
                    Some(t) => t.poll_once(),
                    None => break,
                }
            }

            // ── 3. Stall detection. ──────────────────────────────────────
            // After draining, if the main future was not re-notified and the
            // queue is empty, nothing can make further progress on this thread.
            if !notified.get() && self.inner.queue.borrow().is_empty() {
                panic!(
                    "LocalExecutor stalled: the main future is pending and \
                     no tasks are ready to run"
                );
            }
        }
    }

    /// Poll all ready tasks until the run queue is empty, then return.
    ///
    /// Does not drive any particular "main" future; useful for flushing
    /// background work or for test scenarios where futures are pre-loaded.
    pub fn run_until_stalled(&self) {
        loop {
            let task = self.inner.queue.borrow_mut().pop_front();
            match task {
                Some(t) => t.poll_once(),
                None => return,
            }
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Push a type-erased `()` future onto the run queue.
    fn enqueue(&self, fut: Pin<Box<dyn Future<Output = ()>>>) {
        let task = Rc::new(Task {
            future: RefCell::new(fut),
            inner: Rc::clone(&self.inner),
        });
        self.inner.queue.borrow_mut().push_back(task);
    }

    /// Wrap `future` so its output is delivered through a [`JoinHandle`]
    /// via [`Completer`], then enqueue it.
    fn spawn_erased<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (handle, completer) = Completer::new();
        self.enqueue(Box::pin(async move {
            completer.complete(future.await);
        }));
        handle
    }
}

impl Default for LocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Spawn for LocalExecutor {
    /// Spawn a `Send` future on this single-threaded executor.
    ///
    /// Even though `LocalExecutor` is single-threaded, accepting `Send`
    /// futures is sound because they are polled exclusively on the calling
    /// thread.
    fn spawn<F>(&self, future: F) -> Result<JoinHandle<F::Output>, SpawnError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        Ok(self.spawn_erased(future))
    }
}

impl LocalSpawn for LocalExecutor {
    /// Spawn a `!Send` future on this executor.
    fn spawn_local<F>(&self, future: F) -> Result<JoinHandle<F::Output>, SpawnError>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        Ok(self.spawn_erased(future))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `block_on` returns the future's direct output value.
    #[test]
    fn block_on_returns_value() {
        let executor = LocalExecutor::new();
        assert_eq!(executor.block_on(async { 42 }), 42);
    }

    /// A value produced by a spawned task is accessible via its `JoinHandle`.
    #[test]
    fn spawn_and_join() {
        let executor = LocalExecutor::new();
        // Clone the executor so the handle can be moved into the `'static`
        // async block without borrowing `executor`.
        let ex = executor.clone();
        let result = executor.block_on(async move {
            let handle = ex.spawn_local(async { 99u32 }).unwrap();
            handle.await.unwrap()
        });
        assert_eq!(result, 99u32);
    }

    /// A task spawned from inside another spawned task completes correctly.
    #[test]
    fn nested_spawn() {
        let executor = LocalExecutor::new();
        let ex = executor.clone();
        let result = executor.block_on(async move {
            let ex2 = ex.clone();
            let outer = ex
                .spawn_local(async move {
                    let inner = ex2.spawn_local(async { 1u32 }).unwrap();
                    inner.await.unwrap() + 1
                })
                .unwrap();
            outer.await.unwrap()
        });
        assert_eq!(result, 2u32);
    }

    /// Tasks that are spawned earlier are polled before later-spawned tasks
    /// (FIFO run order is preserved).
    #[test]
    fn wake_ordering() {
        let executor = LocalExecutor::new();
        let log: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));

        let ex = executor.clone();
        let log1 = Rc::clone(&log);
        let log2 = Rc::clone(&log);
        let log3 = Rc::clone(&log);

        executor.block_on(async move {
            let h1 = ex
                .spawn_local(async move {
                    log1.borrow_mut().push(1);
                })
                .unwrap();
            let h2 = ex
                .spawn_local(async move {
                    log2.borrow_mut().push(2);
                })
                .unwrap();
            let h3 = ex
                .spawn_local(async move {
                    log3.borrow_mut().push(3);
                })
                .unwrap();

            h1.await.unwrap();
            h2.await.unwrap();
            h3.await.unwrap();
        });

        assert_eq!(*log.borrow(), vec![1, 2, 3]);
    }
}
