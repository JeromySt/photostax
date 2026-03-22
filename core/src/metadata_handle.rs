//! Metadata handle trait and MetadataRef accessor for per-stack metadata I/O.
//!
//! [`MetadataHandle`] abstracts metadata read/write operations so that each
//! repository backend provides its own implementation. [`MetadataRef`] is the
//! user-facing wrapper with lazy loading and caching.
//!
//! # Design
//!
//! ```text
//! User → stack.metadata().read()
//!      → MetadataRef.read()
//!      → Arc<dyn MetadataHandle>.load()
//!      → LocalMetadataHandle (EXIF + XMP + sidecar)
//! ```

use std::sync::Arc;

use crate::file_access::ReadSeek;
use crate::photo_stack::Metadata;
use crate::repository::RepositoryError;

/// A no-op metadata handle that returns empty metadata.
///
/// Used as the default for newly-constructed [`PhotoStack`](crate::photo_stack::PhotoStack)
/// instances that have not yet been associated with a repository backend.
pub struct NullMetadataHandle;

impl MetadataHandle for NullMetadataHandle {
    fn load(&self) -> Result<Metadata, RepositoryError> {
        Ok(Metadata::default())
    }

    fn write(&self, _tags: &Metadata) -> Result<(), RepositoryError> {
        Err(RepositoryError::Other(
            "no metadata handle configured".to_string(),
        ))
    }

    fn is_valid(&self) -> bool {
        true
    }
}

/// Per-stack metadata I/O handle provided by a repository backend.
///
/// Each [`MetadataHandle`] manages the reading and writing of EXIF, XMP,
/// and custom sidecar metadata for a single stack. The repository creates
/// one per stack and embeds it in a [`MetadataRef`] on the
/// [`PhotoStack`](crate::photo_stack::PhotoStack).
pub trait MetadataHandle: Send + Sync {
    /// Load all metadata (EXIF, XMP, custom) for this stack.
    ///
    /// Reads from image files, embedded XMP, and sidecar files as
    /// appropriate for the backend.
    fn load(&self) -> Result<Metadata, RepositoryError>;

    /// Write metadata tags to the stack's sidecar/storage.
    ///
    /// The `tags` parameter may contain partial metadata — only the
    /// provided fields are written (merge semantics).
    fn write(&self, tags: &Metadata) -> Result<(), RepositoryError>;

    /// Whether this handle still points to valid backing storage.
    fn is_valid(&self) -> bool;

    /// Read the raw sidecar file bytes, if a sidecar exists.
    ///
    /// Returns the unprocessed file content (e.g., XMP XML for local repos).
    /// Returns `Ok(None)` if no sidecar file exists or the backend doesn't
    /// support sidecars. This is distinct from [`load`](Self::load) which
    /// parses and merges metadata from multiple sources (EXIF, XMP, sidecar).
    fn read_raw(&self) -> Result<Option<Vec<u8>>, RepositoryError> {
        Ok(None)
    }

    /// Open a stream to the raw sidecar file, if a sidecar exists.
    ///
    /// Like [`read_raw`](Self::read_raw) but returns a streaming reader
    /// instead of loading the entire file into memory. Returns `Ok(None)`
    /// if no sidecar file exists.
    fn read_raw_stream(&self) -> Result<Option<Box<dyn ReadSeek>>, RepositoryError> {
        Ok(None)
    }
}

/// User-facing accessor for stack-level metadata.
///
/// Wraps an `Arc<dyn MetadataHandle>` with lazy loading: metadata is not
/// read from disk until [`read`](Self::read) is called. Once loaded, the
/// result is cached and returned on subsequent calls until
/// [`invalidate`](Self::invalidate) is called.
#[derive(Clone)]
pub struct MetadataRef {
    handle: Arc<dyn MetadataHandle>,
    loaded: bool,
    data: Metadata,
}

impl MetadataRef {
    /// Create a new `MetadataRef` wrapping a handle.
    pub fn new(handle: Arc<dyn MetadataHandle>) -> Self {
        Self {
            handle,
            loaded: false,
            data: Metadata::default(),
        }
    }

    /// Whether metadata has been loaded (cached) from the backing store.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Whether the underlying handle is still valid.
    pub fn is_valid(&self) -> bool {
        self.handle.is_valid()
    }

    /// Load metadata from the backing store, caching the result.
    ///
    /// On first call, reads EXIF/XMP/sidecar data via the handle. On
    /// subsequent calls, returns the cached data immediately.
    ///
    /// # Errors
    ///
    /// - [`RepositoryError::StackDeleted`] if the handle was invalidated
    /// - [`RepositoryError::Io`] on read failure
    pub fn read(&mut self) -> Result<&Metadata, RepositoryError> {
        if !self.handle.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        if !self.loaded {
            self.data = self.handle.load()?;
            self.loaded = true;
        }
        Ok(&self.data)
    }

    /// Get cached metadata without triggering a load.
    ///
    /// Returns `None` if metadata hasn't been loaded yet.
    pub fn cached(&self) -> Option<&Metadata> {
        if self.loaded {
            Some(&self.data)
        } else {
            None
        }
    }

    /// Write metadata to the backing store.
    pub fn write(&self, tags: &Metadata) -> Result<(), RepositoryError> {
        if !self.handle.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        self.handle.write(tags)
    }

    /// Read the raw sidecar file bytes without parsing.
    ///
    /// Returns the unprocessed sidecar content (e.g., XMP XML), or `None`
    /// if no sidecar exists. Unlike [`read`](Self::read), this bypasses
    /// all metadata parsing and merging — useful when the raw file content
    /// is needed (e.g., for AI ingestion or external tooling).
    pub fn read_raw(&self) -> Result<Option<Vec<u8>>, RepositoryError> {
        if !self.handle.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        self.handle.read_raw()
    }

    /// Open a stream to the raw sidecar file without parsing.
    ///
    /// Like [`read_raw`](Self::read_raw) but returns a streaming reader
    /// instead of loading the entire file into memory. Returns `None` if
    /// no sidecar exists.
    pub fn read_raw_stream(&self) -> Result<Option<Box<dyn ReadSeek>>, RepositoryError> {
        if !self.handle.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        self.handle.read_raw_stream()
    }

    /// Invalidate the cached metadata, forcing a re-read on next access.
    pub fn invalidate(&mut self) {
        self.loaded = false;
        self.data = Metadata::default();
    }
}

impl std::fmt::Debug for MetadataRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetadataRef")
            .field("is_loaded", &self.loaded)
            .field("is_valid", &self.is_valid())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockMetadataHandle {
        valid: AtomicBool,
        exif: HashMap<String, String>,
    }

    impl MockMetadataHandle {
        fn new() -> Self {
            let mut exif = HashMap::new();
            exif.insert("Make".to_string(), "EPSON".to_string());
            Self {
                valid: AtomicBool::new(true),
                exif,
            }
        }
    }

    impl MetadataHandle for MockMetadataHandle {
        fn load(&self) -> Result<Metadata, RepositoryError> {
            if !self.is_valid() {
                return Err(RepositoryError::StackDeleted);
            }
            Ok(Metadata {
                exif_tags: self.exif.clone(),
                ..Metadata::default()
            })
        }

        fn write(&self, _tags: &Metadata) -> Result<(), RepositoryError> {
            if !self.is_valid() {
                return Err(RepositoryError::StackDeleted);
            }
            Ok(())
        }

        fn is_valid(&self) -> bool {
            self.valid.load(Ordering::Relaxed)
        }
    }

    #[test]
    fn test_metadata_ref_lazy_load() {
        let handle = Arc::new(MockMetadataHandle::new());
        let mut r = MetadataRef::new(handle);
        assert!(!r.is_loaded());
        assert!(r.cached().is_none());

        let meta = r.read().unwrap();
        assert_eq!(meta.exif_tags.get("Make").unwrap(), "EPSON");
        assert!(r.is_loaded());
        assert!(r.cached().is_some());
    }

    #[test]
    fn test_metadata_ref_cached_on_second_read() {
        let handle = Arc::new(MockMetadataHandle::new());
        let mut r = MetadataRef::new(handle);
        let _ = r.read().unwrap();
        // Second read should return cached data
        let meta = r.read().unwrap();
        assert_eq!(meta.exif_tags.get("Make").unwrap(), "EPSON");
    }

    #[test]
    fn test_metadata_ref_invalidate() {
        let handle = Arc::new(MockMetadataHandle::new());
        let mut r = MetadataRef::new(handle);
        let _ = r.read().unwrap();
        assert!(r.is_loaded());

        r.invalidate();
        assert!(!r.is_loaded());
        assert!(r.cached().is_none());
    }

    #[test]
    fn test_metadata_ref_write() {
        let handle = Arc::new(MockMetadataHandle::new());
        let r = MetadataRef::new(handle);
        assert!(r.write(&Metadata::default()).is_ok());
    }

    #[test]
    fn test_metadata_ref_handle_invalidated() {
        let handle = Arc::new(MockMetadataHandle::new());
        let mut r = MetadataRef::new(handle.clone());
        handle.valid.store(false, Ordering::Relaxed);

        assert!(!r.is_valid());
        assert!(r.read().is_err());
        assert!(r.write(&Metadata::default()).is_err());
    }

    #[test]
    fn test_metadata_ref_debug() {
        let handle = Arc::new(MockMetadataHandle::new());
        let r = MetadataRef::new(handle);
        let debug = format!("{:?}", r);
        assert!(debug.contains("MetadataRef"));
        assert!(debug.contains("is_loaded: false"));
    }

    #[test]
    fn test_metadata_ref_clone() {
        let handle = Arc::new(MockMetadataHandle::new());
        let mut r1 = MetadataRef::new(handle);
        let _ = r1.read().unwrap();

        let r2 = r1.clone();
        assert!(r2.is_loaded());
        assert!(r2.cached().is_some());
    }

    #[test]
    fn test_null_metadata_handle_load() {
        let handle = NullMetadataHandle;
        let meta = handle.load().unwrap();
        assert!(meta.exif_tags.is_empty());
    }

    #[test]
    fn test_null_metadata_handle_write_error() {
        let handle = NullMetadataHandle;
        let err = handle.write(&Metadata::default()).unwrap_err();
        assert!(matches!(err, RepositoryError::Other(_)));
    }

    #[test]
    fn test_null_metadata_handle_is_valid() {
        let handle = NullMetadataHandle;
        assert!(handle.is_valid());
    }

    #[test]
    fn test_metadata_ref_with_null_handle() {
        let handle = Arc::new(NullMetadataHandle);
        let mut r = MetadataRef::new(handle);
        let meta = r.read().unwrap();
        assert!(meta.exif_tags.is_empty());
        assert!(r.write(&Metadata::default()).is_err());
    }
}
