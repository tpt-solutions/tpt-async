# Contributing to tpt-async

Thank you for your interest in contributing!

## Prerequisites

- Rust 1.75+ (MSRV)
- `cargo install cargo-deny cargo-nextest`
- For embedded targets: `rustup target add thumbv7em-none-eabihf riscv32imac-unknown-none-elf wasm32-unknown-unknown`

## Workflow

1. Fork the repository and create a feature branch.
2. Make your changes, keeping the following invariants:
   - `tpt-async-core` and `tpt-async-timer` must remain `#![no_std]`
   - No `async-trait` dependency anywhere — use RPITIT (async fn in traits, Rust 1.75+)
   - Every `unsafe` block must have a `// SAFETY:` comment explaining the invariant
   - `cargo deny check` must pass (no GPL/viral licenses, no OpenSSL)
3. Run the full test suite: `cargo nextest run --workspace`
4. Run the no_std build checks: `cargo check-embedded && cargo check-riscv && cargo check-wasm`
5. Run clippy: `cargo check-all`
6. Open a pull request against `main`.

## Commit style

Use conventional commits: `feat:`, `fix:`, `docs:`, `test:`, `chore:`.
A commit-msg hook enforces this (see below).

## Local automation

- `cargo xtask ci` reproduces the full CI matrix in one command (fmt,
  clippy, tests, docs, cross-target builds, cargo-deny).
- Recommended: install the git hooks to get fmt/clippy + commit-message
  checks on every commit:

  ```sh
  git config core.hooksPath .githooks
  ```

- Open issues with the bug/feature templates; PRs use the PR template and
  should keep `CHANGELOG.md` updated for user-visible changes.

## License

By contributing you agree that your contributions are dual-licensed under MIT OR Apache-2.0, copyright TPT Solutions.
