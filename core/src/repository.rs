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
//!     fn load_metadata(&self, _stack: &mut PhotoStack) -> Result<(), RepositoryError> {
//!         // Fetch EXIF/XMP from cloud storage
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
/// // Fast scan — just file paths and folder metadata, no file I/O
/// let stacks = repo.scan()?;
/// println!("Found {} stacks", stacks.len());
///
/// // Load metadata only when needed
/// let mut stack = repo.get_stack("IMG_0001")?;
/// repo.load_metadata(&mut stack)?;
/// println!("{}: {} EXIF tags", stack.id, stack.metadata.exif_tags.len());
/// # Ok::<(), photostax_core::repository::RepositoryError>(())
/// ```
///
/// [`backends::local::LocalRepository`]: crate::backends::local::LocalRepository
pub trait Repository {
    /// Scan the repository and return all discovered photo stacks.
    ///
    /// Returns lightweight [`PhotoStack`] objects with file paths and
    /// folder-derived metadata only. **No file content I/O** is performed
    /// beyond the directory listing — EXIF, XMP, and sidecar data are not
    /// loaded. Call [`load_metadata`](Self::load_metadata) on individual
    /// stacks when you need their full metadata.
    ///
    /// This design makes scanning O(1) per file (just `read_dir`) regardless
    /// of how many stacks exist, which is ideal for pagination and counting.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the repository location cannot be accessed.
    fn scan(&self) -> Result<Vec<PhotoStack>, RepositoryError>;

    /// Load EXIF, XMP, and sidecar metadata into an existing photo stack.
    ///
    /// Populates the stack's [`Metadata`] by reading:
    /// 1. EXIF tags from the enhanced (preferred) or original image file
    /// 2. Embedded XMP from JPEG files
    /// 3. XMP sidecar file (`.xmp`) — highest priority, overrides above
    /// 4. Folder name metadata — lowest priority, fills gaps
    ///
    /// This is the lazy counterpart to scanning: call it only when you need
    /// a stack's full metadata (e.g., for display, search, or export).
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if metadata files cannot be read.
    fn load_metadata(&self, stack: &mut PhotoStack) -> Result<(), RepositoryError>;

    /// Retrieve a single photo stack by its ID.
    ///
    /// The ID is the base filename without the `_a`/`_b` suffix or extension.
    /// Returns a lightweight stack without file-based metadata loaded.
    /// Call [`load_metadata`](Self::load_metadata) to populate EXIF/XMP/sidecar data.
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
    /// - **Custom tags**: Stored in the XMP sidecar file (`.xmp`)
    /// - **EXIF tags**: Stored as overrides in the XMP sidecar file (direct EXIF writing is avoided for safety)
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Other`] if metadata cannot be written.
    fn write_metadata(&self, stack: &PhotoStack, tags: &Metadata) -> Result<(), RepositoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_error_io_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let repo_err = RepositoryError::Io(io_err);
        let display = format!("{}", repo_err);
        assert!(display.contains("I/O error"));
        assert!(display.contains("file not found"));
    }

    #[test]
    fn test_repository_error_not_found_display() {
        let err = RepositoryError::NotFound("IMG_001".to_string());
        let display = format!("{}", err);
        assert!(display.contains("Photo stack not found"));
        assert!(display.contains("IMG_001"));
    }

    #[test]
    fn test_repository_error_other_display() {
        let err = RepositoryError::Other("something went wrong".to_string());
        let display = format!("{}", err);
        assert!(display.contains("something went wrong"));
    }

    #[test]
    fn test_repository_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let repo_err: RepositoryError = io_err.into();

        match repo_err {
            RepositoryError::Io(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
            }
            _ => panic!("Expected Io variant"),
        }
    }

    #[test]
    fn test_repository_error_debug() {
        let err = RepositoryError::NotFound("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotFound"));
    }
}
