// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Error type for `tpt-net-http`.

use core::fmt;

use tpt_async_io::IoError;

/// All errors that can occur in this crate.
#[derive(Debug)]
pub enum HttpError {
    /// Transport-level I/O error.
    Io(IoError),
    /// The peer sent a malformed HTTP message.  The payload names the reason.
    Parse(&'static str),
    /// The peer closed the transport in the middle of a message.
    ConnectionClosed,
    /// The request exceeded its configured timeout.
    TimedOut,
    /// A header value was present twice where a single value is required
    /// (e.g. two different `Content-Length` values — request smuggling
    /// vector, always rejected).
    ConflictingHeaders(&'static str),
    /// An HTTP/2 protocol error from the `h2` crate (feature `http2`).
    #[cfg(feature = "http2")]
    H2(h2::Error),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Io(e) => write!(f, "I/O error: {e}"),
            HttpError::Parse(reason) => write!(f, "malformed HTTP message: {reason}"),
            HttpError::ConnectionClosed => write!(f, "connection closed by peer"),
            HttpError::TimedOut => write!(f, "operation timed out"),
            HttpError::ConflictingHeaders(what) => {
                write!(f, "conflicting headers: {what}")
            }
            #[cfg(feature = "http2")]
            HttpError::H2(e) => write!(f, "HTTP/2 error: {e}"),
        }
    }
}

impl std::error::Error for HttpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HttpError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<IoError> for HttpError {
    fn from(e: IoError) -> Self {
        HttpError::Io(e)
    }
}

impl From<tpt_async_timer::timeout::TimedOut> for HttpError {
    fn from(_: tpt_async_timer::timeout::TimedOut) -> Self {
        HttpError::TimedOut
    }
}

#[cfg(feature = "http2")]
impl From<h2::Error> for HttpError {
    fn from(e: h2::Error) -> Self {
        HttpError::H2(e)
    }
}
