# Migrating from tokio

The tpt-async API is deliberately close to tokio's in shape, but it has no
built-in runtime: you bring the executor and I/O driver (or use the bundled
`LocalExecutor` + timer driver for pure-logic workloads).  This guide maps
the common operations.

## Entry point

| tokio | tpt-async |
|---|---|
| `#[tokio::main]` | `#[tpt_async::main]` (facade `macros` feature; expands to `LocalExecutor::block_on`) |
| `Runtime::new()?.block_on(f)` | `LocalExecutor::new().block_on(f)` |
| spawning onto a handle | `tpt_async::runtime::tokio_support::TokioHandle(rt.handle()).spawn(f)` → `tpt-async` `JoinHandle` |

The `LocalExecutor` accepts `!Send` futures (`spawn_local`) and parks the
thread when idle, so external wakes (timers, channels, other threads) work
without spinning.

## Time

| tokio | tpt-async |
|---|---|
| `tokio::time::sleep(d)` | `tpt_async::sleep(d)` |
| `tokio::time::timeout(d, fut)` | `tpt_async::timeout(d, fut)` |
| `tokio::time::interval(d)` | `tpt_async::interval(d)` + `MissedTickBehavior` |
| periodic on a runtime thread | embedded: drive a `TimerWheel` from your tick interrupt — no threads |

The std timer driver starts a background thread on first use; on `no_std`
targets you advance the wheel yourself (see `examples/embedded`).

## I/O traits

| tokio | tpt-async |
|---|---|
| `tokio::io::AsyncRead` / `AsyncWrite` | `tpt_async_io::AsyncRead` / `AsyncWrite` |
| `AsyncReadExt::read_exact` etc. | `AsyncReadExt` / `AsyncWriteExt` (same names, poll-based traits) |
| `TcpStream` directly | wrap it: `TokioReader::new(half)` / `TokioCompat::new(stream)`; or implement the traits on your own transport |

Under the `tokio` feature, tokio sockets plug straight in — the tokio
runtime remains the readiness driver while your protocol code speaks only
`tpt-async-io` traits (see `examples/desktop/src/bin/tcp-echo.rs`).

## Things that do NOT map one-to-one

- **No built-in channels / mutexes / sync primitives** — bring `futures`,
  `crossbeam`, or std sync; the stack only owns scheduling, time, and I/O
  abstraction.
- **`Spawn::spawn` returns `Result`** — a `JoinHandle<T>` resolves to
  `Result<T, Cancelled>`; tasks are cancelled when the executor drops them.
- **`JoinHandle` is not `Send`** and the executor handle is thread-local —
  cloning shares the run queue, but spawning/polling stays on one thread.
- **No `select!`** — compose with the timer (`timeout`) or write small
  futures; the trait surface is deliberately tiny.

## Feature flags to get started

```toml
[dependencies]
tpt-async = { version = "0.1", features = ["macros", "io"] }
```

Add `"tls"` for rustls re-exports, `"spawn-tokio"` for the tokio `Spawn`
adapter, `"spawn-smol"` for smol.
