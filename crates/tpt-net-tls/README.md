# tpt-net-tls

> **rustls 0.23 wrapper - TLS 1.3 by default, no OpenSSL**

> Part of the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack. Licensed MIT OR Apache-2.0. MSRV 1.75.

## Overview

A minimal, locked-down TLS layer over rustls 0.23: TLS 1.3 by default
(TLS 1.2 opt-in), no OpenSSL anywhere, handshake timeouts, PEM
certificate/key loaders, and optional certificate pinning. Works over
any `tpt-async-io` transport - TCP sockets, in-memory pipes, or your
own hardware stream.

## Usage

```rust
use tpt_net_tls::prelude::*;

// Client with bundled Mozilla roots (webpki-roots feature, default):
let connector = TlsConnector::new(rustls_config().expect("trust store"));

// Server from PEM files:
// let certs = load_pem_certs(&mut reader)?;
// let key = load_pem_key(&mut reader)?;
// let config = server_config(certs, key)?;

// Certificate pinning (skip chain validation, trust exactly these):
// let verifier = pinned_connector(vec![pin_for_cert(&cert_der)]);

// let tls = connector.connect_timeout(server_name, stream, Duration::from_secs(5)).await?;
```

## Highlights

- **`TlsConnector` / `TlsAcceptor`** - client and server handshakes over any transport
- **`connect_timeout` / `accept_timeout`** - handshake deadlines via the timer driver
- **`rustls_config()`** - locked-down client config (ring, TLS 1.3, root store validated)
- **`load_pem_certs` / `load_pem_key`** - PEM helpers built on rustls-pemfile
- **`pinned_connector`** - SHA-256 certificate pinning, optional webpki fallback

## Feature flags

| Flag | Enables |
|------|---------|
| `webpki-roots (default)` | bundled Mozilla roots |
| `native-certs` | system trust store |
| `tls12` | opt-in TLS 1.2 |

## Testing

```sh
cargo test -p tpt-net-tls
```

See the [workspace README](https://github.com/tpt-solutions/tpt-async) for the full stack, examples, and design notes.
