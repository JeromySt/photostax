//! Storage backend implementations.
//!
//! This module contains implementations of the [`Repository`] trait for different
//! storage backends. Currently only local filesystem is implemented, with cloud
//! storage backends planned.
//!
//! ## Available Backends
//!
//! - [`local::LocalRepository`] — Local filesystem directory
//!
//! ## Planned Backends
//!
//! - OneDrive — Microsoft cloud storage
//! - Google Drive — Google cloud storage
//!
//! [`Repository`]: crate::repository::Repository

pub mod local;
