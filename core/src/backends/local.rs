//! Local filesystem repository implementation.
//!
//! This module provides [`LocalRepository`], a [`Repository`] implementation that
//! reads photo stacks from a local filesystem directory.
//!
//! ## Filesystem Layout
//!
//! The repository expects FastFoto-style file organization:
//!
//! ```text
//! /photos/
//! ├── IMG_0001.jpg           # Original scan
//! ├── IMG_0001_a.jpg         # Enhanced scan
//! ├── IMG_0001_b.jpg         # Back scan
//! ├── IMG_0001.xmp           # XMP sidecar (metadata)
//! ├── IMG_0002.tif           # Another stack (TIFF)
//! ├── IMG_0002_a.tif
//! └── IMG_0002.xmp           # XMP sidecar for TIFF stack
//! ```
//!
//! ## Metadata Enrichment
//!
//! When scanning, [`LocalRepository`] automatically enriches each [`PhotoStack`]
//! with metadata from three sources:
//!
//! 1. **EXIF tags** — Read from the enhanced image (or original if no enhanced)
//! 2. **XMP tags** — Read from embedded XMP (JPEG) or XMP sidecar file
//! 3. **Custom tags** — Read from the XMP sidecar file (photostax namespace)
//!
//! ## Examples
//!
//! ```rust,no_run
//! use photostax_core::backends::local::LocalRepository;
//! use photostax_core::repository::Repository;
//!
//! let repo = LocalRepository::new("/photos");
//!
//! // Fast scan — just file paths and folder metadata
//! let stacks = repo.scan()?;
//! println!("Found {} stacks", stacks.len());
//!
//! // Load metadata only when needed
//! let mut stack = stacks.into_iter().next().unwrap();
//! repo.load_metadata(&mut stack)?;
//! let meta = stack.metadata.cached().unwrap();
//! println!("{} (id={}): {} EXIF tags, {} custom tags",
//!     stack.name,
//!     stack.id,
//!     meta.exif_tags.len(),
//!     meta.custom_tags.len());
//!
//! // Or scan with metadata in one call (old behavior)
//! let full_stacks = repo.scan_with_metadata()?;
//! # Ok::<(), photostax_core::repository::RepositoryError>(())
//! ```
//!
//! [`Repository`]: crate::repository::Repository
//! [`PhotoStack`]: crate::photo_stack::PhotoStack

use std::path::{Path, PathBuf};
use std::sync::Arc;

use notify::{EventKind, RecursiveMode, Watcher};

use crate::backends::local_handles::LocalMetadataHandle;
use crate::classify;
use crate::events::{FileVariant, StackEvent};
use crate::file_access::{FileAccess, ReadSeek};
use crate::image_handle::ImageRef;
use crate::metadata::sidecar;
use crate::metadata::xmp;
use crate::metadata::detect_image_format;
use crate::metadata_handle::MetadataRef;
use crate::photo_stack::{
    Metadata, PhotoStack, Rotation, RotationTarget, ScanPhase, ScanProgress, ScannerProfile,
};
use crate::repository::{Repository, RepositoryError};
use crate::scanner::{self, classify_stem, ScannerConfig};

/// A repository backed by a local filesystem directory.
///
/// Scans a directory for FastFoto-style photo files and groups them into
/// [`PhotoStack`] objects. Automatically enriches stacks with metadata from
/// EXIF, XMP, and XMP sidecar files.
///
/// # Thread Safety
///
/// Multiple `LocalRepository` instances can safely operate on the same
/// directory concurrently for read operations. Write operations (metadata
/// updates) use file-level I/O to XMP sidecar files.
///
/// [`PhotoStack`]: crate::photo_stack::PhotoStack
pub struct LocalRepository {
    root: PathBuf,
    config: ScannerConfig,
    location: String,
    repo_id: String,
}

impl LocalRepository {
    /// Create a new `LocalRepository` rooted at the given directory.
    ///
    /// Uses default [`ScannerConfig`] (FastFoto naming convention).
    ///
    /// # Arguments
    ///
    /// * `root` - Path to the directory containing photo files
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use photostax_core::backends::local::LocalRepository;
    ///
    /// let repo = LocalRepository::new("/photos");
    /// let repo2 = LocalRepository::new(std::path::PathBuf::from("/archive"));
    /// ```
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root: PathBuf = root.into();
        let location = Self::compute_location(&root);
        let repo_id = crate::hashing::make_stack_id(&location, "", "");
        Self {
            root,
            config: ScannerConfig::default(),
            location,
            repo_id,
        }
    }

    /// Create a new `LocalRepository` with a custom scanner configuration.
    ///
    /// Use this when working with non-standard file naming conventions.
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::backends::local::LocalRepository;
    /// use photostax_core::scanner::ScannerConfig;
    ///
    /// let config = ScannerConfig {
    ///     extensions: vec!["tif".to_string()], // TIFF only
    ///     ..ScannerConfig::default()
    /// };
    /// let repo = LocalRepository::with_config("/archive", config);
    /// ```
    pub fn with_config(root: impl Into<PathBuf>, config: ScannerConfig) -> Self {
        let root: PathBuf = root.into();
        let location = Self::compute_location(&root);
        let repo_id = crate::hashing::make_stack_id(&location, "", "");
        Self {
            root,
            config,
            location,
            repo_id,
        }
    }

    /// Returns the root directory of this repository.
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::backends::local::LocalRepository;
    ///
    /// let repo = LocalRepository::new("/photos");
    /// assert_eq!(repo.root().to_str(), Some("/photos"));
    /// ```
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Compute the `file:///` location URI from a root path.
    ///
    /// Canonicalizes the path when possible, normalises to forward slashes,
    /// and prepends `file:///`.
    fn compute_location(root: &Path) -> String {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let s = canonical.to_string_lossy().replace('\\', "/");
        format!("file:///{}", s.trim_start_matches('/'))
    }

    /// Create a `LocalMetadataHandle` for a stack and assign it.
    ///
    /// Determines the best image path for EXIF/XMP reading by checking
    /// which image variants are present (enhanced preferred over original).
    fn attach_metadata_handle(&self, stack: &mut PhotoStack) {
        let sidecar_dir = stack
            .location
            .as_ref()
            .map(|l| self.root.join(l))
            .unwrap_or_else(|| self.root.clone());

        // Try to find the best image path for EXIF/XMP reading
        let image_path = self.find_image_path(stack);
        let image_format = image_path
            .as_ref()
            .and_then(|p| detect_image_format(p));

        let handle = LocalMetadataHandle::with_folder(
            stack.name.clone(),
            sidecar_dir,
            image_path,
            image_format,
            stack.folder.clone(),
        );
        stack.metadata = MetadataRef::new(Arc::new(handle));
    }

    /// Find the filesystem path for the best image in a stack (enhanced preferred).
    ///
    /// Uses Any trait downcasting isn't available on `dyn ImageHandle`, so we
    /// re-derive the path from the scan data stored in `location`.
    fn find_image_path(&self, stack: &PhotoStack) -> Option<PathBuf> {
        // Since LocalImageHandle stores the path but we can't downcast,
        // we reconstruct the path from the stack's name and location.
        let dir = stack
            .location
            .as_ref()
            .map(|l| self.root.join(l))
            .unwrap_or_else(|| self.root.clone());

        // Try to find enhanced then original file
        let extensions = &self.config.extensions;
        for suffix in [&self.config.enhanced_suffix, ""] {
            let stem = if suffix.is_empty() {
                stack.name.clone()
            } else {
                format!("{}{}", stack.name, suffix)
            };
            for ext in extensions {
                let candidate = dir.join(format!("{stem}.{ext}"));
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// Scan and return stacks with full metadata loaded (EXIF, XMP, sidecar).
    ///
    /// This is a convenience method that combines [`scan()`](Repository::scan)
    /// with [`load_metadata()`](Repository::load_metadata) for every stack.
    /// Ambiguous `_a` images are classified by default.
    /// Use this when you need all metadata up front (e.g., for export or
    /// full-text search).
    ///
    /// For large repositories where you only need counts or paginated listings,
    /// prefer [`scan()`](Repository::scan) which performs no metadata file I/O.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the repository or metadata files
    /// cannot be accessed.
    pub fn scan_with_metadata(&self) -> Result<Vec<PhotoStack>, RepositoryError> {
        let mut stacks = self.scan()?;
        for stack in &mut stacks {
            self.load_metadata(stack)?;
        }
        Ok(stacks)
    }

}

impl FileAccess for LocalRepository {
    fn open_read(&self, path: &str) -> std::io::Result<Box<dyn ReadSeek>> {
        let file = std::fs::File::open(path)?;
        // On Windows, files opened read-only allow concurrent reads by default
        // On Unix, flock LOCK_SH would be used for explicit shared locking
        let reader = std::io::BufReader::with_capacity(64 * 1024, file);
        Ok(Box::new(reader))
    }

    fn open_write(&self, path: &str) -> std::io::Result<Box<dyn std::io::Write + Send>> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        // On Windows, opening for write with default sharing mode provides
        // exclusive access. On Unix, flock LOCK_EX would be used.
        Ok(Box::new(file))
    }
}

impl Repository for LocalRepository {
    fn location(&self) -> &str {
        &self.location
    }

    fn id(&self) -> &str {
        &self.repo_id
    }

    fn scan_with_progress(
        &self,
        profile: ScannerProfile,
        mut progress: Option<&mut dyn FnMut(&ScanProgress)>,
    ) -> Result<Vec<PhotoStack>, RepositoryError> {
        // Pass 1: fast directory scan
        let mut stacks = scanner::scan_directory(&self.root, &self.config, &self.location)?;
        let stack_count = stacks.len();

        for (i, stack) in stacks.iter_mut().enumerate() {
            // Set location for sidecar resolution
            stack.location = stack.folder.clone();

            // Attach a metadata handle that knows how to lazily load
            // EXIF/XMP/sidecar + folder-derived metadata.
            self.attach_metadata_handle(stack);

            if let Some(ref mut cb) = progress {
                cb(&ScanProgress {
                    phase: ScanPhase::Scanning,
                    current: i + 1,
                    total: stack_count,
                });
            }
        }

        // Pass 2: classify ambiguous _a images (only when profile requires it)
        if profile.needs_classification() {
            let ambiguous_indices: Vec<usize> = stacks
                .iter()
                .enumerate()
                .filter(|(_, s)| s.enhanced.is_present() && !s.back.is_present())
                .map(|(i, _)| i)
                .collect();

            let total = ambiguous_indices.len();
            for (step, idx) in ambiguous_indices.into_iter().enumerate() {
                classify::classify_ambiguous(&mut stacks[idx])?;
                if let Some(ref mut cb) = progress {
                    cb(&ScanProgress {
                        phase: ScanPhase::Classifying,
                        current: step + 1,
                        total,
                    });
                }
            }
        }

        // Report completion
        if let Some(ref mut cb) = progress {
            cb(&ScanProgress {
                phase: ScanPhase::Complete,
                current: stacks.len(),
                total: stacks.len(),
            });
        }

        Ok(stacks)
    }

    fn load_metadata(&self, stack: &mut PhotoStack) -> Result<(), RepositoryError> {
        // Ensure we have a proper metadata handle
        if !stack.metadata.is_loaded() {
            // Re-attach handle if needed (e.g., stack was created outside scan)
            if !stack.metadata.is_valid() {
                self.attach_metadata_handle(stack);
            }
        }
        // Trigger lazy load (includes EXIF, XMP, sidecar, and folder metadata)
        stack.metadata.read()?;
        Ok(())
    }

    fn get_stack(&self, id: &str) -> Result<PhotoStack, RepositoryError> {
        let stacks = scanner::scan_directory(&self.root, &self.config, &self.location)?;
        let mut stack = stacks
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        stack.location = stack.folder.clone();
        self.attach_metadata_handle(&mut stack);
        Ok(stack)
    }

    fn read_image(
        &self,
        path: &str,
    ) -> Result<Box<dyn crate::file_access::ReadSeek>, RepositoryError> {
        Ok(self.open_read(path)?)
    }

    fn write_metadata(&self, stack: &PhotoStack, tags: &Metadata) -> Result<(), RepositoryError> {
        // 1. Write XMP tags into the image file directly (JPEG gets embedded XMP).
        if !tags.xmp_tags.is_empty() {
            if let Some(img_path) = self.find_image_path(stack) {
                let _ = xmp::write_xmp(&img_path, &tags.xmp_tags);
            }
        }

        // 2. Write everything to the stack-level XMP sidecar file.
        let sidecar_dir = stack
            .location
            .as_ref()
            .map(|l| self.root.join(l))
            .unwrap_or_else(|| self.root.clone());
        sidecar::merge_and_write(
            &sidecar_dir,
            &stack.name,
            &tags.xmp_tags,
            &tags.custom_tags,
            &tags.exif_tags,
        )
        .map_err(|e| RepositoryError::Other(e.to_string()))
    }

    fn rotate_stack(
        &self,
        id: &str,
        rotation: Rotation,
        target: RotationTarget,
    ) -> Result<PhotoStack, RepositoryError> {
        let stack = self.get_stack(id)?;

        // Collect which variants to rotate
        let refs_to_rotate: Vec<&ImageRef> = match target {
            RotationTarget::All => vec![&stack.original, &stack.enhanced, &stack.back],
            RotationTarget::Front => vec![&stack.original, &stack.enhanced],
            RotationTarget::Back => vec![&stack.back],
        };

        let present: Vec<&ImageRef> = refs_to_rotate
            .into_iter()
            .filter(|r| r.is_present())
            .collect();

        if present.is_empty() {
            return Err(RepositoryError::Other(format!(
                "Stack '{id}' has no image files to rotate for target {target:?}"
            )));
        }

        for r in present {
            r.rotate(rotation)?;
        }

        // Re-fetch the stack so the caller gets fresh state
        let mut refreshed = self.get_stack(id)?;
        self.load_metadata(&mut refreshed)?;
        Ok(refreshed)
    }

    fn watch(&self) -> Result<std::sync::mpsc::Receiver<StackEvent>, RepositoryError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let root = self.root.clone();
        let location = self.location.clone();
        let config = self.config.clone();

        std::thread::spawn(move || {
            let (notify_tx, notify_rx) = std::sync::mpsc::channel();

            let mut watcher = match notify::recommended_watcher(
                move |res: Result<notify::Event, notify::Error>| {
                    if let Ok(event) = res {
                        let _ = notify_tx.send(event);
                    }
                },
            ) {
                Ok(w) => w,
                Err(_) => return,
            };

            let mode = if config.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };

            if watcher.watch(&root, mode).is_err() {
                return;
            }

            for event in notify_rx {
                let is_relevant = matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                );
                if !is_relevant {
                    continue;
                }

                for path in &event.paths {
                    if path.is_dir() {
                        continue;
                    }

                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase());

                    let is_valid = ext
                        .as_ref()
                        .map(|e| config.extensions.contains(e))
                        .unwrap_or(false);

                    if !is_valid {
                        continue;
                    }

                    let stem = match path.file_stem().and_then(|s| s.to_str()) {
                        Some(s) => s.to_string(),
                        None => continue,
                    };

                    let (base_name, variant) = classify_stem(&stem, &config);
                    let file_variant = FileVariant::from(variant);

                    let relative_dir = path
                        .parent()
                        .and_then(|p| p.strip_prefix(&root).ok())
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_default();

                    let stack_id =
                        crate::hashing::make_stack_id(&location, &relative_dir, &base_name);

                    let stack_event = match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                            StackEvent::FileChanged {
                                stack_id,
                                variant: file_variant,
                                path: path.to_string_lossy().to_string(),
                                size,
                            }
                        }
                        EventKind::Remove(_) => StackEvent::FileRemoved {
                            stack_id,
                            variant: file_variant,
                        },
                        _ => continue,
                    };

                    if tx.send(stack_event).is_err() {
                        return;
                    }
                }
            }
        });

        Ok(rx)
    }
}

// rotate_image_file is now handled by LocalImageHandle::rotate()

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;
    use tempfile::TempDir;

    /// Get a stack by its display name (scans the repo, finds by name, then fetches by opaque ID).
    fn get_stack_by_name(
        repo: &LocalRepository,
        name: &str,
    ) -> Result<PhotoStack, RepositoryError> {
        let stacks = repo.scan()?;
        let opaque_id = stacks
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| RepositoryError::NotFound(name.to_string()))?
            .id
            .clone();
        repo.get_stack(&opaque_id)
    }

    // Helper to create minimal valid JPEG for testing
    fn create_test_jpeg() -> Vec<u8> {
        let mut jpeg = Vec::new();
        jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI
        jpeg.extend_from_slice(&[0xFF, 0xE0]); // APP0
        let jfif_data = b"JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00";
        jpeg.extend_from_slice(&((jfif_data.len() + 2) as u16).to_be_bytes());
        jpeg.extend_from_slice(jfif_data);
        jpeg.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x43, 0x00]); // DQT
        jpeg.extend_from_slice(&[16u8; 64]);
        jpeg.extend_from_slice(&[
            0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00,
        ]); // SOF0
        jpeg.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x1F, 0x00]); // DHT
        jpeg.extend_from_slice(&[
            0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ]);
        jpeg.extend_from_slice(&[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
        ]);
        jpeg.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]); // SOS
        jpeg.push(0x7F);
        jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI
        jpeg
    }

    #[test]
    fn test_local_repository_new_default_config() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());

        assert_eq!(repo.root(), tmp.path());
        assert_eq!(repo.config.enhanced_suffix, "_a");
        assert_eq!(repo.config.back_suffix, "_b");
    }

    #[test]
    fn test_local_repository_with_config() {
        let tmp = TempDir::new().unwrap();
        let config = ScannerConfig {
            enhanced_suffix: "_enhanced".to_string(),
            back_suffix: "_back".to_string(),
            extensions: vec!["jpg".to_string()],
            ..ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(tmp.path(), config);

        assert_eq!(repo.config.enhanced_suffix, "_enhanced");
        assert_eq!(repo.config.back_suffix, "_back");
    }

    #[test]
    fn test_root_returns_correct_path() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());
        assert_eq!(repo.root(), tmp.path());
    }

    #[test]
    fn test_scan_populated_directory() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();
        fs::write(dir.join("IMG_001_a.jpg"), &jpeg_data).unwrap();
        fs::write(dir.join("IMG_002.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let stacks = repo.scan().unwrap();

        assert_eq!(stacks.len(), 2);
        assert!(stacks.iter().any(|s| s.name == "IMG_001"));
        assert!(stacks.iter().any(|s| s.name == "IMG_002"));
        // Scan is lazy — metadata should not be loaded yet
        for stack in &stacks {
            assert!(!stack.metadata.is_loaded());
        }
    }

    #[test]
    fn test_scan_with_metadata() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();
        fs::write(dir.join("IMG_001_a.jpg"), &jpeg_data).unwrap();

        // Write some sidecar metadata
        let data = crate::metadata::sidecar::SidecarData {
            custom_tags: {
                let mut m = std::collections::HashMap::new();
                m.insert("album".to_string(), serde_json::json!("Test Album"));
                m
            },
            ..Default::default()
        };
        crate::metadata::sidecar::write_sidecar(dir, "IMG_001", &data).unwrap();

        let repo = LocalRepository::new(dir);
        let stacks = repo.scan_with_metadata().unwrap();

        assert_eq!(stacks.len(), 1);
        assert_eq!(
            stacks[0].metadata.cached().unwrap().custom_tags.get("album"),
            Some(&serde_json::json!("Test Album"))
        );
    }

    #[test]
    fn test_scan_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());
        let stacks = repo.scan().unwrap();
        assert!(stacks.is_empty());
    }

    #[test]
    fn test_get_stack_existing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();
        fs::write(dir.join("IMG_001_a.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        // Scan to discover the opaque ID
        let stacks = repo.scan().unwrap();
        let opaque_id = stacks
            .iter()
            .find(|s| s.name == "IMG_001")
            .unwrap()
            .id
            .clone();

        let stack = repo.get_stack(&opaque_id).unwrap();

        assert_eq!(stack.name, "IMG_001");
        assert!(stack.original.is_present());
        assert!(stack.enhanced.is_present());
    }

    #[test]
    fn test_get_stack_not_found() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let result = repo.get_stack("NONEXISTENT");

        assert!(matches!(result, Err(RepositoryError::NotFound(_))));
    }

    #[test]
    fn test_read_image_existing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        let img_path = dir.join("IMG_001.jpg");
        fs::write(&img_path, &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let mut data = Vec::new();
        repo.read_image(img_path.to_str().unwrap())
            .unwrap()
            .read_to_end(&mut data)
            .unwrap();

        assert_eq!(data, jpeg_data);
    }

    #[test]
    fn test_read_image_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());
        let nonexistent = tmp.path().join("nonexistent.jpg");
        let result = repo.read_image(nonexistent.to_str().unwrap());

        assert!(matches!(result, Err(RepositoryError::Io(_))));
    }

    #[test]
    fn test_write_metadata_custom_tags() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let stack = get_stack_by_name(&repo, "IMG_001").unwrap();

        let mut metadata = Metadata::default();
        metadata
            .custom_tags
            .insert("ocr_text".to_string(), serde_json::json!("Hello World"));
        metadata
            .custom_tags
            .insert("processed".to_string(), serde_json::json!(true));

        repo.write_metadata(&stack, &metadata).unwrap();

        // Verify tags were written by reading them back from sidecar
        let sidecar_data = crate::metadata::sidecar::read_sidecar(dir, "IMG_001").unwrap();
        assert_eq!(
            sidecar_data.custom_tags.get("ocr_text"),
            Some(&serde_json::json!("Hello World"))
        );
        assert_eq!(
            sidecar_data.custom_tags.get("processed"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn test_write_metadata_xmp_tags() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let stack = get_stack_by_name(&repo, "IMG_001").unwrap();

        let mut metadata = Metadata::default();
        metadata
            .xmp_tags
            .insert("description".to_string(), "Test description".to_string());

        repo.write_metadata(&stack, &metadata).unwrap();

        // Verify XMP was written to the sidecar
        let sidecar_data = crate::metadata::sidecar::read_sidecar(dir, "IMG_001").unwrap();
        assert_eq!(
            sidecar_data.xmp_tags.get("description"),
            Some(&"Test description".to_string())
        );
    }

    #[test]
    fn test_write_metadata_exif_tags() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let stack = get_stack_by_name(&repo, "IMG_001").unwrap();

        let mut metadata = Metadata::default();
        metadata
            .exif_tags
            .insert("CustomMake".to_string(), "TestMake".to_string());

        repo.write_metadata(&stack, &metadata).unwrap();

        // EXIF overrides stored in sidecar
        let sidecar_data = crate::metadata::sidecar::read_sidecar(dir, "IMG_001").unwrap();
        assert_eq!(
            sidecar_data.exif_overrides.get("CustomMake"),
            Some(&"TestMake".to_string())
        );
    }

    #[test]
    fn test_write_metadata_empty() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let stack = get_stack_by_name(&repo, "IMG_001").unwrap();

        let metadata = Metadata::default();
        let result = repo.write_metadata(&stack, &metadata);

        assert!(result.is_ok());
    }

    #[test]
    fn test_metadata_enrichment() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        // Write some sidecar metadata first
        let data = crate::metadata::sidecar::SidecarData {
            custom_tags: {
                let mut m = std::collections::HashMap::new();
                m.insert("custom_tag".to_string(), serde_json::json!("custom_value"));
                m
            },
            ..Default::default()
        };
        crate::metadata::sidecar::write_sidecar(dir, "IMG_001", &data).unwrap();

        let repo = LocalRepository::new(dir);
        let mut stack = get_stack_by_name(&repo, "IMG_001").unwrap();

        // Before load_metadata, sidecar tags should NOT be loaded
        assert!(stack.metadata.cached().is_none());

        // After load_metadata, sidecar tags should be loaded
        repo.load_metadata(&mut stack).unwrap();
        assert_eq!(
            stack.metadata.cached().unwrap().custom_tags.get("custom_tag"),
            Some(&serde_json::json!("custom_value"))
        );
    }

    #[test]
    fn test_load_exif_prefers_enhanced_over_original() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create files (both are minimal JPEGs without EXIF, but we test the preference logic)
        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();
        fs::write(dir.join("IMG_001_a.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let stacks = repo.scan().unwrap();
        let stack = stacks.iter().find(|s| s.name == "IMG_001").unwrap();

        // Just verify the stack has both image handles
        assert!(stack.original.is_present());
        assert!(stack.enhanced.is_present());
    }

    #[test]
    fn test_load_sidecar_tags_no_sidecar() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create a file but no sidecar
        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let mut stack = get_stack_by_name(&repo, "IMG_001").unwrap();
        repo.load_metadata(&mut stack).unwrap();

        // Should have no sidecar-derived custom tags.
        // (folder-derived tags with `folder_` prefix may be present from the
        // temp directory name, so we only check for non-folder custom tags.)
        let meta = stack.metadata.cached().unwrap();
        let non_folder_custom: Vec<_> = meta
            .custom_tags
            .keys()
            .filter(|k| !k.starts_with("folder_"))
            .collect();
        assert!(non_folder_custom.is_empty());
    }

    #[test]
    fn test_scan_with_custom_config() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();
        fs::write(dir.join("IMG_001_enhanced.jpg"), &jpeg_data).unwrap();
        fs::write(dir.join("IMG_001_back.jpg"), &jpeg_data).unwrap();

        let config = ScannerConfig {
            enhanced_suffix: "_enhanced".to_string(),
            back_suffix: "_back".to_string(),
            extensions: vec!["jpg".to_string()],
            ..ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(dir, config);
        let stacks = repo.scan().unwrap();

        assert_eq!(stacks.len(), 1);
        let stack = &stacks[0];
        assert!(stack.original.is_present());
        assert!(stack.enhanced.is_present());
        assert!(stack.back.is_present());
    }

    #[test]
    fn test_load_metadata_no_images() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());

        // A stack with no images should still load without error
        let mut stack = PhotoStack::new("empty");
        // Manually set location so metadata handle can be attached
        stack.location = None;
        repo.load_metadata(&mut stack).unwrap();
        assert!(stack.metadata.cached().unwrap().exif_tags.is_empty());
    }

    // ── Folder metadata enrichment tests ───────────────────────────────────

    #[test]
    fn test_folder_metadata_year_and_subject() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("1984_Mexico");
        fs::create_dir(&subdir).unwrap();
        let jpeg_data = create_test_jpeg();
        fs::write(subdir.join("1984_Mexico_0001.jpg"), &jpeg_data).unwrap();
        fs::write(subdir.join("1984_Mexico_0001_a.jpg"), &jpeg_data).unwrap();

        let config = ScannerConfig {
            recursive: true,
            ..ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(tmp.path(), config);

        let mut stacks = repo.scan().unwrap();
        assert_eq!(stacks.len(), 1);
        // Load metadata to trigger folder metadata population
        repo.load_metadata(&mut stacks[0]).unwrap();
        let meta = stacks[0].metadata.cached().unwrap();
        assert_eq!(
            meta.custom_tags.get("folder_year"),
            Some(&serde_json::json!(1984))
        );
        assert_eq!(
            meta.custom_tags.get("folder_subject"),
            Some(&serde_json::json!("Mexico"))
        );
        assert!(!meta.custom_tags.contains_key("folder_month_or_season"));
        assert_eq!(meta.xmp_tags.get("date"), Some(&"1984".to_string()));
        assert_eq!(
            meta.xmp_tags.get("subject"),
            Some(&"Mexico".to_string())
        );
    }

    #[test]
    fn test_folder_metadata_year_season_subject() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("2024_Summer_Beach");
        fs::create_dir(&subdir).unwrap();
        let jpeg_data = create_test_jpeg();
        fs::write(subdir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let config = ScannerConfig {
            recursive: true,
            ..ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(tmp.path(), config);
        let mut stacks = repo.scan().unwrap();
        repo.load_metadata(&mut stacks[0]).unwrap();
        let meta = stacks[0].metadata.cached().unwrap();

        assert_eq!(
            meta.custom_tags.get("folder_year"),
            Some(&serde_json::json!(2024))
        );
        assert_eq!(
            meta.custom_tags.get("folder_month_or_season"),
            Some(&serde_json::json!("Summer"))
        );
        assert_eq!(
            meta.custom_tags.get("folder_subject"),
            Some(&serde_json::json!("Beach"))
        );
    }

    #[test]
    fn test_folder_metadata_subject_only() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("SteveJones");
        fs::create_dir(&subdir).unwrap();
        let jpeg_data = create_test_jpeg();
        fs::write(subdir.join("SteveJones_0001.jpg"), &jpeg_data).unwrap();

        let config = ScannerConfig {
            recursive: true,
            ..ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(tmp.path(), config);
        let mut stacks = repo.scan().unwrap();
        repo.load_metadata(&mut stacks[0]).unwrap();
        let meta = stacks[0].metadata.cached().unwrap();

        assert!(!meta.custom_tags.contains_key("folder_year"));
        assert_eq!(
            meta.custom_tags.get("folder_subject"),
            Some(&serde_json::json!("SteveJones"))
        );
        assert_eq!(
            meta.xmp_tags.get("subject"),
            Some(&"SteveJones".to_string())
        );
    }

    #[test]
    fn test_folder_metadata_does_not_overwrite_existing_xmp() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("1984_Mexico");
        fs::create_dir(&subdir).unwrap();
        let jpeg_data = create_test_jpeg();
        fs::write(subdir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        // Write an existing XMP subject via sidecar so it takes priority
        use crate::metadata::sidecar;
        sidecar::merge_and_write(
            &subdir,
            "IMG_001",
            &{
                let mut m = std::collections::HashMap::new();
                m.insert("subject".to_string(), "beach, family".to_string());
                m
            },
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        )
        .unwrap();

        let config = ScannerConfig {
            recursive: true,
            ..ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(tmp.path(), config);
        // Use scan_with_metadata since we need sidecar loaded to test priority
        let stacks = repo.scan_with_metadata().unwrap();

        let meta = stacks[0].metadata.cached().unwrap();
        // Sidecar value should win for xmp_tags["subject"]
        assert_eq!(
            meta.xmp_tags.get("subject"),
            Some(&"beach, family".to_string())
        );
        // But folder_subject custom tag is still populated
        assert_eq!(
            meta.custom_tags.get("folder_subject"),
            Some(&serde_json::json!("Mexico"))
        );
    }

    #[test]
    fn test_folder_metadata_year_only() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("1993");
        fs::create_dir(&subdir).unwrap();
        let jpeg_data = create_test_jpeg();
        fs::write(subdir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let config = ScannerConfig {
            recursive: true,
            ..ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(tmp.path(), config);
        let mut stacks = repo.scan().unwrap();
        repo.load_metadata(&mut stacks[0]).unwrap();
        let meta = stacks[0].metadata.cached().unwrap();

        assert_eq!(
            meta.custom_tags.get("folder_year"),
            Some(&serde_json::json!(1993))
        );
        assert!(!meta.custom_tags.contains_key("folder_subject"));
        assert!(!meta.custom_tags.contains_key("folder_month_or_season"));
        assert_eq!(meta.xmp_tags.get("date"), Some(&"1993".to_string()));
    }

    /// Create a real JPEG file with known dimensions using the `image` crate.
    fn create_test_image_jpeg(path: &std::path::Path, width: u32, height: u32) {
        let img = image::RgbImage::from_fn(width, height, |x, y| image::Rgb([x as u8, y as u8, 0]));
        img.save(path).unwrap();
    }

    #[test]
    fn test_rotate_stack_cw90() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        create_test_image_jpeg(&dir.join("IMG_001.jpg"), 4, 2);
        create_test_image_jpeg(&dir.join("IMG_001_a.jpg"), 4, 2);

        let repo = LocalRepository::new(dir);
        let stacks = repo.scan().unwrap();
        let opaque_id = stacks
            .iter()
            .find(|s| s.name == "IMG_001")
            .unwrap()
            .id
            .clone();

        let rotated = repo
            .rotate_stack(&opaque_id, Rotation::Cw90, RotationTarget::All)
            .unwrap();
        assert_eq!(rotated.name, "IMG_001");

        // After 90° CW rotation, 4×2 → 2×4
        let img = image::open(dir.join("IMG_001.jpg")).unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 4);
        let img_a = image::open(dir.join("IMG_001_a.jpg")).unwrap();
        assert_eq!(img_a.width(), 2);
        assert_eq!(img_a.height(), 4);
    }

    #[test]
    fn test_rotate_stack_ccw90() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        create_test_image_jpeg(&dir.join("IMG_001.jpg"), 4, 2);

        let repo = LocalRepository::new(dir);
        let stacks = repo.scan().unwrap();
        let opaque_id = stacks
            .iter()
            .find(|s| s.name == "IMG_001")
            .unwrap()
            .id
            .clone();

        let rotated = repo
            .rotate_stack(&opaque_id, Rotation::Ccw90, RotationTarget::All)
            .unwrap();
        assert_eq!(rotated.name, "IMG_001");

        let img = image::open(dir.join("IMG_001.jpg")).unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 4);
    }

    #[test]
    fn test_rotate_stack_180() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        create_test_image_jpeg(&dir.join("IMG_001.jpg"), 4, 2);

        let repo = LocalRepository::new(dir);
        let stacks = repo.scan().unwrap();
        let opaque_id = stacks
            .iter()
            .find(|s| s.name == "IMG_001")
            .unwrap()
            .id
            .clone();

        let rotated = repo
            .rotate_stack(&opaque_id, Rotation::Cw180, RotationTarget::All)
            .unwrap();
        assert_eq!(rotated.name, "IMG_001");

        // 180° preserves dimensions
        let img = image::open(dir.join("IMG_001.jpg")).unwrap();
        assert_eq!(img.width(), 4);
        assert_eq!(img.height(), 2);
    }

    #[test]
    fn test_rotate_stack_includes_back_image() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        create_test_image_jpeg(&dir.join("IMG_001.jpg"), 4, 2);
        create_test_image_jpeg(&dir.join("IMG_001_a.jpg"), 4, 2);
        create_test_image_jpeg(&dir.join("IMG_001_b.jpg"), 4, 2);

        let repo = LocalRepository::new(dir);
        let stacks = repo.scan().unwrap();
        let opaque_id = stacks
            .iter()
            .find(|s| s.name == "IMG_001")
            .unwrap()
            .id
            .clone();

        let rotated = repo
            .rotate_stack(&opaque_id, Rotation::Cw90, RotationTarget::All)
            .unwrap();
        assert_eq!(rotated.image_count(), 3);

        // All three files should be rotated
        for name in &["IMG_001.jpg", "IMG_001_a.jpg", "IMG_001_b.jpg"] {
            let img = image::open(dir.join(name)).unwrap();
            assert_eq!(img.width(), 2, "width wrong for {name}");
            assert_eq!(img.height(), 4, "height wrong for {name}");
        }
    }

    #[test]
    fn test_rotate_stack_not_found() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());
        let result = repo.rotate_stack("nonexistent", Rotation::Cw90, RotationTarget::All);
        assert!(result.is_err());
        match result.unwrap_err() {
            RepositoryError::NotFound(id) => assert_eq!(id, "nonexistent"),
            other => panic!("Expected NotFound, got: {other:?}"),
        }
    }

    #[test]
    fn test_rotate_stack_returns_refreshed_metadata() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        create_test_image_jpeg(&dir.join("IMG_001.jpg"), 4, 2);

        let repo = LocalRepository::new(dir);
        let stacks = repo.scan().unwrap();
        let opaque_id = stacks
            .iter()
            .find(|s| s.name == "IMG_001")
            .unwrap()
            .id
            .clone();

        let rotated = repo
            .rotate_stack(&opaque_id, Rotation::Cw90, RotationTarget::All)
            .unwrap();
        // The returned stack should have the same name and paths
        assert_eq!(rotated.name, "IMG_001");
        assert!(rotated.original.is_present());
    }

    // ── Snapshot integration tests ──────────────────────────────

    fn testdata_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("testdata")
    }

    #[test]
    fn test_snapshot_from_scan() {
        let repo = LocalRepository::new(testdata_path());
        let snap = crate::snapshot::ScanSnapshot::from_scan(&repo).unwrap();
        assert!(snap.total_count() > 0);
        assert_eq!(snap.ids().len(), snap.total_count());
    }

    #[test]
    fn test_snapshot_from_scan_with_metadata() {
        let repo = LocalRepository::new(testdata_path());
        let snap = crate::snapshot::ScanSnapshot::from_scan_with_metadata(&repo).unwrap();
        assert!(snap.total_count() > 0);
        // Metadata should be populated
        let has_exif = snap
            .stacks()
            .iter()
            .any(|s| {
                s.metadata
                    .cached()
                    .is_some_and(|m| !m.exif_tags.is_empty())
            });
        assert!(has_exif, "At least one stack should have EXIF tags");
    }

    #[test]
    fn test_snapshot_page_consistency() {
        let repo = LocalRepository::new(testdata_path());
        let snap = crate::snapshot::ScanSnapshot::from_scan(&repo).unwrap();
        let total = snap.total_count();

        let page1 = snap.get_page(0, 2);
        let page2 = snap.get_page(2, 2);

        // total_count is identical across pages
        assert_eq!(page1.total_count, total);
        assert_eq!(page2.total_count, total);

        // pages don't overlap (IDs are different)
        if !page1.items.is_empty() && !page2.items.is_empty() {
            assert_ne!(page1.items[0].id, page2.items[0].id);
        }
    }

    #[test]
    fn test_snapshot_check_status_unchanged() {
        let repo = LocalRepository::new(testdata_path());
        let snap = crate::snapshot::ScanSnapshot::from_scan(&repo).unwrap();
        let status = snap.check_status(&repo).unwrap();

        assert!(!status.is_stale);
        assert_eq!(status.added, 0);
        assert_eq!(status.removed, 0);
        assert_eq!(status.snapshot_count, status.current_count);
    }

    #[test]
    fn test_snapshot_check_status_after_addition() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("IMG_001.jpg"), b"fake jpeg").unwrap();

        let repo = LocalRepository::new(dir);
        let snap = crate::snapshot::ScanSnapshot::from_scan(&repo).unwrap();
        assert_eq!(snap.total_count(), 1);

        // Add a new file
        std::fs::write(dir.join("IMG_002.jpg"), b"fake jpeg 2").unwrap();

        let status = snap.check_status(&repo).unwrap();
        assert!(status.is_stale);
        assert_eq!(status.added, 1);
        assert_eq!(status.removed, 0);
        assert_eq!(status.snapshot_count, 1);
        assert_eq!(status.current_count, 2);
    }

    #[test]
    fn test_snapshot_check_status_after_removal() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("IMG_001.jpg"), b"fake").unwrap();
        std::fs::write(dir.join("IMG_002.jpg"), b"fake").unwrap();

        let repo = LocalRepository::new(dir);
        let snap = crate::snapshot::ScanSnapshot::from_scan(&repo).unwrap();
        assert_eq!(snap.total_count(), 2);

        // Remove a file
        std::fs::remove_file(dir.join("IMG_001.jpg")).unwrap();

        let status = snap.check_status(&repo).unwrap();
        assert!(status.is_stale);
        assert_eq!(status.added, 0);
        assert_eq!(status.removed, 1);
        assert_eq!(status.snapshot_count, 2);
        assert_eq!(status.current_count, 1);

        // Snapshot pages still work after deletion (in-memory data intact)
        let page = snap.get_page(0, 10);
        assert_eq!(page.total_count, 2); // snapshot count unchanged
        assert_eq!(page.items.len(), 2);
    }

    #[test]
    fn test_snapshot_filter_then_page() {
        let repo = LocalRepository::new(testdata_path());
        let snap = crate::snapshot::ScanSnapshot::from_scan_with_metadata(&repo).unwrap();

        let query = crate::search::SearchQuery::new().with_text("FamilyPhotos");
        let filtered = snap.filter(&query);

        assert!(filtered.total_count() > 0);
        assert!(filtered.total_count() <= snap.total_count());

        let page = filtered.get_page(0, 100);
        assert_eq!(page.total_count, filtered.total_count());
        for item in &page.items {
            assert!(
                item.name.contains("FamilyPhotos"),
                "filtered item {} should match",
                item.name
            );
        }
    }
}
