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
- `router::Router`: exact `(method, path)` routing with a `405` for a known path under another method, a `404` default, and `fallback` to replace it. Exposed in the prelude

### Changed
- MSRV raised from Rust 1.75 to Rust 1.88

### Fixed
- Keep-alive is preserved when a request has `Content-Length: 0` and the handler does not read the body. Our client sends that header on every GET, so previously each such request closed the connection
- Obsolete line folding is detected only within the header block, so a body that contains a line starting with whitespace is no longer rejected
