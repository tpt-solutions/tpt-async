# tpt-net-http

> **HTTP/1.1 + HTTP/2 with zero-copy header parsing**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.75.

## Overview

A spec-compliant HTTP/1.1 client and server plus HTTP/2 (via the h2
crate, behind the `http2` feature). Request/response heads are parsed
straight into byte ranges over an `Arc`-shared read buffer - zero
per-header allocation. Framing is strict: conflicting `Content-Length`
values, `Transfer-Encoding` + `Content-Length` together, and obsolete
line folding are rejected. Runtime-agnostic: everything runs over
`tpt-async-io` transports.

## Usage

```rust
use tpt_net_http::prelude::*;

// Client: connect, send, read - over any transport.
// let mut conn = ClientConnection::new(io);
// let mut resp = conn.send(&Request::new("GET", "/health"), "example.com").await?;
// assert_eq!(resp.status(), 200);
// let body = resp.body_bytes(64 * 1024).await?;

// Server: keep-alive loop + RPITIT handler (no async-trait):
// serve_connection(io, &mut handler).await?;
```

## Highlights

- **zero-copy headers** - `Arc` + byte ranges; no per-header allocation
- **`ClientConnection` / `Request`** - builder-style requests, streaming response bodies
- **`serve_connection`** - keep-alive server loop with `Handler` trait
- **pooling + redirects** - bounded per-host `Pool`, `Connector` trait, max-hops redirects
- **HTTP/2 (`http2`)** - `serve_h2` server + `Http2Connection` client via h2
- **strict framing** - smuggling vectors rejected: TE+CL, dual CL, obs-fold

## Feature flags

| Flag | Enables |
|------|---------|
| `http2` | HTTP/2 via h2 |
| `fuzz` | exposes `parse::fuzz_entry` for fuzzing harnesses |

## Testing

```sh
cargo test -p tpt-net-http
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
