# tpt-async-macros

> **Proc-macro entry points for the tpt-async stack**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.88.

## Overview

Provides `#[tpt_async::main]`, turning an `async fn main` into a real
entry point driven by the tpt-async executor. The macro expands to
paths owned by the `tpt-async` facade, so depending on the facade is
enough - no direct executor dependency, no hygiene surprises.
`Result` returns become process exit codes; a third-party runner can
be selected with `executor = "path::to::run"`.

## Usage

```rust
use tpt_async::prelude::*;

#[tpt_async::main]
async fn main() -> Result<(), std::io::Error> {
    println!("driven by tpt-async");
    Ok(())
}

// Third-party runner:
// #[tpt_async::main(executor = "my_runtime::block_on_all")]
```

## Highlights

- **`#[tpt_async::main]`** - async entry point on the bundled executor
- **`executor = "..."`** - drive with any `fn(Future) -> Output` runner
- **`Result` returns** - `Termination`-based exit codes, like std

## Testing

```sh
cargo test -p tpt-async-macros
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
