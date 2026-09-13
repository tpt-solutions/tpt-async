//! **tpt-async** — the runtime-agnostic async I/O facade.
//!
//! This crate re-exports the essential items from all `tpt-async` sub-crates
//! under a single `prelude` so users only need one `use` statement.
//!
//! # Feature flags
//!
//! | Flag         | Default | What it enables |
//! |--------------|---------|-----------------|
//! | `std`        | yes     | std-dependent features in sub-crates |
//! | `alloc`      | yes     | `JoinHandle`, `Completer` |
//! | `executor`   | yes     | `LocalExecutor`, `block_on` |
//! | `timer`      | yes     | `Sleep`, `Interval`, `Timeout`, `TimerWheel`, `sleep()` |
//! | `macros`     | no      | `#[tpt_async::main]` (implies `executor`) |
//! | `io`         | no      | `AsyncRead`/`AsyncWrite` traits, ext helpers, adapters |
//! | `tls`        | no      | `TlsConnector`/`TlsAcceptor` (implies `io`) |
//! | `spawn-tokio`| no      | `impl Spawn for tokio::runtime::Handle` |
//! | `spawn-smol` | no      | `spawn_on_smol()` helper for `smol::Executor` |
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

// The std timer driver's free functions are the headline API — put them at
// the crate root, not just in the prelude.
#[cfg(all(feature = "timer", feature = "std"))]
#[cfg_attr(docsrs, doc(cfg(feature = "timer")))]
pub use tpt_async_timer::driver::{interval, sleep, timeout};

/// Ready-made `Spawn` adapters for popular runtimes.
///
/// "Bring your own executor" becomes one feature flag instead of a manual
/// trait implementation.
#[cfg(any(feature = "spawn-tokio", feature = "spawn-smol"))]
pub mod runtime {
    /// Spawn on a tokio runtime through the unified [`Spawn`](tpt_async_core::spawn::Spawn)
    /// trait.
    ///
    /// The [`TokioHandle`](tokio_support::TokioHandle) newtype exists because neither the trait nor
    /// `tokio::runtime::Handle` is defined in this crate (orphan rule); it
    /// dereferences to the inner handle, so it can be used transparently.
    #[cfg(feature = "spawn-tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "spawn-tokio")))]
    pub mod tokio_support {
        use tpt_async_core::error::SpawnError;
        use tpt_async_core::spawn::Spawn;
        use tpt_async_core::task::{Completer, JoinHandle};

        /// Wrapper around [`tokio::runtime::Handle`] implementing the
        /// unified `Spawn` trait.
        #[derive(Debug, Clone)]
        pub struct TokioHandle(pub tokio::runtime::Handle);

        impl std::ops::Deref for TokioHandle {
            type Target = tokio::runtime::Handle;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl Spawn for TokioHandle {
            fn spawn<F>(&self, future: F) -> Result<JoinHandle<F::Output>, SpawnError>
            where
                F: std::future::Future + Send + 'static,
                F::Output: Send + 'static,
            {
                let (join, completer) = Completer::new();
                tokio::task::spawn(async move {
                    completer.complete(future.await);
                });
                Ok(join)
            }
        }

        /// Spawn a future on the tokio runtime whose context we are currently
        /// in, returning a `tpt-async` [`JoinHandle`].
        ///
        /// # Panics
        /// Panics when called outside a tokio runtime, mirroring
        /// [`tokio::spawn`].
        pub fn spawn<F>(future: F) -> Result<JoinHandle<F::Output>, SpawnError>
        where
            F: std::future::Future + Send + 'static,
            F::Output: Send + 'static,
        {
            let handle =
                tokio::runtime::Handle::try_current().expect("spawn(): not inside a tokio runtime");
            Spawn::spawn(&TokioHandle(handle), future)
        }
    }

    /// Spawn a future on a `smol::Executor`, returning a `tpt-async`
    /// [`JoinHandle`](tpt_async_core::task::JoinHandle).
    #[cfg(feature = "spawn-smol")]
    #[cfg_attr(docsrs, doc(cfg(feature = "spawn-smol")))]
    pub fn spawn_on_smol<F>(
        executor: &smol::Executor<'static>,
        future: F,
    ) -> Result<tpt_async_core::task::JoinHandle<F::Output>, tpt_async_core::error::SpawnError>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        use tpt_async_core::task::Completer;

        let (join, completer) = Completer::new();
        executor
            .spawn(async move {
                completer.complete(future.await);
            })
            .detach();
        Ok(join)
    }
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

    // Optional async I/O traits + extension helpers
    #[cfg(feature = "io")]
    #[cfg_attr(docsrs, doc(cfg(feature = "io")))]
    pub use tpt_async_io::prelude::*;

    // Optional TLS
    #[cfg(feature = "tls")]
    #[cfg_attr(docsrs, doc(cfg(feature = "tls")))]
    pub use tpt_net_tls::{
        load_pem_certs, load_pem_key, rustls_config, server_config, TlsAcceptor, TlsConnector,
        TlsError, TlsStream,
    };

    // `Duration` lives in core; useful everywhere.
    pub use core::time::Duration;
}
