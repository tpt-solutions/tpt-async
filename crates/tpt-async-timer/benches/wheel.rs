// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Timer wheel benchmarks: empty ticks, bulk registration, and bulk firing.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use tpt_async_timer::sleep::Sleep;
use tpt_async_timer::wheel::TimerWheel;

type Wheel = TimerWheel<64, 4>;
type DefaultSleep<'a> = Sleep<'a, 64, 4>;

fn noop_waker() -> Waker {
    static VTABLE: RawWakerVTable =
        RawWakerVTable::new(|ptr| RawWaker::new(ptr, &VTABLE), |_| {}, |_| {}, |_| {});
    // SAFETY: the vtable ignores the data pointer.
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
}

fn bench_tick_empty(c: &mut Criterion) {
    let wheel = Wheel::new();
    c.bench_function("wheel/tick_empty", |b| b.iter(|| wheel.tick()));
}

fn bench_register_1000_sleeps(c: &mut Criterion) {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    c.bench_function("wheel/register_1000_sleeps", |b| {
        b.iter_batched(
            Wheel::new,
            |wheel| {
                for i in 1..=1000u64 {
                    let mut sleep = Box::pin(Sleep::new(&wheel, i));
                    let _: Poll<()> = sleep.as_mut().poll(&mut cx);
                }
                // Fire and deregister everything.
                wheel.advance_to(1001);
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_rearm(c: &mut Criterion) {
    let wheel = Wheel::new();
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    let mut sleep: Pin<Box<DefaultSleep<'_>>> = Box::pin(Sleep::new(&wheel, 1));
    let _ = sleep.as_mut().poll(&mut cx);

    c.bench_function("wheel/rearm_and_poll", |b| {
        b.iter(|| {
            sleep.as_mut().reset_pinned(wheel.now() + 10);
            let _: Poll<()> = sleep.as_mut().poll(&mut cx);
        })
    });
}

criterion_group!(
    benches,
    bench_tick_empty,
    bench_register_1000_sleeps,
    bench_rearm
);
criterion_main!(benches);
