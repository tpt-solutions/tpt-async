# AGENTS.md — tpt-async

Runtime-agnostic async I/O + networking stack for Rust: `no_std`-capable from Cortex-M/RISC-V/WASM up to servers. 9-crate Cargo workspace, edition 2021, **MSRV 1.75** (RPITIT required), lock-step version `0.1.0` across all crates (bump all together).

## Commands

```sh
cargo nextest run --workspace --all-features     # tests (CI uses nextest; plain cargo test also works)
cargo test --workspace --all-features --doc      # doc tests
cargo clippy --workspace --all-features -- -D warnings   # alias: cargo check-all
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo deny check                                  # license/advisory/ban audit
cargo check-embedded                              # alias: thumbv7em-none-eabihf (core + timer)
cargo check-riscv                                 # alias: riscv32imac-unknown-none-elf (core + timer)
cargo check-wasm                                  # alias: wasm32-unknown-unknown (core + timer + executor)
```

Tooling: `cargo-nextest`, `cargo-deny`; targets `thumbv7em-none-eabihf`, `riscv32imac-unknown-none-elf`, `wasm32-unknown-unknown`. Shared deps go in `[workspace.dependencies]` in the root `Cargo.toml`, referenced with `workspace = true`.

## Crate layers (dependency order, bottom → top)

| Layer | Crate | Notes |
|---|---|---|
| 0 | `tpt-async-core` | `Spawn`/`LocalSpawn` traits, waker utils. **Must stay `#![no_std]`**; `alloc`/`std` features |
| 0 | `tpt-async-timer` | Heapless const-generic timer wheel. **Must stay `#![no_std]`**; depends only on core |
| 1 | `tpt-async-executor` | Optional cooperative `LocalExecutor`/`block_on`; needs core+alloc. `work-stealing` feature reserved |
| 1 | `tpt-async-io` | `AsyncRead`/`AsyncWrite` traits; tokio/async-std adapters behind `tokio`/`async-std` features |
| 1 | `tpt-async-macros` | `#[tpt_async::main]` proc-macro (trybuild UI tests) |
| 2 | `tpt-net-tls` | rustls 0.23 wrapper, std only |
| 3 | `tpt-net-http` | HTTP/1.1 (default); HTTP/2 behind `http2` (`h2`) |
| 3 | `tpt-net-ws` | WebSocket over tpt-net-http; `permessage-deflate` optional |
| 4 | `tpt-async` | Facade re-exporting `tpt_async::prelude` from the sub-crates |

Do not add upward (left-to-right) dependencies; layers only depend downward. Each public crate exposes a `prelude` module, and the facade re-exports those.

## Hard invariants (CI-enforced)

- **No `async-trait` anywhere** — use RPITIT (`async fn` in traits, stable in 1.75).
- **No OpenSSL** — `deny.toml` bans `openssl`/`openssl-sys`; TLS is rustls 0.23 only, TLS 1.3 by default (`tls12` is opt-in). New deps must pass the `deny.toml` license allow-list (MIT/Apache/BSD/ISC/Zlib/Unicode…).
- Every `unsafe` block needs a `// SAFETY:` comment stating the invariant.
- `#![warn(missing_docs)]` — public API items need doc comments; docs build fails on warnings.
- `clippy -D warnings` and `rustfmt --check` must pass.

## Gotchas

- The facade's `macros` feature is **not default** — `#[tpt_async::main]` requires `tpt-async` with `features = ["macros"]`.
- Dependabot runs weekly (Mondays); `rustls` is pinned to patch bumps only — major/minor rustls updates need manual review.
- Most tests are inline (`#[cfg(test)]`); integration tests currently exist only in `tpt-net-tls/tests/`.
- CI (`.github/workflows/ci.yml`) runs on push to `main` and PRs: test matrix (Linux/Windows × stable/1.75), clippy, fmt, docs, the three cross-target builds, and cargo-deny.

## Conventions

- Conventional commits: `feat:`, `fix:`, `docs:`, `test:`, `chore:`.
- `CHANGELOG.md`: Keep a Changelog format, semver; add entries under `## [Unreleased]`.

## Reference docs (read before touching these areas)

- `spec.txt` — design doc: vision, crate architecture, "secret sauce" (RPITIT, heapless timer wheel, zero-copy HTTP parsing).
- `todo.md` — per-crate development checklist with intended APIs; check here before implementing new features.
- `CONTRIBUTING.md` — contribution invariants summarized above.
