// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Executor benchmarks: spawn + join round-trips and wake scheduling.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use criterion::{criterion_group, criterion_main, Criterion};
use tpt_async_core::spawn::LocalSpawn as _;
use tpt_async_executor::LocalExecutor;

fn bench_block_on_immediate(c: &mut Criterion) {
    let executor = LocalExecutor::new();
    c.bench_function("executor/block_on_immediate", |b| {
        b.iter(|| executor.block_on(async { 42u32 }))
    });
}

struct YieldOnce(bool);

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

fn bench_spawn_join_100(c: &mut Criterion) {
    let executor = LocalExecutor::new();
    c.bench_function("executor/spawn_join_100", |b| {
        b.iter(|| {
            let ex = executor.clone();
            executor.block_on(async {
                for _ in 0..100 {
                    let handle = ex.spawn_local(YieldOnce(false)).unwrap();
                    handle.await.unwrap();
                }
            })
        })
    });
}

criterion_group!(benches, bench_block_on_immediate, bench_spawn_join_100);
criterion_main!(benches);
