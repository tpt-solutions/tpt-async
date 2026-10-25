# Changelog

All notable changes to this crate are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); the
crate follows Semantic Versioning and shares the workspace lock-step
version (see the workspace [CHANGELOG](../../CHANGELOG.md) for stack-wide changes).

## [Unreleased]

### Added
- `TlsConnector`/`TlsAcceptor` over rustls 0.23 (TLS 1.3 default)
- `rustls_config`, PEM loaders, certificate pinning, handshake timeouts

### Changed
- MSRV raised from Rust 1.75 to Rust 1.88
- `sha2` dependency moved to 0.11
