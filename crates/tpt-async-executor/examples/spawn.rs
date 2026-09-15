// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Spawn tasks, join them, and use cancel-on-drop `Task` handles.

use std::time::Duration;

use tpt_async_core::spawn::LocalSpawn as _;
use tpt_async_executor::prelude::*;

fn main() {
    let executor = LocalExecutor::new();
    let ex = executor.clone();

    let answer = executor.block_on(async move {
        // `spawn_local` returns a JoinHandle: detach-on-drop.
        let join = ex.spawn_local(async { 6 * 7 }).unwrap();

        // `spawn_task_local` returns a Task: cancel-on-drop.
        let task = ex.spawn_task_local(async { 1 + 1 });

        let joined = join.await.unwrap();
        let computed = task.await.unwrap();
        joined + computed
    });

    println!("spawn example: {answer}");

    // Tasks can also be cancelled explicitly:
    let ex = executor.clone();
    executor.block_on(async move {
        let task = ex.spawn_task_local(async { 0 });
        task.cancel();
        assert!(task.is_finished());
    });

    std::thread::sleep(Duration::from_millis(1)); // keep timers out of the way
}
