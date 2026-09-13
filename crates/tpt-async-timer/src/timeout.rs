//! [`Timeout`] — wraps a future with a deadline.

use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::ptr::NonNull;
use core::task::{Context, Poll};

use crate::wheel::{TimerEntry, TimerWheel};

/// Returned by [`Timeout`] when the deadline elapses before the inner future
/// completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedOut;

impl core::fmt::Display for TimedOut {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("operation timed out")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TimedOut {}

/// Wraps a future `F` with a deadline expressed in wheel ticks.
///
/// Returns `Ok(F::Output)` if `F` completes before the deadline, or
/// `Err(TimedOut)` if the wheel advances past the deadline first.
///
/// Like [`Sleep`](crate::sleep::Sleep), the wheel is borrowed shared, so a
/// driver thread may keep ticking it while the wrapped future runs.  The
/// wrapped future is stored inline (no heap allocation); `Timeout` is `!Unpin`
/// and must be pinned before polling.
///
/// # Drop behaviour
/// Dropping a pending `Timeout` (or one whose future already finished)
/// deregisters its timer entry.
pub struct Timeout<'a, F, const SLOTS: usize, const LEVELS: usize> {
    /// The (shared) wheel.
    wheel: &'a TimerWheel<SLOTS, LEVELS>,
    /// The wrapped future, pinned structurally once `Timeout` is pinned.
    future: F,
    /// Intrusive timer node for the deadline.
    entry: TimerEntry,
    /// Whether `entry` is currently registered.
    registered: bool,
    /// The future must not move while the entry may be registered.
    _pin: PhantomPinned,
}

impl<'a, F, const SLOTS: usize, const LEVELS: usize> Timeout<'a, F, SLOTS, LEVELS> {
    /// Wrap `future` with a `deadline` tick deadline.
    pub fn new(wheel: &'a TimerWheel<SLOTS, LEVELS>, future: F, deadline: u64) -> Self {
        Self {
            wheel,
            future,
            entry: TimerEntry::new(deadline),
            registered: false,
            _pin: PhantomPinned,
        }
    }

    /// The absolute tick at which the wrapped future times out.
    #[must_use]
    pub fn deadline(&self) -> u64 {
        self.entry.deadline()
    }
}

impl<F, const SLOTS: usize, const LEVELS: usize> Future for Timeout<'_, F, SLOTS, LEVELS>
where
    F: Future,
{
    type Output = Result<F::Output, TimedOut>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: structural pin projection — `Timeout` is `!Unpin`, so the
        // address of `self` (and every field) is stable while pinned; we never
        // move fields out.
        let this = unsafe { self.get_unchecked_mut() };

        // Poll the inner future first — if it's ready we don't need the timer.
        // SAFETY: `future` is structurally pinned (see above).
        let future_pin: Pin<&mut F> = unsafe { Pin::new_unchecked(&mut this.future) };
        if let Poll::Ready(v) = future_pin.poll(cx) {
            // Deregister the timer eagerly so the driver stops tracking us.
            let ptr = NonNull::from(&mut this.entry);
            this.wheel.inner.lock().remove(ptr); // no-op if never registered
            this.registered = false;
            return Poll::Ready(Ok(v));
        }

        let mut wheel = this.wheel.inner.lock();

        // Deadline reached (or passed): the inner future loses the race.
        if wheel.now() >= this.entry.deadline() {
            let ptr = NonNull::from(&mut this.entry);
            wheel.remove(ptr); // no-op if never registered
            this.registered = false;
            return Poll::Ready(Err(TimedOut));
        }

        let waker = cx.waker();
        if this
            .entry
            .waker
            .as_ref()
            .map_or(true, |w| !w.will_wake(waker))
        {
            this.entry.waker = Some(waker.clone());
        }

        if !this.registered {
            let ptr = NonNull::from(&mut this.entry);
            wheel.insert(ptr);
            this.registered = true;
            drop(wheel);
            #[cfg(feature = "std")]
            crate::driver::on_registration();
        }

        Poll::Pending
    }
}

impl<F, const SLOTS: usize, const LEVELS: usize> Drop for Timeout<'_, F, SLOTS, LEVELS> {
    fn drop(&mut self) {
        let ptr = NonNull::from(&mut self.entry);
        // In drop the entry cannot be mid-move; no-op if already deregistered.
        self.wheel.inner.lock().remove(ptr);
        self.registered = false;
    }
}

impl<F: core::fmt::Debug, const SLOTS: usize, const LEVELS: usize> core::fmt::Debug
    for Timeout<'_, F, SLOTS, LEVELS>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Timeout")
            .field("future", &self.future)
            .field("deadline", &self.entry.deadline())
            .field("registered", &self.registered)
            .finish()
    }
}
