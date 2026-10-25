// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Runtime checks of the allocation-free API, using only `core` types.
//!
//! Nothing here touches the `std` driver thread or a system clock: the wheel is
//! advanced by hand, as a bare-metal tick interrupt would. The file uses no
//! `std`-only items, so `cargo test -p tpt-async-timer --no-default-features`
//! runs the same checks in the configuration embedded targets build.

use core::future::{pending, ready, Future};
use core::pin::pin;
use core::task::{Context, Poll, Waker};

use tpt_async_timer::interval::Interval;
use tpt_async_timer::sleep::Sleep;
use tpt_async_timer::timeout::{TimedOut, Timeout};
use tpt_async_timer::wheel::TimerWheel;

/// Bare-metal pattern: the wheel lives in a `static` and every method takes `&self`.
static WHEEL: TimerWheel<16, 3> = TimerWheel::new();

fn poll_once<F: Future>(fut: core::pin::Pin<&mut F>) -> Poll<F::Output> {
    fut.poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn sleep_completes_only_once_the_wheel_reaches_its_deadline() {
    let wheel = TimerWheel::<16, 3>::new();
    let mut sleep = pin!(Sleep::new(&wheel, 3));

    assert!(poll_once(sleep.as_mut()).is_pending());
    assert_eq!(wheel.len(), 1, "a pending sleep is registered");

    wheel.advance_to(2);
    assert!(poll_once(sleep.as_mut()).is_pending());

    wheel.advance_to(3);
    assert!(poll_once(sleep.as_mut()).is_ready());
    assert_eq!(wheel.len(), 0, "a completed sleep is deregistered");
}

#[test]
fn sleep_at_or_before_now_is_ready_on_first_poll() {
    let wheel = TimerWheel::<16, 3>::new();
    wheel.advance_to(10);
    let mut sleep = pin!(Sleep::new(&wheel, 10));
    assert!(poll_once(sleep.as_mut()).is_ready());
    assert_eq!(wheel.len(), 0);
}

#[test]
fn static_wheel_drives_a_sleep() {
    // Each test owns its own wheel except this one, which uses the static.
    let mut sleep = pin!(Sleep::new(&WHEEL, WHEEL.now() + 2));
    assert!(poll_once(sleep.as_mut()).is_pending());
    WHEEL.tick_n(2);
    assert!(poll_once(sleep.as_mut()).is_ready());
}

#[test]
fn timeout_fires_when_the_inner_future_is_stuck() {
    let wheel = TimerWheel::<16, 3>::new();
    let mut timed = pin!(Timeout::new(&wheel, pending::<()>(), 5));

    assert!(poll_once(timed.as_mut()).is_pending());
    wheel.advance_to(4);
    assert!(poll_once(timed.as_mut()).is_pending());
    wheel.advance_to(5);
    assert_eq!(poll_once(timed.as_mut()), Poll::Ready(Err(TimedOut)));
    assert_eq!(wheel.len(), 0, "an expired timeout is deregistered");
}

#[test]
fn timeout_returns_the_value_when_the_inner_future_finishes_first() {
    let wheel = TimerWheel::<16, 3>::new();
    let mut timed = pin!(Timeout::new(&wheel, ready(7u32), 100));
    assert_eq!(poll_once(timed.as_mut()), Poll::Ready(Ok(7)));
    assert_eq!(
        wheel.len(),
        0,
        "a finished inner future leaves no timer behind"
    );
}

#[test]
fn interval_ticks_once_per_period() {
    let wheel = TimerWheel::<16, 3>::new();
    let mut interval = pin!(Interval::new(&wheel, 10));
    let first = interval.next_deadline();
    assert_eq!(first, 10);

    assert!(interval
        .as_mut()
        .poll_tick(&mut Context::from_waker(Waker::noop()))
        .is_pending());

    wheel.advance_to(10);
    assert!(interval
        .as_mut()
        .poll_tick(&mut Context::from_waker(Waker::noop()))
        .is_ready());
    assert_eq!(interval.next_deadline(), 20);
}
