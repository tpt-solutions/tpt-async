// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `ReadBuf` benchmarks: the zero-copy cursor the parsers build on.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use tpt_async_io::ReadBuf;

fn bench_advance_chunks(c: &mut Criterion) {
    let mut storage = vec![0u8; 64 * 1024];

    let mut group = c.benchmark_group("read_buf/advance_4k_chunks");
    group.throughput(Throughput::Bytes(storage.len() as u64));
    group.bench_function("fill_64k", |b| {
        b.iter(|| {
            let mut buf = ReadBuf::new(&mut storage);
            while buf.remaining() > 0 {
                let chunk = buf.unfilled();
                let n = chunk.len().min(4096);
                buf.advance(n);
            }
        })
    });
    group.finish();
}

criterion_group!(benches, bench_advance_chunks);
criterion_main!(benches);
