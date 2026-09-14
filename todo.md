# tpt-async — Development Checklist

> **License:** MIT OR Apache-2.0 · **Copyright:** TPT Solutions  
> **MSRV:** Rust 1.75 (async fn in traits stable)  
> **Versioning:** Lock-step 0.1.0 across all crates  
> **Crates:** 9 total (7 spec + tpt-async-macros + tpt-async facade)

---

## Workspace Bootstrap

- [x] `cargo new --workspace tpt-async` — create workspace root
- [x] Set `resolver = "2"` in root `Cargo.toml`
- [x] Add all 9 crates to `[workspace.members]`
- [x] Add `[workspace.package]` block: `version = "0.1.0"`, `edition = "2021"`, `rust-version = "1.75"`, `license = "MIT OR Apache-2.0"`, `authors = ["TPT Solutions"]`
- [x] Create `LICENSE-MIT` and `LICENSE-APACHE` files in workspace root
- [x] Add `[workspace.dependencies]` for shared pinned deps (rustls 0.23, etc.)
- [x] Create `.cargo/config.toml` with target aliases for no_std builds
- [x] Create root `README.md` with project vision and crate map

---

## Phase 1 — Foundational Scheduling & Timing (Months 1–2)

### `crates/tpt-async-core`
> Executor-agnostic Future traits, Waker abstractions, zero-cost state machine helpers

- [x] `cargo new --lib crates/tpt-async-core`
- [x] Set `#![no_std]` with `extern crate alloc` behind `alloc` feature flag
- [x] Define `Spawn` trait — implemented by user's runtime or default executor
- [x] Define `JoinHandle<T>` + `Completer` types (alloc)
- [x] Define standalone `Task` type (alloc): cancel-on-drop handle with `detach()`; no-alloc callers use raw traits (true alloc-free spawn still open)
- [x] Implement custom `RawWaker` / `Waker` construction utilities
- [x] Write zero-cost state machine helper: `state_machine!` declarative macro (plain enums + const `match` transition fn, fully no_std)
- [x] Define `LocalSpawn` trait for single-threaded contexts
- [x] Add `prelude` module re-exporting core traits
- [x] Unit tests (no_std-compatible)
- [x] `docs.rs` attribute with `all-features`

### `crates/tpt-async-macros`
> Proc-macro crate: `#[tpt_async::main]`

- [x] `cargo new --lib crates/tpt-async-macros`
- [x] Set `[lib] proc-macro = true` in `Cargo.toml`
- [x] Implement `#[tpt_async::main]` that wraps `async fn main` with the configured executor entry point
- [x] Support `executor = "default"` / `executor = "my_runtime::run"` attribute arg (custom runner test in `tpt-async/tests/entrypoint_custom.rs`)
- [ ] Emit compile-error on `no_std` targets (macro is std-only entry-point sugar; currently fails only via unresolved paths)
- [x] Write macro expansion tests with `trybuild` (`tests/ui.rs`: non-async, wrong name, unknown arg, bad executor path)

### `crates/tpt-async-executor`
> Optional single-threaded cooperative executor

- [x] `cargo new --lib crates/tpt-async-executor`
- [x] Implement `LocalExecutor` — single-threaded, cooperative, poll-based
- [x] Use `VecDeque` or intrusive linked-list task queue (no heap per poll)
- [x] Implement `Spawn` trait from `tpt-async-core` for `LocalExecutor`
- [x] Implement `LocalSpawn` trait from `tpt-async-core`
- [x] Add `block_on(future)` (currently a `LocalExecutor` method, not a free function)
- [x] No `unsafe` except where strictly required for Waker raw pointer handling; document all `unsafe` blocks
- [ ] Feature flag `work-stealing` — reserved for Phase 2+ (empty stub feature removed to avoid false advertising)
- [x] Unit tests: nested spawns, waker re-use, duplicate-wake coalescing, cross-thread wake, task cancellation on executor drop
- [x] Benchmark vs. `tokio::task::LocalSet` with criterion (`benches/localset.rs`)

### `crates/tpt-async-timer`
> Heapless hierarchical timer wheel

- [x] `cargo new --lib crates/tpt-async-timer`
- [x] Set `#![no_std]` (no alloc required for core wheel)
- [x] Implement const-generic hierarchical timer wheel (`TimerWheel<const SLOTS: usize, const LEVELS: usize>`)
- [x] O(1) insertion and removal guarantee; document the invariant
- [x] Implement `Sleep` future backed by the wheel
- [x] Implement `Interval` future (periodic ticks)
- [x] Implement `Timeout<F>` combinator wrapping any `Future`
- [ ] Integrate with `tpt-async-core` `Waker` for wake-on-expiry (currently uses `core::task::Waker` directly)
- [x] Provide `std` feature that hooks into system monotonic clock (the std driver thread reads `StdClock` and calls `wheel.advance_to`)
- [x] Provide `fugit` compatibility behind the `fugit` feature (`FugitTimer` wrapper over `fugit_timer::Timer`; `embedded-time` skipped — unmaintained)
- [ ] no_std tests using `defmt-test` or `embedded-test`
- [ ] Fuzz tick-advance with `cargo-fuzz`

### `crates/tpt-async` (Facade)
> Re-exports prelude from all crates; user-facing entry point

- [x] `cargo new --lib crates/tpt-async`
- [x] Re-export `tpt-async-core::prelude` as `tpt_async::prelude`
- [x] Re-export `tpt-async-executor` behind `executor` feature (default on std)
- [x] Re-export `tpt-async-macros` so `#[tpt_async::main]` works from this crate
- [x] Feature flags: `alloc`, `std`, `executor`, `timer` — composable
- [x] Verify `use tpt_async::prelude::*` compiles the spec's `main` example (adapted to the real API in `tpt-async/tests/entrypoint*.rs`)
- [x] Top-level crate `README.md` with quick-start example (`crates/tpt-async/README.md`)

---

## Phase 2 — I/O Abstraction & TLS (Months 3–4)

### `crates/tpt-async-io`
> Zero-copy I/O traits over std / tokio / async-std / bare-metal

- [x] `cargo new --lib crates/tpt-async-io`
- [x] Define `AsyncRead` trait — no heap, buffer passed by caller (`&mut [u8]`)
- [x] Define `AsyncWrite` trait — [ ] vectored write (`IoSlice`) support still open
- [x] Define `AsyncBufRead` trait with fill-buf / consume model
- [x] Implement `StdCompat` adapter (synchronous `poll_*` calls — non-blocking fds only)
- [x] Implement `TokioCompat` adapter behind `tokio` feature flag (explicit `TokioReader`/`TokioWriter` wrappers)
- [x] Implement `AsyncStdCompat` adapter behind `async-std` feature flag
- [x] Embedded adapter behind the `embedded-io` feature: `EmbeddedIo` bridges `embedded-io-async` Read/Write (embassy ecosystem) to our traits (needs `alloc` for RPITIT boxing; no-alloc targets implement our traits directly)
- [x] Zero-copy `ReadBuf` type (non-allocating; tracks the filled sub-slice, not per-byte init state)
- [x] Integration tests for adapter pairings (tokio duplex round-trip, std cursor, async-std writer)
- [x] `AsyncReadExt`/`AsyncWriteExt` helpers (`read_exact`, `read_to_end`, `write_all`, `flush`, `shutdown`)
- [x] Vectored writes: std-only `AsyncWriteVectored` trait with native tokio gather support and a single-buffer fallback

### `crates/tpt-net-tls`
> rustls 0.23 wrapper; no OpenSSL, no ring fallback

- [x] `cargo new --lib crates/tpt-net-tls`
- [x] Depend on `rustls = "0.23"` with default-features = false (pure-Rust provider)
- [x] Implement `TlsConnector` wrapping `tpt-async-io::AsyncRead + AsyncWrite`
- [x] Implement `TlsAcceptor` for server-side TLS
- [x] Expose `rustls_config()` free function returning a locked-down `ClientConfig`
  - [x] TLS 1.3 only by default; TLS 1.2 opt-in behind feature flag
  - [x] System root certs via `rustls-native-certs` behind `native-certs` feature
  - [x] Bundled Mozilla roots via `webpki-roots` behind `webpki-roots` feature (default)
- [x] Certificate pinning helper: `PinnedCertVerifier` (SHA-256 pins, optional webpki fallback) + `pinned_connector`
- [x] No `unsafe` beyond rustls internals
- [x] Integration test: full TLS handshake against local `rcgen`-generated cert
- [x] Dependency audit: `cargo deny check` green — licenses allow-list extended for webpki-roots (CDLA-Permissive-2.0), rustls bumped past RUSTSEC vuln, async-std/rustls-pemfile/ring advisories ignored with justifications

---

## Phase 3 — High-Level Networking (Months 5–6)

### `crates/tpt-net-http`
> HTTP/1.1 + HTTP/2 client/server, zero-copy header parsing

- [x] `cargo new --lib crates/tpt-net-http`
- [x] HTTP/1.1 parser: request/response heads parsed into byte ranges over an `Arc` read buffer (zero-copy, safe)
- [x] HTTP/1.1 client: `ClientConnection` + `Request` builder (`HttpClient` holds optional TLS)
  - [x] `.tls` integration with `tpt-net-tls` (`HttpClient::with_tls` + `connect_tls`)
  - [x] `.timeout(Duration)` integration with `tpt-async-timer` (TLS connect/accept timeouts; request timeouts via `tpt_async::timeout`)
  - [x] Connection pooling: bounded per-host `Pool` + `Connector` trait; `HttpClient::request` returns owned responses
  - [x] Redirect following (max-hops, relative/absolute Location resolution)
- [x] HTTP/1.1 server: `serve_connection` keep-alive loop with RPITIT `Handler` trait (Router still open)
- [x] HTTP/2 client and server behind the `http2` feature (`h2` crate): `serve_h2` with `Http2Handler` (per-stream tokio tasks), `Http2Connection::connect`/`request` client, owned `H2Request`/`OwnedH2Response`; duplex round-trip tests
- [x] Streaming body readers (`Content-Length`, chunked, EOF-framed) with size caps
- [x] `Request` builder (method, target, headers, body; host/content-length auto-added)
- [x] RFC 9110/9112 edge cases tested (obs-fold, TE+CL smuggling, headerless requests, LF-only heads rejected as incomplete, 204/HEAD no-body, unsupported versions, oversized heads)
- [x] Fuzz scaffolds: `fuzz/` with `http_head_parser`, `timer_wheel_tick`, `ws_frame_decode` targets (run with `cargo +nightly fuzz run <target>`; ongoing fuzzing still open)
- [ ] Benchmark against `hyper` and `ureq` on throughput and latency (own-stack benches exist; cross-stack comparison still open)

### `crates/tpt-net-ws`
> Pure-Rust WebSocket framing, masking, ping/pong (client + server)

- [x] `cargo new --lib crates/tpt-net-ws`
- [x] WebSocket upgrade handshake (HTTP → WS) — client and server (SHA-1 accept via RustCrypto, known-answer tested)
- [x] Frame parser/serialiser: text, binary, continuation, ping, pong, close (bounds-checked, role-checked masking)
- [x] Masking/unmasking for client frames (RFC 6455 §5.3)
- [x] `WebSocketStream` type wrapping `tpt-async-io` transport (fragmentation assembly, auto-pong, close handshake)
- [x] Ping answered with Pong automatically (timer-driven keepalive scheduling still open)
- [x] Max frame size limit (16 MiB, configurable constant)
- [x] Per-message deflate behind the `permessage-deflate` feature: RSV1 codec support, client offer / server accept (no-context-takeover), per-message compress/decompress with decompression-bomb cap; integration verified over duplex
- [x] Integration tests: echo server (duplex), control frames, close handshake, dedicated fragmented-message assembly test
- [ ] Autobahn test suite pass (run via Docker in CI — needs a testee binary + container plumbing)

---

## CI / Infrastructure

- [x] `.github/workflows/ci.yml` — matrix:
  - [x] `x86_64-unknown-linux-gnu` — stable Rust + MSRV 1.75 (`cargo test --workspace`)
  - [x] `x86_64-pc-windows-msvc` — stable (`cargo test --workspace`)
  - [x] `thumbv7em-none-eabihf` — stable (`cargo build --no-std` check crates)
  - [x] `riscv32imac-unknown-none-elf` — stable (no_std build check)
  - [x] `wasm32-unknown-unknown` — stable (`cargo build` only; `wasm-pack test --headless` still open)
- [x] `cargo clippy --workspace --all-features -- -D warnings` in CI
- [x] `cargo fmt --check` in CI
- [x] `cargo doc --workspace --no-deps` — no warnings in CI
- [x] `cargo deny check` — license + advisory audit in CI
- [x] `cargo test --workspace` with `nextest` for faster output
- [x] Dependabot or Renovate for automated dependency PRs (`.github/dependabot.yml`, weekly, rustls patch-only)
- [x] `.cargo/audit.toml` pinning known-safe advisories (created; primary advisory gate remains `cargo deny`)

---

## Documentation & Release

- [x] Each crate: top-level doc comment with purpose, feature flags table, and quick example
- [x] Workspace-level `CONTRIBUTING.md`
- [x] `CHANGELOG.md` with Keep a Changelog format
- [x] `SECURITY.md` — responsible disclosure contact (TPT Solutions)
- [x] `deny.toml` — `[licenses]` allow list: `MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Unicode-DFS-2016`
- [ ] Publish dry-run: `cargo publish --dry-run` for each crate in dependency order
- [ ] Crates.io publish order (respects dep graph):
  1. `tpt-async-core`
  2. `tpt-async-macros`
  3. `tpt-async-executor`
  4. `tpt-async-timer`
  5. `tpt-async-io`
  6. `tpt-net-tls`
  7. `tpt-net-http`
  8. `tpt-net-ws`
  9. `tpt-async` (facade — last)
- [ ] Tag `v0.1.0` and create GitHub release with changelog excerpt

---

## Hardening (from adoption/usability review)

- [x] Fuzz `tpt-net-http` head parser scaffold in `fuzz/` (continuous fuzzing still open)
- [x] Fuzz timer wheel tick-advance scaffold in `fuzz/` (continuous fuzzing still open)
- [x] Autobahn echo client (`examples/autobahn-echo-client.rs`) for fuzzingserver mode — CI Docker wiring still open
- [x] Add `.cargo/audit.toml` pinning/documenting known-safe advisories
- [ ] Wire `tpt-async-timer` wake-on-expiry through `tpt-async-core::Waker` instead of `core::task::Waker` directly
- [x] Finish `AsyncWrite` vectored write (`IoSlice`) support (`AsyncWriteVectored`, tokio gather + fallback)
- [x] Add HTTP connection pooling (bounded, configurable) — `Pool` + `Connector` + `HttpClient::request` with redirects

## Innovative additions (from adoption/usability review)

- [x] `tpt-async-test` crate — `TestDriver` (manually-advanced wheel) + `io_pair` fake I/O with waker-based wakeup; 6 tests
- [ ] `Spawn`/I/O adapter for `embassy` — I/O side DONE (`EmbeddedIo` over `embedded-io-async`); the Spawn side needs embassy-executor review: our `Spawn::spawn` returns a `JoinHandle`, which embassy's fire-and-forget `Spawner::spawn` (SpawnToken) cannot provide — implement as a documented `spawn_fire_and_forget(spawner, fut)` helper instead of a trait impl
- [x] HTTP-agnostic `retry` + `RetryPolicy` (exponential backoff, capped) in `tpt-async-timer` (std feature), built on the driver's `sleep`
- [ ] `tracing`/`defmt` integration behind a feature flag — defmt tried and REVERTED: any `defmt::write!`/Format use references `_defmt_acquire`-family linker symbols that only resolve against an embedded defmt logger, so host `--all-features` test builds fail to link (CI gate). Feasible path: keep defmt impls in a separate `tpt-async-defmt` crate, or support per-target feature resolution. `tracing` (std) remains open and unaffected
- [x] CI job tracking dep count + release/wasm artifact sizes into the job summary (`.github/workflows/size.yml`); README badge still open

## Usability / automation (from adoption/usability review)

- [x] `cargo xtask ci` (xtask crate: fmt, clippy, test, docs, cross, deny; aliases kept for compatibility)
- [x] Pre-commit hook via `.githooks/` (fmt + clippy + commit-msg conventional check; dependency-free — `git config core.hooksPath .githooks`)
- [x] `release-plz` wired (`.github/workflows/release-plz.yml`; needs `CARGO_REGISTRY_TOKEN` secret on first publish)
- [x] Add `.github/ISSUE_TEMPLATE/` (bug report, feature request) and `PULL_REQUEST_TEMPLATE.md`; referenced from `CONTRIBUTING.md`
- [x] Obsolete — those items (IoSlice, redirects, pooling) are now implemented

## Adoption: examples & templates (from adoption/usability review)

- [ ] Embedded HTTP/WS example once a bare-metal `embedded-hal-async` I/O adapter exists — pairs the "HTTP+WS in 30 lines" story with actual embedded proof
- [x] `docs/MIGRATING_FROM_TOKIO.md` — side-by-side mapping for entry points, time, I/O traits, and the deliberate differences
- [ ] Live wasm playground/demo (built on `examples/wasm`) for browser-based try-before-install once `v0.1.0` is published
- [x] Runnable end-to-end example: `examples/desktop/src/bin/http-ws.rs` (HTTP client↔server + WS echo over in-memory pipes), linked from the README
