//! [`Timeout`] — wraps a future with a deadline.
//!
//! Only available when the `alloc` feature is enabled.

#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::sleep::Sleep;
use crate::wheel::TimerWheel;

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
/// Only available with the **`alloc`** feature (the inner future is
/// heap-boxed so it can be pinned without requiring the caller to manage
/// stack pinning for two futures simultaneously).
#[cfg(feature = "alloc")]
pub struct Timeout<'wheel, F, const SLOTS: usize, const LEVELS: usize> {
    /// The wrapped future, heap-allocated so it can be structurally pinned.
    future: Pin<Box<F>>,
    /// The sleep future that fires at the deadline.
    sleep: Sleep<'wheel, SLOTS, LEVELS>,
}

#[cfg(feature = "alloc")]
impl<'wheel, F, const SLOTS: usize, const LEVELS: usize>
    Timeout<'wheel, F, SLOTS, LEVELS>
where
    F: Future,
{
    /// Wrap `future` with a `deadline` tick deadline.
    pub fn new(
        wheel: &'wheel mut TimerWheel<SLOTS, LEVELS>,
        future: F,
        deadline: u64,
    ) -> Self {
        // SAFETY: `Sleep::new` takes a `&'wheel mut TimerWheel` and returns a
        // `Sleep` whose lifetime is tied to `'wheel`.  We split the borrow via
        // a raw pointer so we can also construct `Sleep` after moving `wheel`.
        let wheel_ptr = wheel as *mut TimerWheel<SLOTS, LEVELS>;
        // SAFETY: wheel_ptr is valid for 'wheel.
        let sleep = Sleep::new(unsafe { &mut *wheel_ptr }, deadline);
        Self {
            future: Box::pin(future),
            sleep,
        }
    }
}

#[cfg(feature = "alloc")]
impl<'wheel, F, const SLOTS: usize, const LEVELS: usize> Future
    for Timeout<'wheel, F, SLOTS, LEVELS>
where
    F: Future,
{
    type Output = Result<F::Output, TimedOut>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: we project into pinned fields; Timeout is !Unpin because
        // Sleep is !Unpin.
        let this = unsafe { self.get_unchecked_mut() };

        // Poll the inner future first — if it's ready we don't need the timer.
        match this.future.as_mut().poll(cx) {
            Poll::Ready(v) => return Poll::Ready(Ok(v)),
            Poll::Pending => {}
        }

        // Poll the sleep future.
        // SAFETY: `sleep` is !Unpin and we pin it here.
        let sleep_pin: Pin<&mut Sleep<'wheel, SLOTS, LEVELS>> =
            unsafe { Pin::new_unchecked(&mut this.sleep) };
        match sleep_pin.poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(TimedOut)),
            Poll::Pending => Poll::Pending,
        }
    }
}
