//! **tpt-async** — the runtime-agnostic async I/O facade.
//!
//! This crate re-exports the essential items from all `tpt-async` sub-crates
//! under a single `prelude` so users only need one `use` statement.
//!
//! # Feature flags
//!
//! | Flag       | Default | What it enables |
//! |------------|---------|-----------------|
//! | `std`      | yes     | std-dependent features in sub-crates |
//! | `alloc`    | yes     | `JoinHandle`, `Completer` |
//! | `executor` | yes     | `LocalExecutor`, `block_on` |
//! | `timer`    | yes     | `Sleep`, `Interval`, `Timeout`, `TimerWheel` |
//! | `macros`   | no      | `#[tpt_async::main]` (implies `executor`) |
//!
//! # Example
//!
//! ```rust,no_run
//! use tpt_async::prelude::*;
//!
//! let executor = LocalExecutor::new();
//! executor.block_on(async {
//!     // The timer crate's std driver makes sleep/timeout just work:
//!     sleep(core::time::Duration::from_millis(10)).await;
//!     println!("hello from tpt-async");
//! });
//! ```
//!
//! With `features = ["macros"]`, `#[tpt_async::main]` wraps an `async fn
//! main` in exactly this executor for you.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs, clippy::all)]

// Re-export the proc-macro so `#[tpt_async::main]` resolves from this crate.
#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub use tpt_async_macros::main;

/// Implementation detail of `#[tpt_async::main]`.
///
/// The proc-macro expands to this module's paths so that user crates only
/// need a dependency on the facade (which internally owns the executor).
/// Semver-exempt: anything in here can change between releases.
#[cfg(feature = "executor")]
#[doc(hidden)]
pub mod __private {
    pub use tpt_async_executor::LocalExecutor;
}

pub mod prelude {
    //! The `tpt-async` prelude.
    //!
    //! ```rust
    //! use tpt_async::prelude::*;
    //! ```

    // Core traits + error types
    pub use tpt_async_core::prelude::*;

    // Optional executor
    #[cfg(feature = "executor")]
    #[cfg_attr(docsrs, doc(cfg(feature = "executor")))]
    pub use tpt_async_executor::LocalExecutor;

    // Optional timer
    #[cfg(feature = "timer")]
    #[cfg_attr(docsrs, doc(cfg(feature = "timer")))]
    pub use tpt_async_timer::prelude::*;

    // `Duration` lives in core; useful everywhere.
    pub use core::time::Duration;
}
