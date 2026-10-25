// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Timer-wheel checks that run inside a real browser engine.
//!
//! Run with `wasm-pack test --headless --chrome` (or `--firefox`). The wheel is
//! advanced by hand, so the tests do not depend on the page clock.

use core::future::{pending, ready, Future};
use core::pin::pin;
use core::task::{Context, Poll, Waker};

use tpt_async_timer::interval::Interval;
use tpt_async_timer::sleep::Sleep;
use tpt_async_timer::timeout::{TimedOut, Timeout};
use tpt_async_timer::wheel::TimerWheel;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn poll_once<F: Future>(fut: core::pin::Pin<&mut F>) -> Poll<F::Output> {
    fut.poll(&mut Context::from_waker(Waker::noop()))
}

#[wasm_bindgen_test]
fn sleep_completes_at_its_deadline() {
    let wheel = TimerWheel::<64, 4>::new();
    let mut sleep = pin!(Sleep::new(&wheel, 25));

    assert!(poll_once(sleep.as_mut()).is_pending());
    wheel.advance_to(24);
    assert!(poll_once(sleep.as_mut()).is_pending());
    wheel.advance_to(25);
    assert!(poll_once(sleep.as_mut()).is_ready());
    assert_eq!(wheel.len(), 0);
}

#[wasm_bindgen_test]
fn timeout_expires_when_the_inner_future_stalls() {
    let wheel = TimerWheel::<64, 4>::new();
    let mut timed = pin!(Timeout::new(&wheel, pending::<()>(), 100));

    assert!(poll_once(timed.as_mut()).is_pending());
    wheel.advance_to(100);
    assert_eq!(poll_once(timed.as_mut()), Poll::Ready(Err(TimedOut)));
}

#[wasm_bindgen_test]
fn timeout_yields_the_value_of_a_ready_future() {
    let wheel = TimerWheel::<64, 4>::new();
    let mut timed = pin!(Timeout::new(&wheel, ready(42u32), 1_000));
    assert_eq!(poll_once(timed.as_mut()), Poll::Ready(Ok(42)));
}

#[wasm_bindgen_test]
fn interval_ticks_once_per_period() {
    let wheel = TimerWheel::<64, 4>::new();
    let mut interval = pin!(Interval::new(&wheel, 10));
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

#[wasm_bindgen_test]
fn many_sleeps_across_levels_all_fire() {
    // Deadlines spread across the wheel levels (64 slots per level), so
    // entries cascade down before they expire.
    let wheel = TimerWheel::<64, 4>::new();
    let deadlines: [u64; 6] = [3, 70, 200, 5_000, 40_000, 300_000];
    let mut sleeps = [
        pin!(Sleep::new(&wheel, deadlines[0])),
        pin!(Sleep::new(&wheel, deadlines[1])),
        pin!(Sleep::new(&wheel, deadlines[2])),
        pin!(Sleep::new(&wheel, deadlines[3])),
        pin!(Sleep::new(&wheel, deadlines[4])),
        pin!(Sleep::new(&wheel, deadlines[5])),
    ];
    for s in sleeps.iter_mut() {
        assert!(poll_once(s.as_mut()).is_pending());
    }
    assert_eq!(wheel.len(), 6);

    let mut now = 0;
    for (i, &deadline) in deadlines.iter().enumerate() {
        wheel.advance_to(deadline);
        now = deadline;
        assert!(
            poll_once(sleeps[i].as_mut()).is_ready(),
            "sleep {i} at tick {now}"
        );
    }
    assert_eq!(wheel.len(), 0);
    assert_eq!(now, 300_000);
}
