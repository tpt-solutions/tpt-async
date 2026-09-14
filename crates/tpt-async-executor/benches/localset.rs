// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Benchmark: `LocalExecutor` vs `tokio::task::LocalSet` — the roadmap's
//! apples-to-apples spawn/join comparison on the same workload.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use criterion::{criterion_group, criterion_main, Criterion};
use tpt_async_core::spawn::LocalSpawn as _;
use tpt_async_executor::LocalExecutor;

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

fn bench_local_executor(c: &mut Criterion) {
    let executor = LocalExecutor::new();
    c.bench_function("spawn_join_100/tpt_local_executor", |b| {
        b.iter(|| {
            let ex = executor.clone();
            executor.block_on(async {
                for _ in 0..100 {
                    let h = ex.spawn_local(YieldOnce(false)).unwrap();
                    h.await.unwrap();
                }
            })
        })
    });
}

fn bench_tokio_localset(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("tokio runtime");
    c.bench_function("spawn_join_100/tokio_localset", |b| {
        b.iter(|| {
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async {
                for _ in 0..100 {
                    let h = tokio::task::spawn_local(YieldOnce(false));
                    h.await.unwrap();
                }
            })
        })
    });
}

criterion_group!(benches, bench_local_executor, bench_tokio_localset);
criterion_main!(benches);
