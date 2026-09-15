# tpt-async-io

> **Zero-copy async I/O traits with adapters for every runtime**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.75.

## Overview

The I/O abstraction of the tpt-async stack: `AsyncRead`/`AsyncWrite`
traits with a `ReadBuf` cursor (zero-copy, no per-call allocation),
`AsyncBufRead`, extension helpers (`read_exact`, `write_all`,
`read_to_end`...), and adapters for std, tokio, async-std,
`embedded-io-async`, and bare-metal `no_std` callers. Tokio sockets
get native vectored-write gather support through `TokioCompat`.

## Usage

```rust
use tpt_async_io::prelude::*;

// Wrap any tokio stream - everything downstream speaks only these traits:
// let mut writer = TokioWriter::new(socket.into_split().0);
// writer.write_all(b"hello").await?;
// writer.flush().await?;

let mut buf = [0u8; 64];
let mut rb = ReadBuf::new(&mut buf);
// reader.read(rb).await?; -> returns bytes read this call
```

## Highlights

- **`AsyncRead` / `AsyncWrite` / `AsyncBufRead`** - poll-based traits over caller-provided buffers
- **`ReadBuf`** - write-once cursor tracking the filled sub-slice
- **ext helpers** - `read_exact`, `read_to_end`, `write_all`, `flush`, `shutdown`
- **`AsyncWriteVectored`** - scatter/gather with tokio gather support + fallback
- **adapters** - `StdReader/Writer`, `TokioReader/Writer/Compat`, `AsyncStdReader/Writer`, `EmbeddedSerial`

## Feature flags

| Flag | Enables |
|------|---------|
| `std (default)` | `IoError` wraps `std::io::Error` with `std::error::Error` |
| `alloc (via std)` | `read_to_end` |
| `tokio` | `TokioReader`/`TokioWriter`/`TokioCompat` |
| `async-std` | `AsyncStdReader`/`AsyncStdWriter` |
| `embedded-io` | `EmbeddedIo` for `embedded-io-async` streams (embassy) |

## Testing

```sh
cargo test -p tpt-async-io
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
