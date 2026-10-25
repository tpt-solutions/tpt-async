# tpt-async

> **The runtime-agnostic async I/O stack for Rust.**
> From a 2 MB RAM Cortex-M microcontroller to a 64-core cloud server — one API, no forced executor.

[![CI](https://github.com/tpt-solutions/tpt-async/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-async/actions/workflows/ci.yml)
[![Size](https://github.com/tpt-solutions/tpt-async/actions/workflows/size.yml/badge.svg)](https://github.com/tpt-solutions/tpt-async/actions/workflows/size.yml)
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

## Why tpt-async instead of Tokio?

Tokio is a mature, well-tested runtime with a huge ecosystem, and for a typical server it is the right default. tpt-async is for the cases where Tokio is a poor fit:

- **Your code has to run where Tokio can't.** The core and timer crates are `#![no_std]` with no allocation in the timer wheel. They run on Cortex-M and RISC-V microcontrollers and in the browser (WASM), and the timer is driven from your own tick interrupt on bare metal.
- **You want the executor to be a choice, not a given.** The I/O, HTTP and WebSocket crates are written against `AsyncRead`/`AsyncWrite` and the `Spawn` trait, not a specific runtime. Use Tokio, smol, the bundled `LocalExecutor`, or your own, and swap them without rewriting the networking code.
- **You want to test networking without a runtime.** HTTP and WebSocket run over any transport, including in-memory pipes, and `tpt-async-test` provides a manually advanced timer and fake I/O, so tests are deterministic.
- **You want a lighter HTTP stack.** Request and response headers are parsed into byte ranges over the connection buffer, with no per-header allocation.

Stick with Tokio if you need its ecosystem directly (for example, crates that only accept Tokio types), or if you are already happy with it. The crates here can still use Tokio as a transport through `TokioCompat` and the `spawn-tokio` adapter. See [`docs/MIGRATING_FROM_TOKIO.md`](docs/MIGRATING_FROM_TOKIO.md) for how Tokio concepts map onto tpt-async.

## Workspace Crates

| Crate | Description | `no_std` |
|-------|-------------|----------|
| [`tpt-async-core`](crates/tpt-async-core) | `Spawn`/`LocalSpawn` traits, `JoinHandle`, Waker utilities | ✅ |
| [`tpt-async-macros`](crates/tpt-async-macros) | `#[tpt_async::main]` proc-macro | — |
| [`tpt-async-executor`](crates/tpt-async-executor) | Single-threaded cooperative executor with thread-safe wakers and real parking | `alloc` |
| [`tpt-async-timer`](crates/tpt-async-timer) | Const-generic heapless hierarchical timer wheel + std driver (`sleep`/`timeout`/`interval`, `MissedTickBehavior`) | ✅ |
| [`tpt-async-io`](crates/tpt-async-io) | Zero-copy `AsyncRead`/`AsyncWrite` + ext traits; std, tokio, async-std and embedded-io adapters | `alloc` |
| [`tpt-net-tls`](crates/tpt-net-tls) | rustls 0.23 wrapper — TLS 1.3 by default, TLS 1.2 opt-in, handshake timeouts, certificate pinning, PEM loaders, no OpenSSL | std |
| [`tpt-net-http`](crates/tpt-net-http) | HTTP/1.1 client/server with zero-copy header parsing, connection pooling and redirects; HTTP/2 behind `http2` | std |
| [`tpt-net-ws`](crates/tpt-net-ws) | RFC 6455 WebSocket client/server over any `tpt-async-io` transport; `permessage-deflate` behind a feature | std |
| [`tpt-async-test`](crates/tpt-async-test) | Deterministic testing utilities — manually-advanced timer driver and in-memory fake I/O pairs | std |
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

The desktop examples also include `http-ws` (HTTP client/server plus a WebSocket echo over in-memory pipes; see [below](#http--websocket-in-30-lines)): `cargo run -p desktop-examples --bin http-ws`.

## Supported Targets

- `x86_64-unknown-linux-gnu` / `x86_64-pc-windows-msvc`
- `thumbv7em-none-eabihf` (ARM Cortex-M — core + timer crates)
- `riscv32imac-unknown-none-elf` (RISC-V bare-metal — core + timer crates)
- `wasm32-unknown-unknown` (browser — core + timer + executor; timer's std driver thread requires native targets)

**Minimum supported Rust version: 1.88.** CI tests stable and 1.88 on Linux and Windows.

## Documentation

- [`docs/MIGRATING_FROM_TOKIO.md`](docs/MIGRATING_FROM_TOKIO.md) — mapping Tokio concepts onto tpt-async
- [`spec.txt`](spec.txt) — design document: architecture and the reasoning behind it
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contribution invariants (no `async-trait`, no OpenSSL, `#![no_std]` core and timer, `// SAFETY:` on every `unsafe` block)

## HTTP + WebSocket in 30 lines

```rust,no_run
use tpt_net_http::prelude::*;
use tpt_net_ws::prelude::*;
use tpt_async_io::TokioCompat;

// ── Serve HTTP: handler trait, zero-copy request access ─────────────────────
struct Health;
impl Handler<TokioCompat<tokio::io::DuplexStream>> for Health {
    async fn handle(
        &mut self,
        req: &mut ServerRequest<'_, TokioCompat<tokio::io::DuplexStream>>,
    ) -> ResponseData {
        assert_eq!(req.method(), b"GET");
        ResponseData::ok(b"ok".as_slice())
    }
}

// ── Accept a WebSocket upgrade on the server side ───────────────────────────
// let ws = WebSocketStream::accept(io).await?;
// ws.send(Message::Text("hello".into())).await?;

// ── Client side: pooled request with redirects ──────────────────────────────
// let pool = Pool::new(my_connector, 8); // any `Connector` impl: TCP, TLS, or in-memory
// let resp = HttpClient::new().request(&pool, &Request::new("GET", "/health"), 3).await?;
// assert_eq!(resp.status, 200);
```

Everything above runs over **any** transport: tokio sockets via `TokioCompat`,
TLS via `tpt-net-tls`, or in-memory pipes.

A fully runnable version of this — HTTP client ↔ server plus a WebSocket
echo over in-memory pipes — lives in
[`examples/desktop/src/bin/http-ws.rs`](examples/desktop/src/bin/http-ws.rs):
`cargo run -p desktop-examples --bin http-ws`.

## Status & Roadmap

- ✅ Core traits, executor, heapless timers (+ std driver), I/O traits & adapters, TLS wrapper (TLS 1.3 default, certificate pinning), HTTP/1.1 client/server (zero-copy parser, keep-alive, chunked bodies, connection pooling, redirects), HTTP/2 client/server (`http2` feature), WebSocket (handshake, fragmentation, ping/pong, close, 16 MiB cap, Autobahn suite passing in CI)
- ✅ `permessage-deflate` (RFC 7692) behind a feature flag, with a decompression-bomb cap; the Autobahn suite runs with it enabled in CI
- ✅ Exact-match HTTP router (`tpt_net_http::router::Router`) and timer-driven WebSocket keepalive (`KeepAlive`)
- 🚧 Open: a work-stealing executor is planned for Phase 2+ (no feature flag yet) — see [`todo.md`](todo.md)

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
Copyright 2026 TPT Solutions.
