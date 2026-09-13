// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Regression test for the `#[tpt_async::main]` hygiene bug: the macro must
//! expand to paths resolvable with **only** a `tpt-async` facade dependency
//! (it used to emit `::tpt_async_executor::…`, E0433 for facade-only users).
//!
//! This file is a `harness = false` test binary so it may define `fn main`
//! and run the macro expansion exactly as a user binary would.

use std::time::{Duration, Instant};

use tpt_async::prelude::*;

#[tpt_async::main]
async fn main() -> Result<(), &'static str> {
    let start = Instant::now();
    sleep(Duration::from_millis(25)).await;
    assert!(
        start.elapsed() >= Duration::from_millis(20),
        "sleep too short"
    );

    let value = timeout(Duration::from_millis(500), async { 6 * 7 })
        .await
        .map_err(|_| "timeout fired too early")?;
    assert_eq!(value, 42);

    println!("entrypoint regression test passed");
    Ok(())
}
