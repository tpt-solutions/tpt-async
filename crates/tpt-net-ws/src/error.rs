// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Error type for `tpt-net-ws`.

use core::fmt;

use tpt_async_io::IoError;

/// All errors that can occur in this crate.
#[derive(Debug)]
pub enum WsError {
    /// Transport-level I/O error.
    Io(IoError),
    /// An RFC 6455 protocol violation by the peer.
    Protocol(&'static str),
    /// The peer closed the transport mid-message.
    ConnectionClosed,
    /// The HTTP upgrade handshake failed.
    Handshake(&'static str),
    /// An operation exceeded its configured timeout.
    TimedOut,
}

impl fmt::Display for WsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WsError::Io(e) => write!(f, "I/O error: {e}"),
            WsError::Protocol(why) => write!(f, "WebSocket protocol violation: {why}"),
            WsError::ConnectionClosed => write!(f, "connection closed by peer"),
            WsError::Handshake(why) => write!(f, "handshake failed: {why}"),
            WsError::TimedOut => write!(f, "operation timed out"),
        }
    }
}

impl std::error::Error for WsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WsError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<IoError> for WsError {
    fn from(e: IoError) -> Self {
        WsError::Io(e)
    }
}

impl From<tpt_net_http::HttpError> for WsError {
    fn from(e: tpt_net_http::HttpError) -> Self {
        match e {
            tpt_net_http::HttpError::Io(io) => WsError::Io(io),
            tpt_net_http::HttpError::ConnectionClosed => WsError::ConnectionClosed,
            tpt_net_http::HttpError::TimedOut => WsError::TimedOut,
            other => WsError::Handshake(Box::leak(format!("{other}").into_boxed_str())),
        }
    }
}
