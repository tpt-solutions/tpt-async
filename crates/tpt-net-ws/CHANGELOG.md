# Changelog

All notable changes to this crate are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/); the
crate follows Semantic Versioning and shares the workspace lock-step
version (see the workspace [CHANGELOG](../../CHANGELOG.md) for stack-wide changes).

## [Unreleased]

### Added
- RFC 6455 `WebSocketStream`: handshake, frames, fragmentation, ping/pong, close
- Timer-driven keepalive: `WebSocketStream::set_keepalive(Some(KeepAlive { interval, timeout }))`; `recv` fails with `WsError::TimedOut` if a peer stays silent after a keepalive `Ping`
- `permessage-deflate` compression (feature-gated)
