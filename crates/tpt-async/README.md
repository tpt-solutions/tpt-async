# tpt-async

> **The one-dependency facade for the tpt-async stack**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.88.

## Overview

`tpt-async` re-exports the prelude of every sub-crate so applications
need a single dependency: the executor, timer, I/O traits, and TLS -
composed behind feature flags. `#[tpt_async::main]` (the `macros`
feature) turns an `async fn main` into a running program.

## Usage

```rust
use std::time::Duration;
use tpt_async::prelude::*;

#[tpt_async::main]
async fn main() -> Result<(), tpt_async::Timeout> {
    sleep(Duration::from_millis(10)).await;
    let answer = timeout(Duration::from_secs(1), async { 42 }).await?;
    println!("answer: {answer}");
    Ok(())
}
```

## Highlights

- **prelude** - executor + timer + core traits in one `use`
- **`macros`** - `#[tpt_async::main]`
- **`io` / `tls`** - I/O traits + TLS re-exports
- **`spawn-tokio` / `spawn-smol`** - ready-made `Spawn` adapters for existing runtimes

## Feature flags

| Flag | Enables |
|------|---------|
| `std/alloc/executor/timer (defaults)` | core stack |
| `macros` | `#[tpt_async::main]` (implies executor) |
| `io` | I/O traits + ext helpers |
| `tls` | tpt-net-tls (implies io) |
| `spawn-tokio / spawn-smol` | runtime adapters |

## Testing

```sh
cargo test -p tpt-async
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
