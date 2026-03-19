//! Content hashing for duplicate detection and cache invalidation.
//!
//! Provides [`ImageFile`] — a file reference with lazy content hashing — and
//! [`HashingReader`] — a streaming wrapper that computes SHA-256 opportunistically
//! as bytes flow through any read operation.
//!
//! ## Design
//!
//! Content hashes are **lazy**: computed on first demand (either via an explicit
//! call to [`ImageFile::content_hash`] or as a side effect of reading through a
//! [`HashingReader`]). This keeps scans fast while enabling duplicate detection
//! when needed.
//!
//! ## Example
//!
//! ```rust,no_run
//! use photostax_core::hashing::{ImageFile, HashingReader};
//! use std::io::Read;
//!
//! // Created during scan — size from fs metadata, no hash yet
//! let mut img = ImageFile::new("/photos/IMG_001.jpg", 4_200_000);
//! assert!(!img.has_hash());
//!
//! // Option A: explicit hash (standalone read)
//! let hash = img.content_hash().unwrap();
//!
//! // Option B: opportunistic hash via HashingReader (zero extra I/O)
//! let file = std::fs::File::open("/photos/IMG_001.jpg").unwrap();
//! let mut reader = HashingReader::new(std::io::BufReader::new(file));
//! let mut buf = Vec::new();
//! reader.read_to_end(&mut buf).unwrap();
//! let hash = reader.finalize();
//! ```

use std::fmt;
use std::io::{self, BufReader, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Buffer size for streaming hash computation (64 KB).
const HASH_BUF_SIZE: usize = 64 * 1024;

/// Truncated hash length in hex characters.
const HASH_HEX_LEN: usize = 16;

/// A reference to an image file with lazy content hashing.
///
/// Created during scanning with `size` captured from filesystem metadata
/// (cheap, no content I/O). The `content_hash` is computed on first demand —
/// either via an explicit [`content_hash`](Self::content_hash) call or by
/// capturing the result of a [`HashingReader`].
#[derive(Clone, Serialize, Deserialize)]
pub struct ImageFile {
    /// URI or path to the image file.
    ///
    /// For local backends this is a filesystem path (e.g., `/photos/IMG_001.jpg`
    /// or `C:\photos\IMG_001.jpg`). For cloud backends it will be a URI
    /// (e.g., `azure://account/container/IMG_001.jpg`).
    ///
    /// The [`Repository`](crate::repository::Repository) backend knows how to
    /// resolve this string into a readable stream.
    pub path: String,

    /// File size in bytes, captured from filesystem metadata during scan.
    pub size: u64,

    /// Cached SHA-256 content hash (truncated to 16 hex chars).
    /// `None` until first computation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
}

impl fmt::Debug for ImageFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageFile")
            .field("path", &self.path)
            .field("size", &self.size)
            .field(
                "content_hash",
                &self.content_hash.as_deref().unwrap_or("<not computed>"),
            )
            .finish()
    }
}

impl ImageFile {
    /// Create a new `ImageFile` with no content hash.
    ///
    /// The `size` should come from filesystem metadata during scan (cheap).
    /// The content hash will be computed lazily on first access.
    pub fn new(path: impl Into<String>, size: u64) -> Self {
        Self {
            path: path.into(),
            size,
            content_hash: None,
        }
    }

    /// Returns `true` if the content hash has already been computed.
    pub fn has_hash(&self) -> bool {
        self.content_hash.is_some()
    }

    /// Return the cached content hash, or compute it by reading the full file.
    ///
    /// On first call, streams the file in 64 KB chunks through SHA-256.
    /// Subsequent calls return the cached result instantly.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the file cannot be read.
    pub fn content_hash(&mut self) -> io::Result<&str> {
        if self.content_hash.is_none() {
            self.content_hash = Some(hash_file(Path::new(&self.path))?);
        }
        Ok(self.content_hash.as_ref().unwrap())
    }

    /// Return the cached content hash without triggering computation.
    ///
    /// Returns `None` if no hash has been computed yet.
    pub fn cached_hash(&self) -> Option<&str> {
        self.content_hash.as_deref()
    }

    /// Store a hash computed externally (e.g., by a [`HashingReader`]).
    ///
    /// This avoids redundant reads when the file has already been streamed
    /// for another purpose (classification, EXIF parsing, rotation, display).
    pub fn set_content_hash(&mut self, hash: String) {
        self.content_hash = Some(hash);
    }

    /// Invalidate the cached hash.
    ///
    /// Call this when a file modification event is detected so the next
    /// access recomputes the hash from the updated content.
    pub fn invalidate_hash(&mut self) {
        self.content_hash = None;
    }
}

/// Compute a SHA-256 content hash by streaming a file in 64 KB chunks.
///
/// Returns a truncated 16-character hex string. This is the standalone
/// fallback used by [`ImageFile::content_hash`] when no [`HashingReader`]
/// has been used.
pub fn hash_file(path: &Path) -> io::Result<String> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(HASH_BUF_SIZE, file);
    let mut hasher = Sha256::new();
    io::copy(&mut reader, &mut hasher)?;
    Ok(format_hash(hasher.finalize()))
}

/// Generate a stable, opaque stack ID from a location URI + relative path + stem.
///
/// The hash is deterministic: the same inputs always produce the same ID.
/// This ensures stack IDs are stable across process restarts.
///
/// # Arguments
///
/// * `location` — Repository location URI (e.g., `file:///C:/photos`)
/// * `relative_dir` — Subfolder relative to repo root (empty string for root)
/// * `stem` — File stem without suffix or extension (e.g., `IMG_001`)
///
/// # Examples
///
/// ```
/// use photostax_core::hashing::make_stack_id;
///
/// let id = make_stack_id("file:///C:/photos", "1984_Mexico", "IMG_001");
/// assert_eq!(id.len(), 16); // 16-char hex hash
///
/// // Same inputs → same ID (deterministic)
/// let id2 = make_stack_id("file:///C:/photos", "1984_Mexico", "IMG_001");
/// assert_eq!(id, id2);
///
/// // Different subfolder → different ID (no collision)
/// let id3 = make_stack_id("file:///C:/photos", "1985_Christmas", "IMG_001");
/// assert_ne!(id, id3);
/// ```
pub fn make_stack_id(location: &str, relative_dir: &str, stem: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(location.as_bytes());
    hasher.update(b"/");
    if !relative_dir.is_empty() {
        hasher.update(relative_dir.as_bytes());
        hasher.update(b"/");
    }
    hasher.update(stem.as_bytes());
    format_hash(hasher.finalize())
}

/// A streaming reader wrapper that computes SHA-256 as bytes flow through.
///
/// Wraps any [`Read`] implementation and feeds every byte through a SHA-256
/// hasher transparently. When the caller is done reading, call [`finalize`](Self::finalize)
/// to retrieve the computed hash.
///
/// This enables **opportunistic hashing** — operations that already read the
/// file (classification, EXIF parsing, rotation) get content hashing for free
/// with zero extra I/O.
///
/// # Example
///
/// ```rust,no_run
/// use photostax_core::hashing::HashingReader;
/// use std::io::Read;
///
/// let file = std::fs::File::open("photo.jpg").unwrap();
/// let mut reader = HashingReader::new(std::io::BufReader::new(file));
///
/// let mut buf = vec![0u8; 1024];
/// while reader.read(&mut buf).unwrap() > 0 {
///     // process bytes...
/// }
///
/// let hash = reader.finalize(); // 16-char hex SHA-256
/// ```
pub struct HashingReader<R: Read> {
    inner: R,
    hasher: Sha256,
}

impl<R: Read> HashingReader<R> {
    /// Wrap a reader to compute its SHA-256 hash as bytes are read.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
        }
    }

    /// Consume the reader and return the computed content hash.
    ///
    /// Returns a 16-character hex string (truncated SHA-256).
    /// Call this after the stream has been fully consumed for an accurate hash.
    pub fn finalize(self) -> String {
        format_hash(self.hasher.finalize())
    }

    /// Get a reference to the inner reader.
    pub fn inner(&self) -> &R {
        &self.inner
    }

    /// Get a mutable reference to the inner reader.
    pub fn inner_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Unwrap the reader without computing the hash.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.hasher.update(&buf[..n]);
        }
        Ok(n)
    }
}

/// Format a SHA-256 digest as a truncated 16-char hex string.
fn format_hash(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .take(HASH_HEX_LEN / 2)
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_file(content: &[u8]) -> (tempfile::NamedTempFile, String) {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content).unwrap();
        f.flush().unwrap();
        let path = f.path().to_string_lossy().to_string();
        (f, path)
    }

    #[test]
    fn test_hash_file_deterministic() {
        let (_f, path) = create_test_file(b"hello world");
        let h1 = hash_file(Path::new(&path)).unwrap();
        let h2 = hash_file(Path::new(&path)).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), HASH_HEX_LEN);
    }

    #[test]
    fn test_hash_file_different_content() {
        let (_f1, p1) = create_test_file(b"hello");
        let (_f2, p2) = create_test_file(b"world");
        assert_ne!(hash_file(Path::new(&p1)).unwrap(), hash_file(Path::new(&p2)).unwrap());
    }

    #[test]
    fn test_hashing_reader_matches_hash_file() {
        let content = b"test content for hashing";
        let (_f, path) = create_test_file(content);

        let direct_hash = hash_file(Path::new(&path)).unwrap();

        let file = std::fs::File::open(Path::new(&path)).unwrap();
        let mut reader = HashingReader::new(BufReader::new(file));
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        let reader_hash = reader.finalize();

        assert_eq!(direct_hash, reader_hash);
    }

    #[test]
    fn test_image_file_lazy_hash() {
        let (_f, path) = create_test_file(b"image bytes");
        let mut img = ImageFile::new(path, 11);

        assert!(!img.has_hash());
        assert!(img.cached_hash().is_none());

        let hash = img.content_hash().unwrap().to_string();
        assert!(img.has_hash());
        assert_eq!(img.cached_hash(), Some(hash.as_str()));
        assert_eq!(hash.len(), HASH_HEX_LEN);
    }

    #[test]
    fn test_image_file_set_hash() {
        let mut img = ImageFile::new("/fake", 0);
        assert!(!img.has_hash());

        img.set_content_hash("abcdef1234567890".to_string());
        assert!(img.has_hash());
        assert_eq!(img.cached_hash(), Some("abcdef1234567890"));
    }

    #[test]
    fn test_image_file_invalidate() {
        let (_f, path) = create_test_file(b"data");
        let mut img = ImageFile::new(path, 4);

        let _ = img.content_hash().unwrap();
        assert!(img.has_hash());

        img.invalidate_hash();
        assert!(!img.has_hash());
    }

    #[test]
    fn test_make_stack_id_deterministic() {
        let id1 = make_stack_id("file:///C:/photos", "1984_Mexico", "IMG_001");
        let id2 = make_stack_id("file:///C:/photos", "1984_Mexico", "IMG_001");
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), HASH_HEX_LEN);
    }

    #[test]
    fn test_make_stack_id_subfolder_uniqueness() {
        let id1 = make_stack_id("file:///photos", "folder_a", "IMG_001");
        let id2 = make_stack_id("file:///photos", "folder_b", "IMG_001");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_make_stack_id_repo_uniqueness() {
        let id1 = make_stack_id("file:///photos1", "", "IMG_001");
        let id2 = make_stack_id("file:///photos2", "", "IMG_001");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_make_stack_id_empty_relative_dir() {
        let id1 = make_stack_id("file:///photos", "", "IMG_001");
        let id2 = make_stack_id("file:///photos", "sub", "IMG_001");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_make_stack_id_cross_backend() {
        let local = make_stack_id("file:///photos", "vacation", "DSC_001");
        let azure = make_stack_id("azure://acct/photos", "vacation", "DSC_001");
        assert_ne!(local, azure);
    }
}
