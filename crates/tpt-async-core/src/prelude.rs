//! The `tpt-async-core` prelude.
//!
//! ```rust
//! use tpt_async_core::prelude::*;
//! ```

pub use crate::error::{Cancelled, SpawnError};
pub use crate::spawn::{LocalSpawn, Spawn};
pub use crate::waker::{noop_context, noop_waker, waker_fn};

#[cfg(feature = "alloc")]
pub use crate::task::{Completer, JoinHandle};
