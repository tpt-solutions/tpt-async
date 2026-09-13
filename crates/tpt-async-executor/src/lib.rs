// Copyright TPT Solutions. Dual-licensed under MIT OR Apache-2.0.

//! Single-threaded cooperative executor for the `tpt-async` ecosystem.
//!
//! [`LocalExecutor`] drives `!Send` (and `Send`) futures on the calling
//! thread.  It implements both [`Spawn`] and [`LocalSpawn`] from
//! `tpt-async-core`.
//!
//! # Thread-safety model
//!
//! Futures are **polled only on the executor thread**, but the wakers handed
//! to them are fully thread-safe: waking from any thread merely schedules the
//! task and notifies the executor.  This means a task may await anything that
//! wakes from elsewhere — the timer crate's std driver thread, a channel, an
//! OS thread — without unsynchronized access to task state.
//!
//! The [`LocalExecutor`] handle itself is `Clone` (clones share the run
//! queue) but **not** `Send`: spawning and polling must happen on the thread
//! that owns the executor.
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

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::{pin, Pin};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};

use tpt_async_core::error::SpawnError;
use tpt_async_core::spawn::{LocalSpawn, Spawn};
use tpt_async_core::task::{Completer, JoinHandle};

// ---------------------------------------------------------------------------
// Thread-safe scheduling core (shared with wakers)
// ---------------------------------------------------------------------------

/// Per-task scheduling flags, shared between the executor thread and any
/// thread that might wake one of its wakers.
struct TaskState {
    /// Set when the task is finished (polled to completion or cancelled);
    /// further wakes are ignored.
    done: AtomicBool,
    /// Set while the task's id sits in the run queue, so duplicate wakes
    /// coalesce into a single poll.
    queued: AtomicBool,
}

impl TaskState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            done: AtomicBool::new(false),
            queued: AtomicBool::new(false),
        })
    }
}

/// What a waker should wake: the block_on main future or a spawned task.
enum Target {
    /// Re-poll the main future.
    Main,
    /// Re-poll the spawned task with the given id.
    Task { id: u64, state: Arc<TaskState> },
}

/// Thread-safe scheduling core shared by every executor clone and every
/// waker.  Only ids and atomics live here — never task futures — so it is
/// sound to touch from any thread.
struct Inner {
    /// Ids of tasks that are ready to be polled.  The *futures* stay in the
    /// executor's thread-local registry; only this id crosses threads.
    queue: Mutex<VecDeque<u64>>,
    /// Notified whenever the queue gains an entry or the main future is
    /// re-notified; `block_on` parks here.
    signal: Condvar,
    next_id: AtomicU64,
}

impl Inner {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            signal: Condvar::new(),
            next_id: AtomicU64::new(1),
        }
    }

    fn allocate_id(&self) -> u64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Mark the target as ready and wake the executor if it is parked.
    fn wake_target(&self, target: &Target) {
        match target {
            Target::Main => {
                // The `notified` flag lives inside the waker data (see
                // `MainWaker`); here we only need to release the parker.
                let guard = self.queue.lock().expect("executor queue poisoned");
                self.signal.notify_all();
                drop(guard);
            }
            Target::Task { id, state } => {
                if state.done.load(Ordering::Acquire) {
                    return;
                }
                if state.queued.swap(true, Ordering::AcqRel) {
                    return; // already scheduled; duplicate wake coalesced
                }
                let mut queue = self.queue.lock().expect("executor queue poisoned");
                queue.push_back(*id);
                self.signal.notify_all();
            }
        }
    }
}

/// Waker payload: everything needed to schedule a task from any thread.
struct WakerData {
    inner: Arc<Inner>,
    target: Target,
}

impl Wake for WakerData {
    fn wake(self: Arc<Self>) {
        self.inner.wake_target(&self.target);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.inner.wake_target(&self.target);
    }
}

/// Waker payload for the `block_on` main future.
struct MainWaker {
    inner: Arc<Inner>,
    notified: Arc<AtomicBool>,
}

impl Wake for MainWaker {
    fn wake(self: Arc<Self>) {
        self.notified.store(true, Ordering::Release);
        self.inner.wake_target(&Target::Main);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        Wake::wake(self.clone());
    }
}

fn waker_for(data: WakerData) -> Waker {
    Waker::from(Arc::new(data))
}

// ---------------------------------------------------------------------------
// LocalExecutor — public API
// ---------------------------------------------------------------------------

/// A spawned task: its boxed future (stable address, may be `!Send`) plus the
/// scheduling flags shared with its wakers.
struct TaskEntry {
    body: Pin<Box<dyn Future<Output = ()>>>,
    state: Arc<TaskState>,
}

/// A single-threaded cooperative executor.
///
/// All futures are polled on the thread that calls [`block_on`] or
/// [`run_until_stalled`].  Both `Send` and `!Send` futures are accepted.
/// Wakes may come from any thread (see the [thread-safety model](self#thread-safety-model)).
///
/// `LocalExecutor` is cheaply cloneable: every clone shares the same run
/// queue, so handles can be moved into spawned futures to allow nested
/// spawning.  The handle itself is not `Send` — clone it before moving work
/// to another thread is not supported; use another executor there.
///
/// # Blocking
///
/// [`block_on`] parks the thread when nothing is runnable, so futures that
/// await external events (timers, channels, other threads) work as expected.
///
/// # WASM
///
/// On `wasm32-unknown-unknown` there is no thread parking; `block_on` falls
/// back to its historical behaviour of panicking when the main future is
/// pending and nothing is runnable ("stall").  Prefer keeping the main
/// future always immediately resumable there.
///
/// [`block_on`]: LocalExecutor::block_on
/// [`run_until_stalled`]: LocalExecutor::run_until_stalled
#[derive(Clone)]
pub struct LocalExecutor {
    inner: Arc<Inner>,
    /// id → task registry.  Only touched from the executor thread; the
    /// futures inside may be `!Send`, which is why the executor handle is
    /// not `Send`.
    registry: Rc<RefCell<HashMap<u64, TaskEntry>>>,
}

impl LocalExecutor {
    /// Create a new, empty executor.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner::new()),
            registry: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Run `future` to completion, driving all concurrently-spawned tasks.
    ///
    /// The executor polls `future` and queued tasks until `future` resolves.
    /// When nothing is runnable the thread parks (except on WASM, see the
    /// type docs) until the next wake arrives — from a timer, another
    /// thread, or a channel.
    ///
    /// Spawned tasks still queued when `block_on` returns are dropped, which
    /// cancels them: their [`JoinHandle`]s resolve to
    /// `Err(SpawnError::Cancelled)` if polled afterwards.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        // The notify flag starts `true` so we poll the main future on the
        // very first iteration without waiting for an external wake.
        let notified = Arc::new(AtomicBool::new(true));
        let main_waker = Waker::from(Arc::new(MainWaker {
            inner: Arc::clone(&self.inner),
            notified: Arc::clone(&notified),
        }));
        let mut future = pin!(future);

        loop {
            // ── 1. Poll the main future if its waker fired. ──────────────
            if notified.swap(false, Ordering::AcqRel) {
                let mut cx = Context::from_waker(&main_waker);
                if let Poll::Ready(val) = future.as_mut().poll(&mut cx) {
                    return val;
                }
            }

            // ── 2. Poll every scheduled task. ────────────────────────────
            self.poll_scheduled_tasks();

            // ── 3. Park until something wakes, or loop if already awake. ─
            let queue = self.inner.queue.lock().expect("executor queue poisoned");
            if notified.load(Ordering::Acquire) || !queue.is_empty() {
                continue; // work arrived while we were holding the lock
            }
            self.park(queue);
        }
    }

    /// Park until the next wake.  On non-WASM targets this blocks the thread
    /// (bounded, so a missed notify can never hang forever); on WASM it
    /// panics — see the type docs.
    #[cfg(not(target_family = "wasm"))]
    fn park(&self, queue: std::sync::MutexGuard<'_, VecDeque<u64>>) {
        // Cap the wait so clock changes or a lost notify can't hang forever.
        const IDLE_CAP: std::time::Duration = std::time::Duration::from_millis(100);
        let _ = self
            .inner
            .signal
            .wait_timeout(queue, IDLE_CAP)
            .expect("executor queue poisoned");
        // Loop back: the caller re-checks notified/queue under the lock.
    }

    /// WASM fallback: no parking available; report the stall loudly.
    #[cfg(target_family = "wasm")]
    fn park(&self, _queue: std::sync::MutexGuard<'_, VecDeque<u64>>) {
        panic!(
            "LocalExecutor stalled: the main future is pending and no tasks \
             are ready to run (parking is unavailable on WASM)"
        );
    }

    /// Poll all tasks currently in the run queue (and anything scheduled
    /// while polling them).
    fn poll_scheduled_tasks(&self) {
        loop {
            let id = {
                let mut queue = self.inner.queue.lock().expect("executor queue poisoned");
                queue.pop_front()
            };
            let Some(id) = id else { return };
            self.poll_task(id);
        }
    }

    /// Poll a single task by id.
    fn poll_task(&self, id: u64) {
        // Take the entry out so polling can freely spawn/complete tasks that
        // touch the registry.
        let mut entry = match self.registry.borrow_mut().remove(&id) {
            Some(e) => e,
            None => return, // already completed and deregistered
        };
        entry.state.queued.store(false, Ordering::Release);

        let waker = waker_for(WakerData {
            inner: Arc::clone(&self.inner),
            target: Target::Task {
                id,
                state: Arc::clone(&entry.state),
            },
        });
        let mut cx = Context::from_waker(&waker);

        if let Poll::Ready(()) = entry.body.as_mut().poll(&mut cx) {
            entry.state.done.store(true, Ordering::Release);
            // Entry (and its future) is dropped: not re-registered.
        } else {
            self.registry.borrow_mut().insert(id, entry);
        }
    }

    /// Poll all ready tasks until the run queue is empty, then return.
    ///
    /// Does not drive any particular "main" future; useful for flushing
    /// background work or for test scenarios where futures are pre-loaded.
    pub fn run_until_stalled(&self) {
        self.poll_scheduled_tasks();
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Register a type-erased future and enqueue it.
    fn enqueue(&self, body: Pin<Box<dyn Future<Output = ()>>>) -> u64 {
        let id = self.inner.allocate_id();
        let state = TaskState::new();
        // Starts queued (we push it below); poll_task clears the flag.
        state.queued.store(true, Ordering::Release);
        self.registry
            .borrow_mut()
            .insert(id, TaskEntry { body, state });
        self.inner
            .queue
            .lock()
            .expect("executor queue poisoned")
            .push_back(id);
        self.inner.signal.notify_all();
        id
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

/// Convenience re-exports for typical use.
pub mod prelude {
    pub use crate::LocalExecutor;
    pub use tpt_async_core::spawn::{LocalSpawn, Spawn};
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

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

    /// A future that parks until a helper thread wakes it — from *another*
    /// thread.  Proves the wake path is thread-safe and that `block_on`
    /// really parks instead of spinning or panicking.
    #[test]
    fn cross_thread_wake_awakens_parked_executor() {
        let executor = LocalExecutor::new();
        let flag = Arc::new(AtomicBool::new(false));
        let waker_slot: Arc<StdMutex<Option<Waker>>> = Arc::new(StdMutex::new(None));

        struct WaitUntil {
            flag: Arc<AtomicBool>,
            waker_slot: Arc<StdMutex<Option<Waker>>>,
        }
        impl Future for WaitUntil {
            type Output = ();
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
                if self.flag.load(Ordering::Acquire) {
                    Poll::Ready(())
                } else {
                    *self.waker_slot.lock().unwrap() = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        }

        let slot = Arc::clone(&waker_slot);
        let flag2 = Arc::clone(&flag);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            flag2.store(true, Ordering::Release);
            // Fire the executor thread's waker from THIS thread.
            if let Some(w) = slot.lock().unwrap().take() {
                w.wake();
            }
        });

        executor.block_on(WaitUntil { flag, waker_slot });
    }

    /// Waking a task's waker several times before the executor runs it
    /// results in a single poll, not three.
    #[test]
    fn duplicate_wakes_coalesce() {
        static POLLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

        struct WakeThrice;
        impl Future for WakeThrice {
            type Output = ();
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
                POLLS.fetch_add(1, Ordering::SeqCst);
                let n = POLLS.load(Ordering::SeqCst);
                if n == 1 {
                    // Wake three times while we are being polled.
                    cx.waker().wake_by_ref();
                    cx.waker().wake_by_ref();
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            }
        }

        let executor = LocalExecutor::new();
        let ex = executor.clone();
        executor.block_on(async move {
            let handle = ex.spawn_local(WakeThrice).unwrap();
            handle.await.unwrap();
        });

        // One initial poll + one coalesced poll (the three wakes collapse
        // into a single re-schedule), then done.
        assert_eq!(POLLS.load(Ordering::SeqCst), 2);
    }

    /// Dropping the executor drops pending task futures, which cancels their
    /// `Completer`s: the `JoinHandle` then resolves to `Err(Cancelled)`.
    #[test]
    fn dropped_executor_cancels_tasks() {
        use tpt_async_core::error::Cancelled;

        let handle;
        {
            let executor = LocalExecutor::new();
            let ex = executor.clone();
            handle = ex
                .spawn_local(async {
                    // Would loop forever if driven.
                    loop {
                        core::future::pending::<()>().await;
                    }
                })
                .unwrap();
            // `executor` and `ex` drop at the end of this block, dropping the
            // registry and with it the pending task future.
        }

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut handle = core::pin::pin!(handle);
        match handle.as_mut().poll(&mut cx) {
            Poll::Ready(Err(Cancelled)) => {}
            Poll::Ready(Ok(())) => panic!("task cannot complete after executor drop"),
            Poll::Pending => panic!("handle must resolve after executor drop"),
        }
    }

    fn noop_waker() -> Waker {
        use core::task::{RawWaker, RawWakerVTable};
        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|ptr| RawWaker::new(ptr, &VTABLE), |_| {}, |_| {}, |_| {});
        // SAFETY: the vtable ignores the data pointer.
        unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
    }
}
