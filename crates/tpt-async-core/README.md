# tpt-async-core

> **Executor-agnostic core traits for the tpt-async stack**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.75.

## Overview

`tpt-async-core` is the foundation of the tpt-async workspace: the
`Spawn`/`LocalSpawn` traits that any executor can implement, the
`JoinHandle`/`Completer`/`Task` plumbing that links spawned futures to
their results, waker construction utilities (including allocation-free
`waker_fn` and closure-based `waker_with`), and a zero-cost
`state_machine!` generator.

Everything here is `no_std` (with an optional `alloc` feature) and has
no runtime opinions: bring any executor - tokio, smol, embassy, the
bundled `tpt-async-executor`, or your own.

## Usage

```rust
use tpt_async_core::prelude::*;
use tpt_async_core::waker::waker_fn;

// Any executor can implement `Spawn`/`LocalSpawn`; the bundled
// `tpt-async-executor` does. Waker helpers cover the construction side:
let waker = waker_fn(|| println!("task woken"));

// `state_machine!` generates plain enums + a const match transition fn:
tpt_async_core::state_machine! {
    pub Conn, ConnEvent {
        states: [Closed, Open],
        events: [Connect, Disconnect],
        Closed -- Connect --> Open,
        Open -- Disconnect --> Closed,
    }
}

let s = Conn::Closed.transition(ConnEvent::Connect);
assert_eq!(s, Some(Conn::Open));
```

## Highlights

- **`Spawn` / `LocalSpawn`** - the traits executors implement - no `async-trait`, no heap dispatch
- **`JoinHandle` / `Completer` / `Task`** - result plumbing with cancel-on-drop semantics (`alloc`)
- **waker utilities** - `noop_waker`, `waker_fn` (stateless fn pointers), `waker_with` (closure + Arc)
- **`state_machine!`** - zero-cost declarative state machines: plain enums + const match

## Feature flags

| Flag | Enables |
|------|---------|
| `alloc (default)` | `JoinHandle`, `Completer`, `Task`, closure waker |
| `std` | `std::error::Error` impls on error types |
| `defmt-03` | `defmt::Format` impls for error types (embedded logging) |

## Testing

```sh
cargo test -p tpt-async-core
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
