# tpt-async

> **The runtime-agnostic async I/O stack for Rust.**
> From a 2 MB RAM Cortex-M microcontroller to a 64-core cloud server — one API, no forced executor, no 150-crate dependency tree.

[![CI](https://github.com/tpt-solutions/tpt-async/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-async/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/tpt-async.svg)](https://crates.io/crates/tpt-async)
[![docs.rs](https://docs.rs/tpt-async/badge.svg)](https://docs.rs/tpt-async)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

## Quick Start

```toml
[dependencies]
tpt-async = { version = "0.1", features = ["macros", "io"] }
```

```rust,no_run
use std::time::Duration;
use tpt_async::prelude::*;

#[tpt_async::main]
async fn main() -> Result<(), tpt_async::Timeout> {
    // The timer's std driver makes sleep/timeout just work — no runtime setup:
    sleep(Duration::from_millis(10)).await;

    let answer = timeout(Duration::from_secs(1), async { 42 }).await?;
    println!("answer: {answer}");
    Ok(())
}
```

A runnable version of this lives in the repo — see [Examples](#examples).

## Workspace Crates

| Crate | Description | `no_std` |
|-------|-------------|----------|
| [`tpt-async-core`](crates/tpt-async-core) | `Spawn`/`LocalSpawn` traits, `JoinHandle`, Waker utilities | ✅ |
| [`tpt-async-macros`](crates/tpt-async-macros) | `#[tpt_async::main]` proc-macro | — |
| [`tpt-async-executor`](crates/tpt-async-executor) | Single-threaded cooperative executor with thread-safe wakers and real parking | `alloc` |
| [`tpt-async-timer`](crates/tpt-async-timer) | Const-generic heapless hierarchical timer wheel + std driver (`sleep`/`timeout`/`interval`, `MissedTickBehavior`) | ✅ |
| [`tpt-async-io`](crates/tpt-async-io) | Zero-copy `AsyncRead`/`AsyncWrite` + ext traits; std/tokio/async-std adapters | `alloc` |
| [`tpt-net-tls`](crates/tpt-net-tls) | rustls 0.23 wrapper — TLS 1.3 by default, handshake timeouts, PEM loaders, no OpenSSL | std |
| [`tpt-net-http`](crates/tpt-net-http) | HTTP/1.1 client/server with zero-copy header parsing | std |
| [`tpt-net-ws`](crates/tpt-net-ws) | RFC 6455 WebSocket client/server over any `tpt-async-io` transport | std |
| [`tpt-async`](crates/tpt-async) | Facade re-exporting `tpt_async::prelude` | feature-gated |

## Design Principles

- **No `async-trait`** — uses RPITIT (async fn in traits, stable since Rust 1.75). Zero heap allocation from trait dispatch.
- **Heapless timers** — const-generic hierarchical timer wheel: O(1) insertion/removal, zero dynamic allocation. On `std` a background driver thread drives it for you; on embedded you drive it from your tick interrupt.
- **Zero-copy HTTP headers** — request/response heads are parsed directly into byte *ranges* over the connection's read buffer (`Arc`-shared). No per-header allocation.
- **Bring your own executor** — implement the `Spawn` trait for your runtime, or use the bundled `spawn-tokio`/`spawn-smol` adapters and the built-in `LocalExecutor`.
- **Audited TLS chain** — rustls 0.23 only, TLS 1.3 by default, no OpenSSL path, `cargo deny` enforced.

## Examples

| Example | What it shows | Run |
|---------|---------------|-----|
| [`examples/desktop`](examples/desktop) | `heartbeat` (pure stack: sleep/timeout/interval) and `tcp-echo` (tokio I/O driver + tpt traits + `Spawn` adapter + timer timeouts) | `cargo run -p desktop-examples --bin heartbeat` |
| [`examples/embedded`](examples/embedded) | Heapless timer wheel + `Sleep` futures on Cortex-M, zero allocation | `cargo build --manifest-path examples/embedded/Cargo.toml --target thumbv7em-none-eabihf --release` |
| [`examples/wasm`](examples/wasm) | Timer wheel driven by `performance.now()` | `cargo build --manifest-path examples/wasm/Cargo.toml --target wasm32-unknown-unknown` |
| [`templates/tpt-app`](templates/tpt-app) | cargo-generate scaffold for new projects | `cargo generate tpt-solutions/tpt-async --branch main` |

## Supported Targets

- `x86_64-unknown-linux-gnu` / `x86_64-pc-windows-msvc`
- `thumbv7em-none-eabihf` (ARM Cortex-M — core + timer crates)
- `riscv32imac-unknown-none-elf` (RISC-V bare-metal — core + timer crates)
- `wasm32-unknown-unknown` (browser — core + timer + executor; timer's std driver thread requires native targets)

## HTTP + WebSocket in 30 lines

```rust,no_run
use tpt_net_http::prelude::*;
use tpt_net_ws::prelude::*;
use tpt_async_io::TokioCompat;

// ── Serve HTTP on any transport ──────────────────────────────────────────────
struct Hello;
impl Handler<TokioCompat<tokio::io::DuplexStream>> for Hello {
    async fn handle(&mut self, _req: &mut ServerRequest<'_, TokioCompat<tokio::io::DuplexStream>>) -> ResponseData {
        ResponseData::ok("hello".as_bytes().to_vec())
    }
}

// ── Ask for a WebSocket upgrade on the server side ───────────────────────────
// let ws = WebSocketStream::accept(io).await?;
// ws.send(Message::Text("hello".into())).await?;

// ── Client side ──────────────────────────────────────────────────────────────
// let mut conn = ClientConnection::new(io);
// let mut resp = conn.send(&Request::new("GET", "/health"), "example.com").await?;
// assert_eq!(resp.status(), 200);
```

## Status & Roadmap

- ✅ Core traits, executor, heapless timers (+ std driver), I/O traits & adapters, TLS wrapper, HTTP/1.1 client/server (zero-copy parser, keep-alive, chunked bodies), WebSocket (handshake, fragmentation, ping/pong, close, 16 MiB cap)
- 🚧 HTTP/2, permessage-deflate, Autobahn test suite, connection pooling — see [`todo.md`](todo.md)

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
Copyright 2026 TPT Solutions.
