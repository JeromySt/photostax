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
//! use photostax_core::photo_stack::{ClassifyMode, Metadata, PhotoStack, Rotation, RotationTarget, ScanProgress, ScannerProfile};
//! use photostax_core::file_access::{FileAccess, ReadSeek};
//! use std::io::{self, Write};
//!
//! struct MyCloudRepository {
//!     bucket: String,
//!     location: String,
//!     repo_id: String,
//! }
//!
//! impl FileAccess for MyCloudRepository {
//!     fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
//!         // Open cloud object for reading
//!         todo!()
//!     }
//!
//!     fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
//!         // Open cloud object for writing
//!         todo!()
//!     }
//! }
//!
//! impl Repository for MyCloudRepository {
//!     fn location(&self) -> &str {
//!         &self.location
//!     }
//!
//!     fn id(&self) -> &str {
//!         &self.repo_id
//!     }
//!
//!     fn scan_with_progress(&self, _profile: ScannerProfile, _progress: Option<&mut dyn FnMut(&ScanProgress)>) -> Result<Vec<PhotoStack>, RepositoryError> {
//!         // List objects in cloud bucket, group by naming convention
//!         todo!()
//!     }
//!
//!     fn scan_with_classification(&self, _mode: ClassifyMode) -> Result<Vec<PhotoStack>, RepositoryError> {
//!         self.scan_with_progress(_mode.into(), None)
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
//!     fn read_image(&self, path: &str) -> Result<Box<dyn photostax_core::file_access::ReadSeek>, RepositoryError> {
//!         // Stream image bytes from cloud
//!         todo!()
//!     }
//!
//!     fn write_metadata(&self, stack: &PhotoStack, tags: &Metadata) -> Result<(), RepositoryError> {
//!         // Upload metadata to cloud
//!         todo!()
//!     }
//!
//!     fn rotate_stack(&self, id: &str, rotation: Rotation, target: RotationTarget) -> Result<PhotoStack, RepositoryError> {
//!         // Download, rotate, re-upload
//!         todo!()
//!     }
//! }
//! ```
//!
//! [`backends::local::LocalRepository`]: crate::backends::local::LocalRepository

use crate::events::StackEvent;
use crate::file_access::FileAccess;
use crate::photo_stack::{
    ClassifyMode, Metadata, PhotoStack, Rotation, RotationTarget, ScanProgress, ScannerProfile,
};

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
/// // Each repository has a canonical URI and a short ID
/// println!("Location: {}", repo.location());
/// println!("ID: {}", repo.id());
///
/// // Fast scan — just file paths and folder metadata, no file I/O
/// let stacks = repo.scan()?;
/// println!("Found {} stacks", stacks.len());
///
/// // Load metadata only when needed
/// let mut stack = stacks.into_iter().next().unwrap();
/// repo.load_metadata(&mut stack)?;
/// println!("{} (id={}): {} EXIF tags", stack.name, stack.id, stack.metadata.exif_tags.len());
/// # Ok::<(), photostax_core::repository::RepositoryError>(())
/// ```
///
/// [`backends::local::LocalRepository`]: crate::backends::local::LocalRepository
pub trait Repository: FileAccess {
    /// Returns the canonical URI of this repository.
    ///
    /// For local repositories this is a `file:///` URI derived from the
    /// canonicalized root path. Cloud backends would return their own scheme
    /// (e.g., `azure://account/container`).
    ///
    /// The location is used to generate deterministic, opaque stack IDs via
    /// [`make_stack_id`](crate::hashing::make_stack_id).
    fn location(&self) -> &str;

    /// Returns a short, deterministic identifier derived from the location.
    ///
    /// Useful as a cache key or database partition key. The value is a
    /// truncated SHA-256 hex string (16 characters).
    fn id(&self) -> &str;

    /// Scan with a [`ScannerProfile`] and optional progress callback.
    ///
    /// This is the primary scan method. It performs a two-pass scan:
    ///
    /// 1. **Pass 1 (fast)**: Directory scan — discovers files, groups them into
    ///    stacks, and applies folder metadata. Progress callbacks report stacks
    ///    discovered.
    /// 2. **Pass 2 (Auto only)**: Classification — analyses ambiguous `_a` images
    ///    using pixel variance. Skipped when the profile is not [`ScannerProfile::Auto`].
    ///    Progress callbacks report per-stack classification progress.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the repository location cannot be accessed.
    fn scan_with_progress(
        &self,
        profile: ScannerProfile,
        progress: Option<&mut dyn FnMut(&ScanProgress)>,
    ) -> Result<Vec<PhotoStack>, RepositoryError>;

    /// Scan the repository with the given classification mode.
    ///
    /// Convenience wrapper around [`scan_with_progress`](Self::scan_with_progress)
    /// with no progress callback.
    fn scan_with_classification(
        &self,
        mode: ClassifyMode,
    ) -> Result<Vec<PhotoStack>, RepositoryError> {
        self.scan_with_progress(mode.into(), None)
    }

    /// Scan the repository and return all discovered photo stacks.
    ///
    /// This is equivalent to calling
    /// [`scan_with_progress(ScannerProfile::Auto, None)`](Self::scan_with_progress).
    /// Ambiguous `_a` images are automatically classified as enhanced or back
    /// using pixel analysis.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the repository location cannot be accessed.
    fn scan(&self) -> Result<Vec<PhotoStack>, RepositoryError> {
        self.scan_with_progress(ScannerProfile::Auto, None)
    }

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

    /// Read an image file as a seekable stream.
    ///
    /// The path should be one of the paths from a [`PhotoStack`] (original, enhanced, or back).
    /// Returns a boxed stream that supports both reading and seeking.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the file cannot be read.
    fn read_image(
        &self,
        path: &str,
    ) -> Result<Box<dyn crate::file_access::ReadSeek>, RepositoryError>;

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

    /// Rotate images in a photo stack by the given angle.
    ///
    /// The `target` parameter controls which images are rotated:
    ///
    /// | Target | Images rotated |
    /// |--------|----------------|
    /// | [`All`](RotationTarget::All) | original + enhanced + back |
    /// | [`Front`](RotationTarget::Front) | original + enhanced only |
    /// | [`Back`](RotationTarget::Back) | back only |
    ///
    /// After rotation the stack is returned with refreshed metadata so
    /// callers can immediately use the updated state.
    ///
    /// # Supported Formats
    ///
    /// | Format | Behaviour |
    /// |--------|-----------|
    /// | JPEG | Decoded → rotated → re-encoded (lossy) |
    /// | TIFF | Decoded → rotated → re-encoded |
    ///
    /// # Errors
    ///
    /// - [`RepositoryError::NotFound`] if the stack ID does not exist
    /// - [`RepositoryError::Io`] if any image file cannot be read or written
    /// - [`RepositoryError::Other`] if an image cannot be decoded
    fn rotate_stack(
        &self,
        id: &str,
        rotation: Rotation,
        target: RotationTarget,
    ) -> Result<PhotoStack, RepositoryError>;

    /// Start watching for file changes. Returns a receiver for StackEvents.
    /// Default implementation returns a receiver that never produces events.
    ///
    /// The watcher runs in a background thread. Drop the receiver to stop watching.
    fn watch(&self) -> Result<std::sync::mpsc::Receiver<StackEvent>, RepositoryError> {
        let (_tx, rx) = std::sync::mpsc::channel();
        Ok(rx)
    }
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

    #[test]
    fn test_default_watch_returns_receiver() {
        use crate::backends::local::LocalRepository;
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());
        let rx = repo.watch();
        assert!(rx.is_ok());
    }
}
