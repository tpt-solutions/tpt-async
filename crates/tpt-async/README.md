# tpt-async (facade)

The single-dependency entry point for the tpt-async runtime-agnostic async
stack. Re-exports `tpt_async::prelude` from all sub-crates.

## Quick start

```toml
[dependencies]
tpt-async = { version = "0.1", features = ["macros", "io"] }
```

```rust,no_run
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

## Feature flags

| Flag         | Default | What it enables |
|--------------|---------|-----------------|
| `std`        | yes     | std features in sub-crates |
| `alloc`      | yes     | `JoinHandle`, `Completer` |
| `executor`   | yes     | `LocalExecutor`, `block_on` |
| `timer`      | yes     | `Sleep`, `Interval`, `Timeout`, wheel, `sleep()`/`timeout()`/`interval()` |
| `macros`     | no      | `#[tpt_async::main]` (implies `executor`) |
| `io`         | no      | `AsyncRead`/`AsyncWrite` + ext traits + adapters |
| `tls`        | no      | `tpt-net-tls` re-exports (implies `io`) |
| `spawn-tokio`| no      | `impl Spawn` for tokio runtime handles |
| `spawn-smol` | no      | `spawn_on_smol()` helper |

See the [workspace README](../../README.md) for the full story.
