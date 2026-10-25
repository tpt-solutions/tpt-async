# tpt-net-ws

> **RFC 6455 WebSocket client/server - pure Rust, no callbacks**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.88.

## Overview

WebSocket built directly on `tpt-net-http` framing and the
`tpt-async-io` traits: a real opening handshake, a bounds-checked
frame codec with role-checked masking, message fragmentation assembly,
automatic ping/pong, a proper closing handshake, and a 16 MiB payload
cap. Optional `permessage-deflate` compression (RFC 7692,
no-context-takeover) behind a feature flag.

## Usage

```rust
use tpt_net_ws::prelude::*;

// Server side: accept an upgrade over any transport.
// let mut ws = WebSocketStream::accept(io).await?;
//
// Client side:
// let mut ws = WebSocketStream::connect(io, "/ws", "example.com").await?;
//
// ws.send(Message::Text("hello".into())).await?;
// while let Some(msg) = ws.recv().await? {
//     ws.send(msg).await?; // echo
// }
```

## Highlights

- **handshake** - client + server upgrade with SHA-1 accept (known-answer tested)
- **frame codec** - bounds-checked, role-aware masking, extended lengths, control-frame limits
- **messages** - fragmentation assembly, UTF-8 validation, auto-pong, close handshake
- **`permessage-deflate`** - RFC 7692 compression with a decompression-bomb cap

## Feature flags

| Flag | Enables |
|------|---------|
| `permessage-deflate` | RFC 7692 compression (flate2) |

## Testing

```sh
cargo test -p tpt-net-ws
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
