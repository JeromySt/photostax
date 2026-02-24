//! Storage backend abstraction for photo repositories.
//!
//! This module defines the [`Repository`] trait that abstracts over different
//! storage backends. The trait provides a common interface for scanning, reading,
//! and writing photo stacks regardless of where they are stored.
//!
//! ## Backend Pattern
//!
//! The `Repository` trait enables a plugin architecture:
//!
//! - [`backends::local::LocalRepository`] — Local filesystem (implemented)
//! - OneDrive, Google Drive, etc. — Cloud storage (planned)
//!
//! ## Example: Custom Backend
//!
//! Implementing a custom repository backend:
//!
//! ```rust,no_run
//! use photostax_core::repository::{Repository, RepositoryError};
//! use photostax_core::photo_stack::{Metadata, PhotoStack};
//! use std::path::Path;
//!
//! struct MyCloudRepository {
//!     bucket: String,
//! }
//!
//! impl Repository for MyCloudRepository {
//!     fn scan(&self) -> Result<Vec<PhotoStack>, RepositoryError> {
//!         // List objects in cloud bucket, group by naming convention
//!         todo!()
//!     }
//!
//!     fn get_stack(&self, id: &str) -> Result<PhotoStack, RepositoryError> {
//!         // Fetch specific stack by ID
//!         todo!()
//!     }
//!
//!     fn read_image(&self, path: &Path) -> Result<Vec<u8>, RepositoryError> {
//!         // Download image bytes from cloud
//!         todo!()
//!     }
//!
//!     fn write_metadata(&self, stack: &PhotoStack, tags: &Metadata) -> Result<(), RepositoryError> {
//!         // Upload metadata to cloud
//!         todo!()
//!     }
//! }
//! ```
//!
//! [`backends::local::LocalRepository`]: crate::backends::local::LocalRepository

use std::path::Path;

use crate::photo_stack::{Metadata, PhotoStack};

/// Errors that can occur when interacting with a photo repository.
///
/// # Variants
///
/// | Variant | When It Occurs |
/// |---------|----------------|
/// | [`Io`](Self::Io) | File system operations fail (permissions, disk full, etc.) |
/// | [`NotFound`](Self::NotFound) | Requested stack ID doesn't exist in the repository |
/// | [`Other`](Self::Other) | Backend-specific errors (cloud auth, network, etc.) |
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// An I/O error occurred while accessing the repository.
    ///
    /// This wraps standard [`std::io::Error`] for filesystem operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The requested photo stack was not found.
    ///
    /// The contained string is the stack ID that was requested.
    #[error("Photo stack not found: {0}")]
    NotFound(String),

    /// A backend-specific error occurred.
    ///
    /// Used for errors that don't fit other categories, such as authentication
    /// failures, network timeouts, or serialization errors.
    #[error("{0}")]
    Other(String),
}

/// Abstraction over a storage backend containing Epson FastFoto photo stacks.
///
/// Implementations exist for local filesystem access ([`backends::local::LocalRepository`]),
/// with cloud storage backends (OneDrive, Google Drive) planned for future releases.
///
/// # Thread Safety
///
/// Implementations should be safe to use from multiple threads. The local filesystem
/// implementation uses atomic operations where possible.
///
/// # Examples
///
/// Using a repository to scan and process photos:
///
/// ```rust,no_run
/// use photostax_core::backends::local::LocalRepository;
/// use photostax_core::repository::Repository;
///
/// let repo = LocalRepository::new("/photos");
///
/// // Scan all stacks
/// for stack in repo.scan()? {
///     println!("Found: {} ({} EXIF tags)",
///         stack.id,
///         stack.metadata.exif_tags.len());
/// }
/// # Ok::<(), photostax_core::repository::RepositoryError>(())
/// ```
///
/// [`backends::local::LocalRepository`]: crate::backends::local::LocalRepository
pub trait Repository {
    /// Scan the repository and return all discovered photo stacks.
    ///
    /// Scans the repository root, groups files by the FastFoto naming convention
    /// (`_a` for enhanced, `_b` for back), and returns enriched [`PhotoStack`] objects
    /// with merged metadata from EXIF, XMP, and sidecar sources.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the repository location cannot be accessed.
    fn scan(&self) -> Result<Vec<PhotoStack>, RepositoryError>;

    /// Retrieve a single photo stack by its ID.
    ///
    /// The ID is the base filename without the `_a`/`_b` suffix or extension.
    ///
    /// # Errors
    ///
    /// - [`RepositoryError::NotFound`] if no stack with the given ID exists
    /// - [`RepositoryError::Io`] if the repository cannot be accessed
    fn get_stack(&self, id: &str) -> Result<PhotoStack, RepositoryError>;

    /// Read the raw bytes of an image file within the repository.
    ///
    /// The path should be one of the paths from a [`PhotoStack`] (original, enhanced, or back).
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the file cannot be read.
    fn read_image(&self, path: &Path) -> Result<Vec<u8>, RepositoryError>;

    /// Write metadata tags to the files in a photo stack.
    ///
    /// The behavior depends on the metadata type:
    ///
    /// - **XMP tags**: Written directly to image files (JPEG) or sidecar `.xmp` files (TIFF)
    /// - **Custom tags**: Stored in the sidecar SQLite database
    /// - **EXIF tags**: Stored in sidecar database (direct EXIF writing is avoided for safety)
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Other`] if metadata cannot be written.
    fn write_metadata(&self, stack: &PhotoStack, tags: &Metadata) -> Result<(), RepositoryError>;
}
