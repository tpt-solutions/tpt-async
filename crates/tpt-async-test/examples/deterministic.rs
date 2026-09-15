// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic timing tests: time only moves when the driver advances, so
//! timeout paths are exercised without wall-clock sleeps.

use std::future::Future;
use std::pin::pin;
use std::pin::Pin;
use std::task::{Context, Poll};
use tpt_async_io::{AsyncRead as _, AsyncWrite as _};

use tpt_async_core::waker::noop_waker;
use tpt_async_test::{io_pair, TestDriver};
use tpt_async_timer::prelude::*;

fn main() {
    let driver: TestDriver = TestDriver::new();
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    // A 10-tick sleep fires exactly at the deadline:
    let mut sleep = pin!(Sleep::new(driver.wheel(), driver.now() + 10));
    assert!(sleep.as_mut().poll(&mut cx).is_pending());

    driver.advance(5);
    assert!(sleep.as_mut().poll(&mut cx).is_pending(), "too early");

    driver.advance(5);
    assert!(sleep.as_mut().poll(&mut cx).is_ready(), "fired on time");

    // Fake I/O pair: write on one end, read on the other.
    let (mut a, mut b) = io_pair();
    let _ = std::future::poll_fn(|cx| Pin::new(&mut a).poll_write(cx, b"payload")).await_spin();
    std::future::poll_fn(|cx| {
        Pin::new(&mut b).poll_read(cx, &mut tpt_async_io::ReadBuf::new(&mut [0u8; 16]))
    })
    .await_spin()
    .expect("read");
    println!("test example: deterministic advance + fake I/O OK");
}

trait AwaitSpin: Future {
    fn await_spin(self) -> Self::Output
    where
        Self: Sized,
    {
        fn noop() -> std::task::Waker {
            use std::task::{RawWaker, RawWakerVTable};
            static VTABLE: RawWakerVTable =
                RawWakerVTable::new(|p| RawWaker::new(p, &VTABLE), |_| {}, |_| {}, |_| {});
            // SAFETY: the vtable ignores the data pointer.
            unsafe { std::task::Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
        }
        let mut fut = pin!(self);
        let waker = noop();
        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
            std::hint::spin_loop();
        }
    }
}

impl<F: Future> AwaitSpin for F {}
