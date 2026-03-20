//! Storage backend implementations.
//!
//! This module contains implementations of the [`Repository`] trait for different
//! storage backends.
//!
//! ## Available Backends
//!
//! - [`local::LocalRepository`] — Local filesystem directory
//! - [`foreign::ForeignRepository`] — Host-language-provided I/O (OneDrive, Google Drive, etc.)
//!
//! [`Repository`]: crate::repository::Repository

pub mod foreign;
pub mod local;
pub mod local_handles;
