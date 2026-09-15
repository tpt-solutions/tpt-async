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
