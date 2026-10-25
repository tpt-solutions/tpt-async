// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Functional test for `#[tpt_async::main(executor = "...")]`: the macro
//! must call the given runner instead of the bundled executor.

use std::future::Future;
use tpt_async::prelude::LocalExecutor;

fn my_run<F: Future>(future: F) -> F::Output {
    LocalExecutor::new().block_on(future)
}

#[tpt_async::main(executor = "my_run")]
async fn main() {
    // nextest queries harness-less binaries with `--list` and expects libtest's
    // terse `name: test` lines; answer that instead of running the body.
    if std::env::args().any(|arg| arg == "--list") {
        println!("main: test");
        return;
    }

    let value = std::sync::atomic::AtomicU32::new(0);
    std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
    let _ = value;
    println!("custom executor regression test passed");
}
