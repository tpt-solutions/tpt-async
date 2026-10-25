# Changelog

All notable changes to this crate are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); the
crate follows Semantic Versioning and shares the workspace lock-step
version (see the workspace [CHANGELOG](../../CHANGELOG.md) for stack-wide changes).

## [Unreleased]

### Added
- Const-generic hierarchical timer wheel with `&self` API (thread-shareable)
- `Sleep`, `Timeout`, `Interval` + `MissedTickBehavior`
- std driver thread with `sleep`/`timeout`/`interval` free functions
- `Clock` trait with `StdClock`/`FixedClock`; `fugit` compat feature
- `retry` combinator with `RetryPolicy` (exponential backoff, std)

### Changed
- MSRV raised from Rust 1.75 to Rust 1.88

### Fixed
- An entry inserted with a deadline that had already passed fired only after a full wrap of the level-0 slots (up to `SLOTS - 1` ticks late) instead of on the next tick
- `TimerWheel::len()` and `is_empty()` over-counted entries after each cascade (a deadline past the first level was counted twice per level it moved through). Firing was unaffected
