# Changelog

All notable changes to this crate are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); the
crate follows Semantic Versioning and shares the workspace lock-step
version (see the workspace [CHANGELOG](../../CHANGELOG.md) for stack-wide changes).

## [Unreleased]

### Added
- Zero-copy HTTP/1.1 parser; `ClientConnection`/`Request`/`Response`
- Keep-alive server (`serve_connection` + `Handler` trait)
- Connection pooling, redirect following, HTTP/2 via `h2` (`http2` feature)
