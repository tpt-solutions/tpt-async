# Changelog

All notable changes to this crate are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); the
crate follows Semantic Versioning and shares the workspace lock-step
version (see the workspace [CHANGELOG](../../CHANGELOG.md) for stack-wide changes).

## [Unreleased]

### Added
- `Spawn`/`LocalSpawn` traits; `JoinHandle`, `Completer`, `Task` (cancel-on-drop)
- Waker utilities: `noop_waker`, `waker_fn`, closure-based `waker_with`
- `state_machine!` zero-cost state-machine generator

### Changed
- MSRV raised from Rust 1.75 to Rust 1.88

### Fixed
- `spawn` module imports are gated behind the `alloc` feature, so builds without `alloc` no longer carry unused imports
