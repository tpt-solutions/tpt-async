# tpt-async-executor

> **Single-threaded cooperative executor with thread-safe wakers**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.88.

## Overview

A small, single-threaded, cooperative executor. Futures are polled on
the calling thread (both `Send` and `!Send` are accepted), but wakers
are fully thread-safe: any thread may wake a task. When nothing is
runnable, `block_on` parks instead of spinning, so timers, channels,
and other threads work as expected. Dropping the executor cancels its
pending tasks (`Task` handles resolve to `Cancelled`).

## Usage

```rust
use std::time::Duration;
use tpt_async_executor::prelude::*;
use tpt_async_timer::driver::sleep;

let executor = LocalExecutor::new();
let ex = executor.clone();
executor.block_on(async move {
    let task = ex.spawn_task_local(async { 6 * 7 });
    sleep(Duration::from_millis(1)).await;
    assert_eq!(task.await, Ok(42));
});
```

## Highlights

- **`block_on`** - runs a future to completion, parking when idle
- **`run_until_stalled`** - drain ready tasks without a driving future
- **thread-safe wakers** - wake from any thread; std::task::Wake based, no raw-pointer tricks
- **cancel-on-drop** - `spawn_task` returns a `Task` that cancels when dropped; `detach()` for fire-and-forget

## Feature flags

| Flag | Enables |
|------|---------|
| `(none)` | the executor is std-based; bring your own for no_std |

## Testing

```sh
cargo test -p tpt-async-executor
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
