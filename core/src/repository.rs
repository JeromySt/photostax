//! Storage backend abstraction for photo repositories.
//!
//! This module defines the [`Repository`] trait that abstracts over different
//! storage backends. The trait provides a common interface for scanning photo
//! stacks regardless of where they are stored. Per-stack I/O (reading images,
//! loading/writing metadata, rotation) is handled by [`ImageRef`] and
//! [`MetadataRef`] handles embedded in each [`PhotoStack`].
//!
//! ## Backend Pattern
//!
//! The `Repository` trait enables a plugin architecture:
//!
//! - [`backends::local::LocalRepository`] — Local filesystem (implemented)
//! - [`backends::foreign::ForeignRepository`] — Host-language-provided I/O
//! - OneDrive, Google Drive, etc. — Cloud storage (planned)
//!
//! ## Example: Custom Backend
//!
//! Implementing a custom repository backend:
//!
//! ```rust,no_run
//! use photostax_core::repository::{Repository, RepositoryError};
//! use photostax_core::photo_stack::{PhotoStack, ScanProgress, ScannerProfile};
//! use photostax_core::classifier::ImageClassifier;
//! use photostax_core::events::RepoEvent;
//! use photostax_core::file_access::{FileAccess, ReadSeek};
//! use std::sync::Arc;
//! use std::io::{self, Write};
//!
//! struct MyCloudRepository {
//!     bucket: String,
//!     location: String,
//!     repo_id: String,
//!     generation: std::sync::atomic::AtomicU64,
//!     classifier: Option<Arc<dyn ImageClassifier>>,
//! }
//!
//! impl FileAccess for MyCloudRepository {
//!     fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
//!         todo!()
//!     }
//!     fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
//!         todo!()
//!     }
//! }
//!
//! impl Repository for MyCloudRepository {
//!     fn location(&self) -> &str { &self.location }
//!     fn id(&self) -> &str { &self.repo_id }
//!
//!     fn scan_with_progress(&self, _profile: ScannerProfile, _progress: Option<&mut dyn FnMut(&ScanProgress)>) -> Result<Vec<PhotoStack>, RepositoryError> {
//!         todo!()
//!     }
//!
//!     fn generation(&self) -> u64 {
//!         self.generation.load(std::sync::atomic::Ordering::Acquire)
//!     }
//!
//!     fn set_classifier(&mut self, classifier: Arc<dyn ImageClassifier>) {
//!         self.classifier = Some(classifier);
//!     }
//! }
//! ```
//!
//! [`backends::local::LocalRepository`]: crate::backends::local::LocalRepository
//! [`ImageRef`]: crate::image_handle::ImageRef
//! [`MetadataRef`]: crate::metadata_handle::MetadataRef
//! [`PhotoStack`]: crate::photo_stack::PhotoStack

use std::sync::Arc;

use crate::classifier::ImageClassifier;
use crate::events::{RepoEvent, StackEvent};
use crate::file_access::FileAccess;
use crate::photo_stack::{PhotoStack, ScanProgress, ScannerProfile};

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

    /// The photo stack has been deleted and its handles are no longer valid.
    ///
    /// Returned when an operation is attempted on an [`ImageRef`](crate::image_handle::ImageRef)
    /// or [`MetadataRef`](crate::metadata_handle::MetadataRef) whose backing
    /// handle has been invalidated (e.g., after the file was deleted from disk).
    #[error("photo stack has been deleted")]
    StackDeleted,

    /// The operation was cancelled via a [`CancellationToken`](tokio_util::sync::CancellationToken).
    #[error("operation cancelled")]
    Cancelled,

    /// The repository is read-only and does not support write operations.
    ///
    /// Returned when a mutation (rotate, delete, metadata write, swap) is
    /// attempted on a stack from a read-only repository.
    #[error("repository is read-only: {0}")]
    ReadOnly(String),
}

/// Abstraction over a storage backend containing Epson FastFoto photo stacks.
///
/// Implementations exist for local filesystem access ([`backends::local::LocalRepository`]),
/// with cloud storage backends (OneDrive, Google Drive) planned for future releases.
///
/// Per-stack I/O (reading images, loading/writing metadata, rotation) is handled
/// by the [`ImageRef`](crate::image_handle::ImageRef) and
/// [`MetadataRef`](crate::metadata_handle::MetadataRef) handles embedded in each
/// [`PhotoStack`].
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
/// // Metadata is loaded lazily via the handle
/// let stack = stacks.into_iter().next().unwrap();
/// let meta = stack.metadata().read()?;
/// println!("{} (id={}): {} EXIF tags", stack.name(), stack.id(), meta.exif_tags.len());
/// # Ok::<(), photostax_core::repository::RepositoryError>(())
/// ```
///
/// [`backends::local::LocalRepository`]: crate::backends::local::LocalRepository
pub trait Repository: FileAccess + Send + Sync {
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

    /// Current structural generation counter.
    ///
    /// Bumps on stack add/remove. Used by [`ScanSnapshot::is_stale`] to
    /// detect whether cached data is still current without re-scanning.
    ///
    /// [`ScanSnapshot::is_stale`]: crate::snapshot::ScanSnapshot::is_stale
    fn generation(&self) -> u64;

    /// Set the image classifier for this repo.
    ///
    /// Called by [`StackManager`](crate::stack_manager::StackManager) at
    /// registration time so all repos in a session share a single classifier.
    fn set_classifier(&mut self, classifier: Arc<dyn ImageClassifier>);

    /// Subscribe to structural changes ([`RepoEvent::StackAdded`] / [`RepoEvent::StackRemoved`]).
    ///
    /// Returns a receiver. Drop it to unsubscribe. The default implementation
    /// returns a receiver that never produces events.
    fn subscribe(&self) -> Result<std::sync::mpsc::Receiver<RepoEvent>, RepositoryError> {
        let (_tx, rx) = std::sync::mpsc::channel();
        Ok(rx)
    }

    /// Start watching for file changes. Returns a receiver for StackEvents.
    /// Default implementation returns a receiver that never produces events.
    ///
    /// The watcher runs in a background thread. Drop the receiver to stop watching.
    fn watch(&self) -> Result<std::sync::mpsc::Receiver<StackEvent>, RepositoryError> {
        let (_tx, rx) = std::sync::mpsc::channel();
        Ok(rx)
    }

    /// Whether this repository supports write operations (rotate, delete,
    /// metadata write, swap).
    ///
    /// Read-only repositories can still be scanned and queried. Any write
    /// attempt on a stack from a read-only repository returns
    /// [`RepositoryError::ReadOnly`].
    ///
    /// Defaults to `true`. Backends that are inherently read-only (e.g.,
    /// archive volumes, network shares without write access) should return
    /// `false`.
    fn is_writable(&self) -> bool {
        true
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

    #[test]
    fn test_default_subscribe_returns_receiver() {
        use crate::backends::local::LocalRepository;
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());
        let rx = repo.subscribe();
        assert!(rx.is_ok());
    }
}
