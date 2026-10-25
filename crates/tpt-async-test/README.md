# tpt-async-test

> **Deterministic testing utilities - no wall-clock flakes**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.88.

## Overview

Test support for the tpt-async stack: `TestDriver` bundles a timer
wheel with a manually-advanced tick counter (time moves only when you
advance it), and `io_pair` builds connected in-memory transports with
real waker semantics - drain-to-EOF on shutdown, `BrokenPipe` after
the peer is gone.

## Usage

```rust
use std::pin::pin;
use tpt_async_test::TestDriver;
use tpt_async_timer::prelude::*;

let driver: TestDriver = TestDriver::new();
let mut sleep = pin!(Sleep::new(driver.wheel(), driver.now() + 10));
driver.advance(10); // time moves only when you say so
```

## Highlights

- **`TestDriver`** - manually-advanced wheel; advance by ticks or to a deadline
- **`io_pair`** - connected `FakeIo` transports: waker wakeup, drain-to-EOF, `BrokenPipe`

## Testing

```sh
cargo test -p tpt-async-test
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
