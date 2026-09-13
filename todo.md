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
- [ ] Define standalone `Task` type (alloc-optional) — not yet implemented
- [x] Implement custom `RawWaker` / `Waker` construction utilities
- [ ] Write zero-cost state machine derive helper (or macro-based)
- [x] Define `LocalSpawn` trait for single-threaded contexts
- [x] Add `prelude` module re-exporting core traits
- [x] Unit tests (no_std-compatible)
- [x] `docs.rs` attribute with `all-features`

### `crates/tpt-async-macros`
> Proc-macro crate: `#[tpt_async::main]`

- [x] `cargo new --lib crates/tpt-async-macros`
- [x] Set `[lib] proc-macro = true` in `Cargo.toml`
- [x] Implement `#[tpt_async::main]` that wraps `async fn main` with the configured executor entry point
- [ ] Support `executor = "default"` attribute arg (extensible for third-party runtimes)
- [ ] Emit compile-error on `no_std` targets (macro is std-only entry-point sugar; currently fails only via unresolved paths)
- [ ] Write macro expansion tests with `trybuild` or `macrotest`

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
- [ ] Unit tests: nested spawns (done), waker re-use, task cancellation (pending)
- [ ] Benchmark vs. `tokio::task::LocalSet` with criterion

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
- [ ] Provide `std` feature that hooks into system monotonic clock (`StdClock` exists; not yet wired into `TimerWheel::tick`)
- [ ] Provide `embedded-time` / `fugit` compatibility behind feature flags
- [ ] no_std tests using `defmt-test` or `embedded-test`
- [ ] Fuzz tick-advance with `cargo-fuzz`

### `crates/tpt-async` (Facade)
> Re-exports prelude from all crates; user-facing entry point

- [x] `cargo new --lib crates/tpt-async`
- [x] Re-export `tpt-async-core::prelude` as `tpt_async::prelude`
- [x] Re-export `tpt-async-executor` behind `executor` feature (default on std)
- [x] Re-export `tpt-async-macros` so `#[tpt_async::main]` works from this crate
- [x] Feature flags: `alloc`, `std`, `executor`, `timer` — composable
- [ ] Verify `use tpt_async::prelude::*` compiles the spec's `main` example
- [ ] Top-level crate `README.md` with quick-start example

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
- [ ] Bare-metal adapter: register-mapped I/O via `embedded-hal-async` behind feature flag
- [x] Zero-copy `ReadBuf` type (non-allocating; tracks the filled sub-slice, not per-byte init state)
- [ ] Integration tests for each adapter pairing (std↔tokio, etc.)
- [x] `AsyncReadExt`/`AsyncWriteExt` helpers (`read_exact`, `read_to_end`, `write_all`, `flush`, `shutdown`)

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
- [ ] Certificate pinning helper
- [x] No `unsafe` beyond rustls internals
- [x] Integration test: full TLS handshake against local `rcgen`-generated cert
- [ ] Dependency audit: `cargo deny` check that no GPL or viral licenses enter

---

## Phase 3 — High-Level Networking (Months 5–6)

### `crates/tpt-net-http`
> HTTP/1.1 + HTTP/2 client/server, zero-copy header parsing

- [x] `cargo new --lib crates/tpt-net-http`
- [x] HTTP/1.1 parser: request/response heads parsed into byte ranges over an `Arc` read buffer (zero-copy, safe)
- [x] HTTP/1.1 client: `ClientConnection` + `Request` builder (`HttpClient` holds optional TLS)
  - [x] `.tls` integration with `tpt-net-tls` (`HttpClient::with_tls` + `connect_tls`)
  - [x] `.timeout(Duration)` integration with `tpt-async-timer` (TLS connect/accept timeouts; request timeouts via `tpt_async::timeout`)
  - [ ] Connection pooling (bounded, configurable)
  - [ ] Redirect following (max-hops configurable)
- [x] HTTP/1.1 server: `serve_connection` keep-alive loop with RPITIT `Handler` trait (Router still open)
- [ ] HTTP/2 client and server (behind `http2` feature flag, using `h2` crate or hand-rolled hpack)
- [x] Streaming body readers (`Content-Length`, chunked, EOF-framed) with size caps
- [x] `Request` builder (method, target, headers, body; host/content-length auto-added)
- [ ] Spec compliance: RFC 7230/7231 (1.1) and RFC 7540 (2) edge cases tested
- [ ] Fuzz HTTP parser with `cargo-fuzz` (still open)
- [ ] Benchmark against `hyper` and `ureq` on throughput and latency

### `crates/tpt-net-ws`
> Pure-Rust WebSocket framing, masking, ping/pong (client + server)

- [x] `cargo new --lib crates/tpt-net-ws`
- [x] WebSocket upgrade handshake (HTTP → WS) — client and server (SHA-1 accept via RustCrypto, known-answer tested)
- [x] Frame parser/serialiser: text, binary, continuation, ping, pong, close (bounds-checked, role-checked masking)
- [x] Masking/unmasking for client frames (RFC 6455 §5.3)
- [x] `WebSocketStream` type wrapping `tpt-async-io` transport (fragmentation assembly, auto-pong, close handshake)
- [x] Ping answered with Pong automatically (timer-driven keepalive scheduling still open)
- [x] Max frame size limit (16 MiB, configurable constant)
- [ ] Per-message deflate extension behind `permessage-deflate` feature flag
- [x] Integration tests: echo server (duplex), control frames, close handshake; fragmentation assembly covered in recv (dedicated fragment test still open)
- [ ] Autobahn test suite pass (run via Docker in CI)

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
- [ ] `.cargo/audit.toml` pinning known-safe advisories

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
