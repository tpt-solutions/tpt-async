# Changelog

All notable changes to this crate are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); the
crate follows Semantic Versioning and shares the workspace lock-step
version (see the workspace [CHANGELOG](../../CHANGELOG.md) for stack-wide changes).

## [Unreleased]

### Added
- RFC 6455 `WebSocketStream`: handshake, frames, fragmentation, ping/pong, close
- Timer-driven keepalive: `WebSocketStream::set_keepalive(Some(KeepAlive { interval, timeout }))`; `recv` fails with `WsError::TimedOut` if a peer stays silent after a keepalive `Ping`
- `permessage-deflate` compression (feature-gated) with context takeover: the codec keeps compressor and decompressor state across messages when the peer keeps it. Our server commits to `no_context_takeover` in both directions

### Changed
- MSRV raised from Rust 1.75 to Rust 1.88

### Fixed
- Protocol violations are answered with a Close frame (1002, or 1007 for invalid UTF-8) before the error is returned, per RFC 6455 §7.1.7
- Rejects fragmented control frames, one-byte Close payloads, invalid Close status codes, and invalid UTF-8 Close reasons
- `permessage-deflate` finishes the sync flush before a message is returned, so the peer no longer decodes extra bytes (Autobahn 12.2.9 and 12.2.14)
