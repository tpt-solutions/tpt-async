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
//! | `macros`   | no      | re-export of `#[tpt_async::main]` |
//!
//! # Example
//!
//! ```rust,ignore
//! use tpt_async::prelude::*;
//! use tpt_net_http::prelude::*;
//!
//! #[tpt_async::main]
//! async fn main() {
//!     let client = HttpClient::builder()
//!         .tls(tpt_net_tls::rustls_config())
//!         .build();
//!
//!     let response = client
//!         .get("https://api.tpt.solutions/health")
//!         .timeout(Duration::from_millis(500))
//!         .send()
//!         .await
//!         .unwrap();
//!
//!     println!("Status: {}", response.status());
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs, clippy::all)]

// Re-export the proc-macro so `#[tpt_async::main]` resolves from this crate.
#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub use tpt_async_macros::main;

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

    // Duration re-export for convenience (std path)
    #[cfg(feature = "std")]
    pub use core::time::Duration;
}
