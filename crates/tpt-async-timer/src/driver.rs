//! The std timer driver: a background thread that ticks a shared global
//! wheel, so [`sleep`], [`timeout`] and [`interval`] just work — no executor
//! or manual wheel driving required.
//!
//! The driver is started lazily on the first use and exits with the process.
//! Tick resolution is 1 ms.  On `no_std` targets this module does not exist;
//! drive a `TimerWheel` yourself — read `now_ticks()` each loop iteration
//! and call `advance_to` from your tick source.
//!
//! # Examples
//!
//! ```rust,no_run
//! use tpt_async_timer::driver::{sleep, timeout};
//! use core::time::Duration;
//!
//! # async fn demo() -> Result<(), tpt_async_timer::timeout::TimedOut> {
//! sleep(Duration::from_millis(50)).await;
//! let result = timeout(Duration::from_millis(500), async {
//!     // ... some operation ...
//!     42
//! }).await?;
//! # let _ = result;
//! # Ok(())
//! # }
//! ```

use core::future::Future;
use core::time::Duration;

use std::sync::{Condvar, Mutex, OnceLock};

use crate::clock::{Clock as _, StdClock};
use crate::interval::Interval;
use crate::timeout::Timeout;
use crate::wheel::TimerWheel;

/// The shared global wheel backing [`sleep`]/[`timeout`]/[`interval`].
///
/// 256 slots × 4 levels at 1 ms resolution cover ~49 days without wrapping.
pub type GlobalWheel = TimerWheel<256, 4>;

/// The future returned by [`sleep`].
pub type SleepFuture = crate::sleep::Sleep<'static, 256, 4>;
/// The future returned by [`timeout`].
pub type TimeoutFuture<F> = Timeout<'static, F, 256, 4>;
/// The future returned by [`interval`].
pub type IntervalFuture = Interval<'static, 256, 4>;

static WHEEL: GlobalWheel = GlobalWheel::new();
static DRIVER: OnceLock<()> = OnceLock::new();
/// Bumped on every registration so the driver's wait can be interrupted.
static REGISTRATIONS: (Mutex<u64>, Condvar) = (Mutex::new(0), Condvar::new());

/// Milliseconds per wheel tick for the global wheel.
pub const TICK_MS: u64 = 1;

/// Convert a duration to whole wheel ticks (truncating; sub-ms durations
/// become 0 ticks, i.e. "fire on the next poll").
fn ticks_for(dur: Duration) -> u64 {
    (dur.as_millis() as u64).saturating_div(TICK_MS)
}

/// Called by timer futures after their first registration, so the driver
/// wakes up and notices the (possibly earlier) new deadline.  Cheap: a
/// counter bump + condvar notify.
pub(crate) fn on_registration() {
    if DRIVER.get().is_some() {
        let (lock, cv) = &REGISTRATIONS;
        let mut count = lock.lock().expect("timer driver mutex poisoned");
        *count += 1;
        cv.notify_all();
    }
}

fn ensure_driver() {
    DRIVER.get_or_init(|| {
        std::thread::Builder::new()
            .name("tpt-async-timer".into())
            .spawn(driver_loop)
            .expect("failed to spawn tpt-async-timer driver thread");
    });
}

fn driver_loop() {
    let clock = StdClock::new();
    loop {
        // Catch the wheel up to real time; this wakes every due timer.
        WHEEL.advance_to(clock.now_ticks());

        // Sleep until the next deadline (capped so we re-check the clock
        // regularly) or until a new registration changes the picture.
        let cap_ms = WHEEL
            .next_deadline()
            .map(|d| d.saturating_sub(WHEEL.now()))
            .unwrap_or(100)
            .min(100);

        let (lock, cv) = &REGISTRATIONS;
        let count = lock.lock().expect("timer driver mutex poisoned");
        let _ = cv.wait_timeout(count, Duration::from_millis(cap_ms));
    }
}

/// Wait for at least `dur` (1 ms resolution, truncating).
///
/// Backed by the global driver thread; the first call starts it.
///
/// # Examples
///
/// ```rust,no_run
/// use tpt_async_timer::driver::sleep;
/// use core::time::Duration;
///
/// # async fn demo() {
/// sleep(Duration::from_millis(10)).await;
/// # }
/// ```
pub fn sleep(dur: Duration) -> SleepFuture {
    ensure_driver();
    let deadline = WHEEL.now() + ticks_for(dur);
    SleepFuture::new(&WHEEL, deadline)
}

/// Run `fut` with a deadline, returning `Err(TimedOut)` if it does not
/// complete within `dur`.
///
/// Backed by the global driver thread; the first call starts it.
///
/// # Examples
///
/// ```rust,no_run
/// use tpt_async_timer::driver::timeout;
/// use core::time::Duration;
///
/// # async fn demo() -> Result<(), tpt_async_timer::timeout::TimedOut> {
/// let answer = timeout(Duration::from_millis(100), async { 7 }).await?;
/// # let _ = answer;
/// # Ok(())
/// # }
/// ```
pub fn timeout<F: Future>(dur: Duration, fut: F) -> TimeoutFuture<F> {
    ensure_driver();
    let deadline = WHEEL.now() + ticks_for(dur);
    TimeoutFuture::new(&WHEEL, fut, deadline)
}

/// Yield `()` repeatedly every `dur` (1 ms resolution, truncating).
///
/// Backed by the global driver thread; the first call starts it.
///
/// # Examples
///
/// ```rust,no_run
/// use tpt_async_timer::driver::interval;
/// use core::time::Duration;
///
/// # async fn demo() {
/// let mut ticks = core::pin::pin!(interval(Duration::from_millis(250)));
/// loop {
///     ticks.as_mut().tick().await;
///     // periodic work
/// #   break;
/// }
/// # }
/// ```
pub fn interval(dur: Duration) -> IntervalFuture {
    ensure_driver();
    IntervalFuture::new(&WHEEL, ticks_for(dur).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_resolves_after_duration() {
        // The driver thread ticks the global wheel; a minimal poll loop drives
        // the future to completion.
        let start = std::time::Instant::now();
        let mut fut = core::pin::pin!(sleep(Duration::from_millis(15)));
        let waker = test_waker();
        let mut cx = core::task::Context::from_waker(&waker);
        loop {
            match fut.as_mut().poll(&mut cx) {
                core::task::Poll::Ready(()) => break,
                core::task::Poll::Pending => std::thread::sleep(Duration::from_millis(1)),
            }
        }
        assert!(start.elapsed() >= Duration::from_millis(10));
    }

    #[test]
    fn timeout_resolves_value() {
        let mut fut = core::pin::pin!(timeout(
            Duration::from_millis(500),
            std::future::ready(7u32),
        ));
        let waker = test_waker();
        let mut cx = core::task::Context::from_waker(&waker);
        match fut.as_mut().poll(&mut cx) {
            core::task::Poll::Ready(Ok(v)) => assert_eq!(v, 7),
            _ => panic!("ready future must not time out"),
        }
    }

    #[test]
    fn timeout_fires_on_slow_future() {
        struct NeverPending;
        impl Future for NeverPending {
            type Output = ();
            fn poll(
                self: core::pin::Pin<&mut Self>,
                _cx: &mut core::task::Context<'_>,
            ) -> core::task::Poll<()> {
                core::task::Poll::Pending
            }
        }

        let fut = timeout(Duration::from_millis(5), NeverPending);
        let start = std::time::Instant::now();
        let mut fut = core::pin::pin!(fut);
        let waker = test_waker();
        let mut cx = core::task::Context::from_waker(&waker);
        loop {
            match fut.as_mut().poll(&mut cx) {
                core::task::Poll::Ready(Err(_)) => break,
                core::task::Poll::Ready(Ok(())) => panic!("never future resolved"),
                core::task::Poll::Pending => {
                    assert!(
                        start.elapsed() < Duration::from_secs(5),
                        "timeout never fired"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        assert!(start.elapsed() >= Duration::from_millis(3));
    }

    fn test_waker() -> core::task::Waker {
        use core::task::{RawWaker, RawWakerVTable};
        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|ptr| RawWaker::new(ptr, &VTABLE), |_| {}, |_| {}, |_| {});
        // SAFETY: the vtable ignores the data pointer.
        unsafe { core::task::Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
    }
}
