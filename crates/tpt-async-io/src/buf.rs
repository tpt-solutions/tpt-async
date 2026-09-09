// Copyright TPT Solutions. Licensed under MIT OR Apache-2.0.

use core::pin::Pin;
use core::task::{Context, Poll};

use crate::read::AsyncRead;
use crate::IoError;

/// Async counterpart of `std::io::BufRead`.
///
/// Types implementing this trait maintain an internal byte buffer and expose
/// it to callers without an extra copy, enabling zero-copy parsing workflows.
pub trait AsyncBufRead: AsyncRead {
    /// Attempt to fill the internal buffer, returning a view of the available
    /// bytes.
    ///
    /// The returned slice is valid until the next call to [`consume`] or any
    /// mutating operation.  Returning an empty slice signals EOF.
    fn poll_fill_buf(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<&[u8], IoError>>;

    /// Mark `amt` bytes as consumed, advancing the internal cursor.
    ///
    /// `amt` must not exceed the length of the slice returned by the most
    /// recent successful [`poll_fill_buf`] call.
    fn consume(self: Pin<&mut Self>, amt: usize);
}
