// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`retry`] — a retry-with-exponential-backoff combinator.
//!
//! Built on the std driver's `sleep`, it showcases the timer crate's
//! "just works" story: resilient operations in three lines.
//!
//! ```rust,no_run
//! use tpt_async_timer::retry::{retry, RetryPolicy};
//! use core::time::Duration;
//!
//! # async fn fetch() -> Result<String, std::io::Error> { Ok(String::new()) }
//! # async fn demo() -> Result<String, std::io::Error> {
//! let body = retry(&RetryPolicy::default(), fetch).await?;
//! # Ok(body)
//! # }
//! ```

use core::future::Future;
use core::time::Duration;

use crate::driver::sleep;

/// How often and how long to wait between attempts.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Total attempts including the first (default 3).
    pub max_attempts: u32,
    /// Delay before the second attempt (default 10 ms).
    pub initial_delay: Duration,
    /// Upper bound for any single delay (default 1 s).
    pub max_delay: Duration,
    /// Growth factor per attempt (default ×2).
    pub multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// A policy with `max_attempts` attempts and the default backoff curve.
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            ..Self::default()
        }
    }

    /// Override the initial delay.
    #[must_use]
    pub fn initial_delay(mut self, d: Duration) -> Self {
        self.initial_delay = d;
        self
    }

    /// Override the maximum single delay.
    #[must_use]
    pub fn max_delay(mut self, d: Duration) -> Self {
        self.max_delay = d;
        self
    }

    /// Override the growth factor.
    #[must_use]
    pub fn multiplier(mut self, m: f64) -> Self {
        self.multiplier = m;
        self
    }

    /// Delay before attempt number `attempt` (1-based; the delay *after*
    /// attempt `attempt`, i.e. before `attempt + 1`).
    fn delay_for(&self, attempt: u32) -> Duration {
        let factor = self.multiplier.powi(attempt.saturating_sub(1) as i32);
        let d = self.initial_delay.as_secs_f64() * factor;
        let d = d.min(self.max_delay.as_secs_f64());
        Duration::from_secs_f64(d.max(0.0))
    }
}

/// Run `op`, retrying on `Err` per `policy`.
///
/// `op` is called at most `policy.max_attempts` times; after each failure
/// the combinator sleeps for the policy's exponentially growing delay, then
/// tries again.  Returns the first `Ok`, or the *last* `Err`.
pub async fn retry<T, E, Fut, Op>(policy: &RetryPolicy, mut op: Op) -> Result<T, E>
where
    Fut: Future<Output = Result<T, E>>,
    Op: FnMut() -> Fut,
{
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                if attempt >= policy.max_attempts {
                    return Err(err);
                }
                sleep(policy.delay_for(attempt)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn succeeds_without_retry_on_first_ok() {
        static CALLS: AtomicU32 = AtomicU32::new(0);
        let result = spin(retry(&RetryPolicy::default(), || {
            CALLS.fetch_add(1, Ordering::SeqCst);
            async { Ok::<u8, ()>(7) }
        }));
        assert_eq!(result.unwrap(), 7);
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn retries_until_success() {
        static CALLS: AtomicU32 = AtomicU32::new(0);
        let policy = RetryPolicy::new(5)
            .initial_delay(Duration::from_millis(1))
            .max_delay(Duration::from_millis(4));

        let result = spin(retry(&policy, || {
            let n = CALLS.fetch_add(1, Ordering::SeqCst);
            async move {
                if n < 2 {
                    Err("try again")
                } else {
                    Ok(n)
                }
            }
        }));
        assert_eq!(result.unwrap(), 2, "third attempt succeeds");
        assert_eq!(CALLS.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn returns_last_error_after_exhausting_attempts() {
        static CALLS: AtomicU32 = AtomicU32::new(0);
        let policy = RetryPolicy::new(3).initial_delay(Duration::from_millis(1));

        let result: Result<(), &str> = spin(retry(&policy, || {
            CALLS.fetch_add(1, Ordering::SeqCst);
            async { Err("always fails") }
        }));
        assert_eq!(result.unwrap_err(), "always fails");
        assert_eq!(
            CALLS.load(Ordering::SeqCst),
            3,
            "exactly max_attempts calls"
        );
    }

    #[test]
    fn delay_curve_is_exponential_and_capped() {
        let policy = RetryPolicy::new(10)
            .initial_delay(Duration::from_millis(10))
            .max_delay(Duration::from_millis(50));
        assert_eq!(policy.delay_for(1), Duration::from_millis(10));
        assert_eq!(policy.delay_for(2), Duration::from_millis(20));
        assert_eq!(policy.delay_for(3), Duration::from_millis(40));
        assert_eq!(policy.delay_for(4), Duration::from_millis(50), "capped");
        assert_eq!(policy.delay_for(9), Duration::from_millis(50), "capped");
    }

    fn spin<F: Future>(future: F) -> F::Output {
        fn noop() -> std::task::Waker {
            use std::task::{RawWaker, RawWakerVTable};
            static VTABLE: RawWakerVTable =
                RawWakerVTable::new(|p| RawWaker::new(p, &VTABLE), |_| {}, |_| {}, |_| {});
            // SAFETY: the vtable ignores the data pointer.
            unsafe { std::task::Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
        }
        let mut fut = std::pin::pin!(future);
        let waker = noop();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::thread::sleep(Duration::from_millis(1)),
            }
        }
    }

    use std::task::Poll;
}
