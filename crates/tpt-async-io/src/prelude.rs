// Copyright TPT Solutions. Licensed under MIT OR Apache-2.0.

//! Convenience re-exports for the most commonly used items.
//!
//! ```rust
//! use tpt_async_io::prelude::*;
//! ```

pub use crate::buf::AsyncBufRead;
pub use crate::read::{AsyncRead, AsyncReadExt, IoError};
pub use crate::read_buf::ReadBuf;
#[cfg(feature = "std")]
pub use crate::write::AsyncWriteVectored;
pub use crate::write::{AsyncWrite, AsyncWriteExt};
