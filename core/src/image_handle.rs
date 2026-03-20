//! Image handle trait and ImageRef accessor for per-file I/O operations.
//!
//! The [`ImageHandle`] trait abstracts per-file I/O so that each repository
//! backend can provide its own implementation (e.g., `LocalImageHandle` for
//! the filesystem, `ForeignImageHandle` for cloud/host-language repos).
//!
//! [`ImageRef`] is the user-facing wrapper: it holds an optional
//! `Arc<dyn ImageHandle>` (absent when the image variant doesn't exist)
//! and caches computed values like content hash and dimensions.
//!
//! # Design
//!
//! ```text
//! User → stack.original.read()
//!      → ImageRef.read()
//!      → Arc<dyn ImageHandle>.read()
//!      → LocalImageHandle (std::fs::File::open)
//! ```

use std::io::Read;
use std::sync::Arc;

use crate::file_access::ReadSeek;
use crate::hashing::HashingReader;
use crate::photo_stack::Rotation;
use crate::repository::RepositoryError;

/// Per-file I/O handle provided by a repository backend.
///
/// Each [`ImageHandle`] is bound to a single physical file (original,
/// enhanced, or back). The repository creates one per file during scan and
/// embeds it inside an [`ImageRef`] on the [`PhotoStack`](crate::photo_stack::PhotoStack).
///
/// Implementations must be `Send + Sync` because `PhotoStack` values may be
/// shared across threads (e.g., placed in a cache and read from multiple
/// query results).
pub trait ImageHandle: Send + Sync {
    /// Read the full image as a seekable stream.
    fn read(&self) -> Result<Box<dyn ReadSeek>, RepositoryError>;

    /// Open a hashing reader that computes SHA-256 opportunistically as
    /// bytes flow through. Call [`HashingReader::finalize`] after the stream
    /// is fully consumed to retrieve the hash.
    fn stream(&self) -> Result<HashingReader<Box<dyn Read + Send>>, RepositoryError>;

    /// Return the content hash (truncated SHA-256, 16 hex chars).
    ///
    /// Implementations should cache the result after first computation.
    fn hash(&self) -> Result<String, RepositoryError>;

    /// Return the image dimensions `(width, height)` in pixels.
    fn dimensions(&self) -> Result<(u32, u32), RepositoryError>;

    /// File size in bytes (captured cheaply from metadata during scan).
    fn size(&self) -> u64;

    /// Rotate the image on disk by the given angle.
    fn rotate(&self, rotation: Rotation) -> Result<(), RepositoryError>;

    /// Whether this handle still points to a valid file.
    ///
    /// Returns `false` after the backing file has been deleted and the
    /// repository has called [`invalidate`](Self::invalidate).
    fn is_valid(&self) -> bool;

    /// Mark this handle as invalid and clear any cached data.
    ///
    /// Called by the repository when the backing file is deleted or the
    /// stack is removed.
    fn invalidate(&self);
}

/// User-facing accessor for a single image variant within a [`PhotoStack`](crate::photo_stack::PhotoStack).
///
/// `ImageRef` wraps an optional `Arc<dyn ImageHandle>`. When the image
/// variant is not present (e.g., no back scan), the handle is `None` and
/// [`is_present`](Self::is_present) returns `false`.
///
/// # Caching
///
/// Content hash and dimensions are cached on first access. Call
/// [`invalidate_caches`](Self::invalidate_caches) when the backing file
/// changes to force re-computation on next access.
#[derive(Clone)]
pub struct ImageRef {
    handle: Option<Arc<dyn ImageHandle>>,
    cached_hash: Option<String>,
    cached_dimensions: Option<(u32, u32)>,
}

impl ImageRef {
    /// Create an `ImageRef` wrapping a handle.
    pub fn new(handle: Arc<dyn ImageHandle>) -> Self {
        Self {
            handle: Some(handle),
            cached_hash: None,
            cached_dimensions: None,
        }
    }

    /// Create an absent `ImageRef` (variant does not exist).
    pub fn absent() -> Self {
        Self {
            handle: None,
            cached_hash: None,
            cached_dimensions: None,
        }
    }

    /// Whether this image variant exists in the stack.
    pub fn is_present(&self) -> bool {
        self.handle.is_some()
    }

    /// Whether the underlying file handle is still valid.
    ///
    /// Returns `false` if the variant is absent or the handle has been
    /// invalidated (e.g., file deleted).
    pub fn is_valid(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| h.is_valid())
    }

    /// File size in bytes, or `None` if the variant is absent.
    pub fn size(&self) -> Option<u64> {
        self.handle.as_ref().map(|h| h.size())
    }

    /// Read the full image as a seekable byte stream.
    ///
    /// # Errors
    ///
    /// - [`RepositoryError::NotFound`] if the variant is absent
    /// - [`RepositoryError::StackDeleted`] if the handle was invalidated
    /// - [`RepositoryError::Io`] on I/O failure
    pub fn read(&self) -> Result<Box<dyn ReadSeek>, RepositoryError> {
        let handle = self.require_handle()?;
        handle.read()
    }

    /// Open a streaming reader that computes the content hash as bytes
    /// are consumed. After reading to EOF, call `.finalize()` to get the
    /// hash with zero extra I/O.
    pub fn stream(&self) -> Result<HashingReader<Box<dyn Read + Send>>, RepositoryError> {
        let handle = self.require_handle()?;
        handle.stream()
    }

    /// Return the content hash, computing and caching it on first call.
    ///
    /// # Errors
    ///
    /// - [`RepositoryError::NotFound`] if the variant is absent
    /// - [`RepositoryError::Io`] on read failure
    pub fn hash(&mut self) -> Result<&str, RepositoryError> {
        if self.cached_hash.is_none() {
            let handle = self.require_handle()?;
            self.cached_hash = Some(handle.hash()?);
        }
        Ok(self.cached_hash.as_ref().unwrap())
    }

    /// Return the cached hash without triggering computation.
    pub fn cached_hash(&self) -> Option<&str> {
        self.cached_hash.as_deref()
    }

    /// Return image dimensions `(width, height)`, computing and caching
    /// on first call.
    pub fn dimensions(&mut self) -> Result<(u32, u32), RepositoryError> {
        if self.cached_dimensions.is_none() {
            let handle = self.require_handle()?;
            self.cached_dimensions = Some(handle.dimensions()?);
        }
        Ok(self.cached_dimensions.unwrap())
    }

    /// Rotate the image on disk.
    pub fn rotate(&self, rotation: Rotation) -> Result<(), RepositoryError> {
        let handle = self.require_handle()?;
        handle.rotate(rotation)
    }

    /// Clear cached hash and dimensions. The next access will re-read
    /// from the backing file.
    pub fn invalidate_caches(&mut self) {
        self.cached_hash = None;
        self.cached_dimensions = None;
    }

    /// Get a reference to the underlying handle, if present and valid.
    fn require_handle(&self) -> Result<&Arc<dyn ImageHandle>, RepositoryError> {
        match &self.handle {
            None => Err(RepositoryError::NotFound(
                "image variant not present".to_string(),
            )),
            Some(h) if !h.is_valid() => Err(RepositoryError::StackDeleted),
            Some(h) => Ok(h),
        }
    }
}

impl std::fmt::Debug for ImageRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageRef")
            .field("is_present", &self.is_present())
            .field("is_valid", &self.is_valid())
            .field("size", &self.size())
            .field("cached_hash", &self.cached_hash)
            .field("cached_dimensions", &self.cached_dimensions)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A mock ImageHandle for testing.
    struct MockImageHandle {
        data: Vec<u8>,
        valid: AtomicBool,
    }

    impl MockImageHandle {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                valid: AtomicBool::new(true),
            }
        }
    }

    impl ImageHandle for MockImageHandle {
        fn read(&self) -> Result<Box<dyn ReadSeek>, RepositoryError> {
            if !self.is_valid() {
                return Err(RepositoryError::StackDeleted);
            }
            Ok(Box::new(Cursor::new(self.data.clone())))
        }

        fn stream(&self) -> Result<HashingReader<Box<dyn Read + Send>>, RepositoryError> {
            if !self.is_valid() {
                return Err(RepositoryError::StackDeleted);
            }
            Ok(HashingReader::new(
                Box::new(Cursor::new(self.data.clone())) as Box<dyn Read + Send>
            ))
        }

        fn hash(&self) -> Result<String, RepositoryError> {
            if !self.is_valid() {
                return Err(RepositoryError::StackDeleted);
            }
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&self.data);
            let digest = hasher.finalize();
            Ok(digest.iter().take(8).map(|b| format!("{b:02x}")).collect())
        }

        fn dimensions(&self) -> Result<(u32, u32), RepositoryError> {
            Ok((640, 480))
        }

        fn size(&self) -> u64 {
            self.data.len() as u64
        }

        fn rotate(&self, _rotation: Rotation) -> Result<(), RepositoryError> {
            Ok(())
        }

        fn is_valid(&self) -> bool {
            self.valid.load(Ordering::Relaxed)
        }

        fn invalidate(&self) {
            self.valid.store(false, Ordering::Relaxed);
        }
    }

    #[test]
    fn test_image_ref_absent() {
        let r = ImageRef::absent();
        assert!(!r.is_present());
        assert!(!r.is_valid());
        assert!(r.size().is_none());
        assert!(r.read().is_err());
    }

    #[test]
    fn test_image_ref_present_read() {
        let handle = Arc::new(MockImageHandle::new(b"hello".to_vec()));
        let r = ImageRef::new(handle);
        assert!(r.is_present());
        assert!(r.is_valid());
        assert_eq!(r.size(), Some(5));

        let mut reader = r.read().unwrap();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"hello");
    }

    #[test]
    fn test_image_ref_hash_cached() {
        let handle = Arc::new(MockImageHandle::new(b"content".to_vec()));
        let mut r = ImageRef::new(handle);
        assert!(r.cached_hash().is_none());

        let hash1 = r.hash().unwrap().to_string();
        assert_eq!(hash1.len(), 16);
        assert!(r.cached_hash().is_some());

        // Second call returns cached value
        let hash2 = r.hash().unwrap().to_string();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_image_ref_dimensions_cached() {
        let handle = Arc::new(MockImageHandle::new(vec![]));
        let mut r = ImageRef::new(handle);
        let (w, h) = r.dimensions().unwrap();
        assert_eq!((w, h), (640, 480));
    }

    #[test]
    fn test_image_ref_invalidate_caches() {
        let handle = Arc::new(MockImageHandle::new(b"data".to_vec()));
        let mut r = ImageRef::new(handle);
        let _ = r.hash().unwrap();
        let _ = r.dimensions().unwrap();
        assert!(r.cached_hash().is_some());

        r.invalidate_caches();
        assert!(r.cached_hash().is_none());
    }

    #[test]
    fn test_image_ref_handle_invalidated() {
        let handle = Arc::new(MockImageHandle::new(b"data".to_vec()));
        let r = ImageRef::new(handle.clone());
        assert!(r.is_valid());

        handle.invalidate();
        assert!(!r.is_valid());
        assert!(r.read().is_err());
    }

    #[test]
    fn test_image_ref_stream_and_finalize() {
        let handle = Arc::new(MockImageHandle::new(b"stream data".to_vec()));
        let r = ImageRef::new(handle);
        let mut stream = r.stream().unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"stream data");
        let hash = stream.finalize();
        assert_eq!(hash.len(), 16);
    }

    #[test]
    fn test_image_ref_rotate() {
        let handle = Arc::new(MockImageHandle::new(vec![]));
        let r = ImageRef::new(handle);
        assert!(r.rotate(Rotation::Cw90).is_ok());
    }

    #[test]
    fn test_image_ref_debug() {
        let r = ImageRef::absent();
        let debug = format!("{:?}", r);
        assert!(debug.contains("ImageRef"));
        assert!(debug.contains("is_present: false"));
    }

    #[test]
    fn test_image_ref_clone() {
        let handle = Arc::new(MockImageHandle::new(b"data".to_vec()));
        let mut r1 = ImageRef::new(handle);
        let _ = r1.hash().unwrap();

        let r2 = r1.clone();
        assert!(r2.is_present());
        assert_eq!(r2.cached_hash(), r1.cached_hash());
    }
}
