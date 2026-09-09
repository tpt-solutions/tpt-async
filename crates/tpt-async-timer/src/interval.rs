//! [`Interval`] — a stream-like future that fires repeatedly at a fixed period.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::sleep::Sleep;
use crate::wheel::TimerWheel;

/// Yields `()` repeatedly, every `period` ticks of the associated wheel.
///
/// # Usage
/// Poll `Interval` inside an async loop.  Each `await` suspends until one
/// period elapses, then returns `()`.
///
/// ```rust,ignore
/// let mut iv = Interval::new(&mut wheel, period);
/// loop {
///     iv.tick().await;
///     // do periodic work
/// }
/// ```
pub struct Interval<'wheel, const SLOTS: usize, const LEVELS: usize> {
    sleep: Pin<&'wheel mut Sleep<'wheel, SLOTS, LEVELS>>,
    // We store the raw wheel pointer so we can reset the deadline without
    // needing to re-borrow through Sleep's private field.
    wheel: *mut TimerWheel<SLOTS, LEVELS>,
    period: u64,
}

// SAFETY: `Interval` is safe to send if `TimerWheel` is (raw ptr is non-owning).
unsafe impl<'wheel, const SLOTS: usize, const LEVELS: usize> Send
    for Interval<'wheel, SLOTS, LEVELS>
{
}

impl<'wheel, const SLOTS: usize, const LEVELS: usize>
    Interval<'wheel, SLOTS, LEVELS>
{
    /// Create an interval that fires every `period` ticks, with the first tick
    /// firing `period` ticks from the current wheel time.
    ///
    /// `sleep` must be a freshly-created, unpinned [`Sleep`] whose deadline is
    /// already set to `wheel.now() + period`.
    pub fn new(
        sleep: Pin<&'wheel mut Sleep<'wheel, SLOTS, LEVELS>>,
        wheel: &'wheel mut TimerWheel<SLOTS, LEVELS>,
        period: u64,
    ) -> Self {
        Self {
            sleep,
            wheel: wheel as *mut _,
            period,
        }
    }

    /// Wait for the next tick.
    ///
    /// Returns `Poll::Ready(())` every `period` ticks, then resets the
    /// internal deadline for the following period.
    pub fn poll_tick(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<()> {
        // SAFETY: we project to a pinned field; Interval is !Unpin because
        // Sleep is !Unpin.
        let this = unsafe { self.get_unchecked_mut() };

        // SAFETY: `sleep` is a `Pin<&mut Sleep>` so re-pinning is safe.
        let sleep_pin: Pin<&mut Sleep<'wheel, SLOTS, LEVELS>> =
            unsafe { Pin::new_unchecked(&mut *this.sleep) };

        match sleep_pin.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(()) => {
                // Reset the deadline for the next period.
                // SAFETY: wheel pointer is valid for 'wheel.
                let wheel = unsafe { &*this.wheel };
                let next_deadline = wheel.now() + this.period;

                // SAFETY: we project into a pinned field and update deadline only
                // (does not move the entry).
                let inner =
                    unsafe { this.sleep.as_mut().get_unchecked_mut() };
                inner.entry.deadline = next_deadline;
                inner.registered = false; // will re-register on next poll

                Poll::Ready(())
            }
        }
    }
}

/// Convenience wrapper so `interval.tick().await` works.
pub struct Tick<'a, 'wheel, const SLOTS: usize, const LEVELS: usize> {
    interval: Pin<&'a mut Interval<'wheel, SLOTS, LEVELS>>,
}

impl<'a, 'wheel, const SLOTS: usize, const LEVELS: usize> Future
    for Tick<'a, 'wheel, SLOTS, LEVELS>
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY: Tick is structurally pinned through its `interval` field.
        let this = unsafe { self.get_unchecked_mut() };
        this.interval.as_mut().poll_tick(cx)
    }
}

impl<'wheel, const SLOTS: usize, const LEVELS: usize>
    Interval<'wheel, SLOTS, LEVELS>
{
    /// Returns a future that resolves on the next interval tick.
    pub fn tick(&mut self) -> Tick<'_, 'wheel, SLOTS, LEVELS> {
        Tick {
            // SAFETY: we wrap `self` in a Pin; callers must already have this
            // Interval pinned (it is !Unpin due to Sleep being !Unpin).
            interval: unsafe { Pin::new_unchecked(self) },
        }
    }
}
