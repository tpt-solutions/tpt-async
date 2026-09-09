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
tpt-async  = "0.1"
tpt-net-http = "0.1"
tpt-net-tls  = "0.1"
```

```rust
use tpt_async::prelude::*;
use tpt_net_http::prelude::*;

#[tpt_async::main]
async fn main() {
    let client = HttpClient::builder()
        .tls(tpt_net_tls::rustls_config())
        .build();

    let response = client
        .get("https://api.tpt.solutions/health")
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .unwrap();

    println!("Status: {}", response.status());
}
```

## Workspace Crates

| Crate | Description | `no_std` |
|-------|-------------|----------|
| [`tpt-async-core`](crates/tpt-async-core) | `Spawn`/`LocalSpawn` traits, Waker utils, state-machine helpers | ✅ |
| [`tpt-async-macros`](crates/tpt-async-macros) | `#[tpt_async::main]` proc-macro | — |
| [`tpt-async-executor`](crates/tpt-async-executor) | Optional single-threaded cooperative executor | `alloc` |
| [`tpt-async-timer`](crates/tpt-async-timer) | Const-generic heapless hierarchical timer wheel | ✅ |
| [`tpt-async-io`](crates/tpt-async-io) | Zero-copy `AsyncRead`/`AsyncWrite` over std/tokio/async-std/bare-metal | `alloc` |
| [`tpt-net-tls`](crates/tpt-net-tls) | rustls 0.23 wrapper — no OpenSSL | std |
| [`tpt-net-http`](crates/tpt-net-http) | HTTP/1.1 + HTTP/2 zero-copy client/server | std |
| [`tpt-net-ws`](crates/tpt-net-ws) | Pure-Rust WebSocket client/server | std |
| [`tpt-async`](crates/tpt-async) | Facade re-exporting `tpt_async::prelude` | feature-gated |

## Design Principles

- **No `async-trait`** — uses RPITIT (async fn in traits, stable since Rust 1.75). Zero heap allocation from trait dispatch.
- **Heapless timers** — const-generic hierarchical timer wheel: O(1) insertion/removal, zero dynamic allocation.
- **Zero-copy I/O** — HTTP headers parsed directly into `&[u8]` slices from the I/O buffer.
- **Bring your own executor** — implement `Spawn` for your runtime (tokio, async-std, embassy, smol) and everything works.
- **Audited TLS chain** — rustls 0.23 only, TLS 1.3 by default, no OpenSSL path, `cargo deny` enforced.

## Supported Targets

- `x86_64-unknown-linux-gnu` / `x86_64-pc-windows-msvc`
- `thumbv7em-none-eabihf` (ARM Cortex-M, core + timer crates)
- `riscv32imac-unknown-none-elf` (RISC-V bare-metal, core + timer crates)
- `wasm32-unknown-unknown` (Browser / WASI)

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.  
Copyright 2026 TPT Solutions.
