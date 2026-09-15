// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic timers on the shared wheel, driven through the std driver.

use std::time::Duration;

use tpt_async_executor::prelude::*;
use tpt_async_timer::prelude::*;

fn main() {
    let executor = LocalExecutor::new();

    executor.block_on(async {
        // One-shot sleep:
        sleep(Duration::from_millis(20)).await;
        println!("timers: slept 20 ms");

        // Timeout around work:
        let result = timeout(Duration::from_millis(100), async { "work done" }).await;
        println!("timers: timeout -> {result:?}");

        // Periodic ticks with a catch-up policy:
        let mut ticks = core::pin::pin!(interval(Duration::from_millis(30)));
        for i in 0..3 {
            ticks.as_mut().tick().await;
            println!("timers: tick {i}");
        }
    });
}
