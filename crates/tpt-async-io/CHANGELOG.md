# Changelog

All notable changes to this crate are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); the
crate follows Semantic Versioning and shares the workspace lock-step
version (see the workspace [CHANGELOG](../../CHANGELOG.md) for stack-wide changes).

## [Unreleased]

### Added
- `AsyncRead`/`AsyncWrite`/`AsyncBufRead` traits + `ReadBuf`
- `AsyncReadExt`/`AsyncWriteExt`, `AsyncWriteVectored`
- Adapters: std, tokio (reader/writer/compat), async-std, embedded-io
