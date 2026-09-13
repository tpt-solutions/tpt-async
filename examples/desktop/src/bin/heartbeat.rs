//! `heartbeat` — the pure `tpt-async` stack, zero external runtime.
//!
//! Drives `LocalExecutor::block_on` with the timer crate's std driver:
//! sleeps, intervals with missed-tick policies, and timeouts.  Run with:
//!
//! ```text
//! cargo run -p desktop-examples --bin heartbeat
//! ```

use std::time::Duration;

use tpt_async::prelude::*;

fn main() {
    let executor = LocalExecutor::new();

    executor.block_on(async {
        println!("heartbeat: starting (pure tpt-async, no tokio/async-std)");

        // 1. Plain sleep — backed by the global timer driver thread.
        sleep(Duration::from_millis(100)).await;
        println!("heartbeat: slept 100 ms");

        // 2. Timeout around a future that finishes in time.
        let fast = timeout(Duration::from_millis(500), async {
            sleep(Duration::from_millis(50)).await;
            "fast work done"
        })
        .await;
        println!("heartbeat: timeout Ok -> {fast:?}");

        // 3. Timeout around a future that never finishes.
        let slow: Result<(), TimedOut> =
            timeout(Duration::from_millis(100), core::future::pending()).await;
        println!("heartbeat: timeout of a pending future -> {slow:?}");

        // 4. Interval with the Skip missed-tick policy: periodic work stays
        //    aligned even if a tick of work overruns.
        let mut ticks = core::pin::pin!(interval(Duration::from_millis(60)));
        for i in 0..3 {
            ticks.as_mut().tick().await;
            println!("heartbeat: interval tick {i}");
        }

        println!("heartbeat: done");
    });
}
