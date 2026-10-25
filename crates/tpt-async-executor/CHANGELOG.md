# Changelog

All notable changes to this crate are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); the
crate follows Semantic Versioning and shares the workspace lock-step
version (see the workspace [CHANGELOG](../../CHANGELOG.md) for stack-wide changes).

## [Unreleased]

### Added
- `LocalExecutor` with thread-safe wakers and condvar parking
- `spawn_task`/`spawn_task_local` (cancel-on-drop `Task`)
- Optional `tracing` feature: a span per task poll and a trace event per spawn

### Changed
- MSRV raised from Rust 1.75 to Rust 1.88
