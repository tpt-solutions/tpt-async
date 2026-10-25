# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- `tpt-net-http`: keep-alive is preserved when a request has `Content-Length: 0` and the handler does not read the body. Our client sends that header on every GET, so previously each such request closed the connection

### Changed
- MSRV raised from Rust 1.75 to Rust 1.88 (`rust-version` in the workspace, CI test matrix, and docs). Current dependencies, including `time` 0.3.55 and `trybuild` 1.0.121, require 1.88 and edition 2024 support in Cargo

### Added
- Initial workspace structure with 9 crates
- `tpt-async-core`: `Spawn`, `LocalSpawn` traits; `JoinHandle`; Waker utilities
- `tpt-async-macros`: `#[tpt_async::main]` proc-macro
- `tpt-async-executor`: single-threaded cooperative `LocalExecutor` with `block_on`
- `tpt-async-timer`: const-generic heapless hierarchical timer wheel; `Sleep`, `Interval`, `Timeout`
- `tpt-async-io`: `AsyncRead`, `AsyncWrite`, `AsyncBufRead` traits; `AsyncReadExt`/`AsyncWriteExt` helpers; `StdReader`/`StdWriter` adapters, `TokioReader`/`TokioWriter` (`tokio` feature), `AsyncStdReader`/`AsyncStdWriter` (`async-std` feature)
- `tpt-net-tls`: rustls 0.23 wrapper (`TlsConnector`, `TlsAcceptor`, `rustls_config`)
- `tpt-net-http` / `tpt-net-ws`: crate skeletons — the HTTP/1.1 and WebSocket implementations land in an upcoming release
- `tpt-async`: facade crate re-exporting `tpt_async::prelude`

### Added (continued)
- `tpt-net-http`: `router::Router`, exact `(method, path)` routing with a `405` for a known path under another method, a `404` default, and `fallback` to replace it. Exposed in the prelude
- `tpt-net-ws`: timer-driven keepalive via `WebSocketStream::set_keepalive(Some(KeepAlive { interval, timeout }))`. After `interval` of silence a `Ping` is sent; if nothing arrives within `timeout` more, `recv` fails with `WsError::TimedOut`. Disabled by default
- `tpt-async-executor`: optional `tracing` feature — a span per task poll and a trace event per spawn
- `tpt-net-http`: HTTP/1.1 zero-copy parser (`Arc` + byte ranges), `ClientConnection`/`Request`/`Response`, keep-alive `serve_connection` with RPITIT `Handler`, strict framing (rejects conflicting Content-Length, TE+CL, obs-fold), chunked + Content-Length + EOF bodies, TLS via `HttpClient::with_tls`
- `tpt-net-ws`: RFC 6455 `WebSocketStream` — opening handshake (client + server), frame codec with role-checked masking and 16 MiB cap, fragmentation assembly, auto-pong, closing handshake
- `tpt-async` facade: `io`/`tls` features, `spawn-tokio`/`spawn-smol` adapters, timer `sleep`/`timeout`/`interval` at the crate root
- Examples: desktop `heartbeat` + `tcp-echo` (tokio interop), Cortex-M `no_std` firmware demo, WASM demo, cargo-generate template
- CI: coverage (cargo-llvm-cov), cargo-semver-checks, tag-triggered release workflow with ordered publish

### Fixed
- `tpt-net-ws`: protocol violations now answer with a Close frame before the error is returned (1002, or 1007 for invalid UTF-8), per RFC 6455 §7.1.7
- `tpt-net-ws`: rejects fragmented control frames, one-byte Close payloads, invalid Close status codes, and invalid UTF-8 Close reasons (Autobahn cases 5.1, 5.2, 7.5.1, 7.9.x)
- `tpt-net-ws`: Autobahn echo testee sends Host with port (fixes handshake rejection) and speaks WebSocket on `/getCaseCount` and `/updateReports`; CI job now fails on any FAILED case and uses the `crossbario/autobahn-testsuite` image
- `fuzz`: `timer_wheel_tick` harness no longer indexes past an odd trailing byte and keeps every inserted entry alive until it is removed (the wheel holds raw pointers to registered entries)
- All crate manifests now inherit `version`/`edition`/`rust-version`/`license`/`authors`/`repository` from `[workspace.package]` (crates previously declared no edition — silently defaulting to 2015 — and no license, which would block crates.io publish)
- `tpt-async-io`: wired the previously undeclared `buf` (AsyncBufRead), `adapters`, and `prelude` modules; adapters now compile (`IoError::from` instead of the nonexistent `IoError::Std`, `poll_shutdown` instead of `poll_close`)
- `tpt-async-io`: replaced the coherence-conflicting tokio blanket impls (E0119 against the `&mut T` impls) with explicit `TokioReader`/`TokioWriter` wrapper types
- Removed the empty `work-stealing`, `http2`, and `permessage-deflate` feature flags until the corresponding code exists (no false advertising)
