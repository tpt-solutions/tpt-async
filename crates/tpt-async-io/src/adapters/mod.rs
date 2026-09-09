// Copyright TPT Solutions. Licensed under MIT OR Apache-2.0.

#[cfg(feature = "std")]
pub mod std_compat;

#[cfg(feature = "tokio")]
pub mod tokio_compat;
