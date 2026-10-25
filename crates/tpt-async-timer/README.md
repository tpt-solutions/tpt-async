# tpt-async-timer

> **Heapless hierarchical timer wheel - from Cortex-M to cloud**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.88.

## Overview

A const-generic hierarchical timer wheel with O(1) insert/remove and
amortised O(1) ticking. The core is no_std and allocation-free: on
embedded, register `Sleep`/`Interval`/`Timeout` futures against a
static wheel and advance it from your tick interrupt. On std, a
background driver thread does the advancing for you, so
`sleep(Duration)`/`timeout`/`interval` just work.

## Usage

```rust
use core::time::Duration;
use tpt_async_timer::prelude::*;

// std: the driver thread makes these just work:
// sleep(Duration::from_millis(50)).await;
// let r = timeout(Duration::from_secs(1), fut).await?;

// no_std / deterministic tests: drive the wheel yourself:
let wheel: tpt_async_timer::wheel::TimerWheel<64, 4> = TimerWheel::new();
let mut iv = core::pin::pin!(Interval::new(&wheel, 10));
// poll_tick / tick().await in your super-loop

// Missed ticks are policy, not surprises:
let _ = MissedTickBehavior::Skip; // Burst / Delay / Skip
```

## Highlights

- **`TimerWheel<SLOTS, LEVELS>`** - O(1) insert/remove, amortised O(1) tick, `&self` API shareable across threads
- **`Sleep` / `Timeout`** - shared-reference futures; drop-safe deregistration
- **`Interval` + `MissedTickBehavior`** - Burst / Delay / Skip catch-up policies
- **std driver** - background thread + `sleep`/`timeout`/`interval` free functions
- **`fugit` compat** - `FugitTimer` bridges RTIC-compatible timers via the `fugit` feature

## Feature flags

| Flag | Enables |
|------|---------|
| `std (default)` | driver thread + free functions |
| `alloc` | reserved |
| `fugit` | `fugit_timer::Timer` compatibility |

## Testing

```sh
cargo test -p tpt-async-timer
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
