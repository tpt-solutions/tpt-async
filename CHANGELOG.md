# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial workspace structure with 9 crates
- `tpt-async-core`: `Spawn`, `LocalSpawn` traits; `JoinHandle`; Waker utilities
- `tpt-async-macros`: `#[tpt_async::main]` proc-macro
- `tpt-async-executor`: single-threaded cooperative `LocalExecutor` with `block_on`
- `tpt-async-timer`: const-generic heapless hierarchical timer wheel; `Sleep`, `Interval`, `Timeout`
- `tpt-async-io`: `AsyncRead`, `AsyncWrite`, `AsyncBufRead` traits; std/tokio/async-std adapters
- `tpt-net-tls`: rustls 0.23 wrapper (`TlsConnector`, `TlsAcceptor`, `rustls_config`)
- `tpt-net-http`: HTTP/1.1 zero-copy client/server; `HttpClient` builder
- `tpt-net-ws`: WebSocket framing, masking, ping/pong; `WebSocketStream`
- `tpt-async`: facade crate re-exporting `tpt_async::prelude`
