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
//! ├── .photostax.db          # Sidecar database (auto-created)
//! ├── IMG_0001.jpg           # Original scan
//! ├── IMG_0001_a.jpg         # Enhanced scan
//! ├── IMG_0001_b.jpg         # Back scan
//! ├── IMG_0002.tif           # Another stack (TIFF)
//! ├── IMG_0002_a.tif
//! └── IMG_0002.xmp           # XMP sidecar for TIFF
//! ```
//!
//! ## Metadata Enrichment
//!
//! When scanning, [`LocalRepository`] automatically enriches each [`PhotoStack`]
//! with metadata from three sources:
//!
//! 1. **EXIF tags** — Read from the enhanced image (or original if no enhanced)
//! 2. **XMP tags** — Read from embedded XMP (JPEG) or sidecar `.xmp` file (TIFF)
//! 3. **Custom tags** — Read from the `.photostax.db` sidecar database
//!
//! ## Examples
//!
//! ```rust,no_run
//! use photostax_core::backends::local::LocalRepository;
//! use photostax_core::repository::Repository;
//!
//! let repo = LocalRepository::new("/photos");
//!
//! // Scan all stacks
//! let stacks = repo.scan()?;
//! for stack in &stacks {
//!     println!("{}: {} EXIF tags, {} custom tags",
//!         stack.id,
//!         stack.metadata.exif_tags.len(),
//!         stack.metadata.custom_tags.len());
//! }
//!
//! // Get a specific stack
//! let stack = repo.get_stack("IMG_0001")?;
//! # Ok::<(), photostax_core::repository::RepositoryError>(())
//! ```
//!
//! [`Repository`]: crate::repository::Repository
//! [`PhotoStack`]: crate::photo_stack::PhotoStack

use std::path::{Path, PathBuf};

use crate::metadata::exif;
use crate::metadata::sidecar::SidecarDb;
use crate::metadata::xmp;
use crate::photo_stack::{Metadata, PhotoStack};
use crate::repository::{Repository, RepositoryError};
use crate::scanner::{self, ScannerConfig};

/// A repository backed by a local filesystem directory.
///
/// Scans a directory for FastFoto-style photo files and groups them into
/// [`PhotoStack`] objects. Automatically enriches stacks with metadata from
/// EXIF, XMP, and the sidecar database.
///
/// # Thread Safety
///
/// Multiple `LocalRepository` instances can safely operate on the same
/// directory concurrently for read operations. Write operations (metadata
/// updates) use SQLite's built-in locking.
///
/// [`PhotoStack`]: crate::photo_stack::PhotoStack
pub struct LocalRepository {
    root: PathBuf,
    config: ScannerConfig,
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
        Self {
            root: root.into(),
            config: ScannerConfig::default(),
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
        Self {
            root: root.into(),
            config,
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

    /// Load EXIF tags from the best available image in the stack.
    ///
    /// Prefers the enhanced image for EXIF data since it typically has
    /// richer metadata from FastFoto's processing. Falls back to original
    /// if no enhanced image exists.
    fn load_exif_tags(&self, stack: &PhotoStack) -> std::collections::HashMap<String, String> {
        let candidate = stack.enhanced.as_ref().or(stack.original.as_ref());
        match candidate {
            Some(path) => exif::read_exif_tags(path).unwrap_or_default(),
            None => std::collections::HashMap::new(),
        }
    }

    /// Load XMP tags from the best available image in the stack.
    ///
    /// For JPEG: reads embedded XMP from the file.
    /// For TIFF: reads from sidecar `.xmp` file if present.
    fn load_xmp_tags(&self, stack: &PhotoStack) -> std::collections::HashMap<String, String> {
        let candidate = stack.enhanced.as_ref().or(stack.original.as_ref());
        match candidate {
            Some(path) => xmp::read_xmp(path).unwrap_or_default(),
            None => std::collections::HashMap::new(),
        }
    }

    /// Load custom tags from the sidecar database.
    fn load_sidecar_tags(
        &self,
        stack_id: &str,
    ) -> std::collections::HashMap<String, serde_json::Value> {
        match SidecarDb::open(&self.root) {
            Ok(db) => db.get_tags(stack_id).unwrap_or_default(),
            Err(_) => std::collections::HashMap::new(),
        }
    }

    /// Enrich a PhotoStack with EXIF, XMP, and sidecar metadata.
    ///
    /// This is the core metadata merging logic that combines all three
    /// metadata sources into the stack's unified [`Metadata`] structure.
    ///
    /// [`Metadata`]: crate::photo_stack::Metadata
    fn enrich_metadata(&self, stack: &mut PhotoStack) {
        stack.metadata.exif_tags = self.load_exif_tags(stack);
        stack.metadata.xmp_tags = self.load_xmp_tags(stack);
        stack.metadata.custom_tags = self.load_sidecar_tags(&stack.id);
    }
}

impl Repository for LocalRepository {
    fn scan(&self) -> Result<Vec<PhotoStack>, RepositoryError> {
        let mut stacks = scanner::scan_directory(&self.root, &self.config)?;
        for stack in &mut stacks {
            self.enrich_metadata(stack);
        }
        Ok(stacks)
    }

    fn get_stack(&self, id: &str) -> Result<PhotoStack, RepositoryError> {
        let stacks = scanner::scan_directory(&self.root, &self.config)?;
        let mut stack = stacks
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        self.enrich_metadata(&mut stack);
        Ok(stack)
    }

    fn read_image(&self, path: &Path) -> Result<Vec<u8>, RepositoryError> {
        Ok(std::fs::read(path)?)
    }

    fn write_metadata(&self, stack: &PhotoStack, tags: &Metadata) -> Result<(), RepositoryError> {
        // Write XMP tags to the image file (preferred method for photo app compatibility)
        if !tags.xmp_tags.is_empty() {
            // Prefer enhanced image, fall back to original
            let target = stack.enhanced.as_ref().or(stack.original.as_ref());
            if let Some(path) = target {
                // Write XMP - if it fails, log warning but don't fail the operation
                if let Err(e) = xmp::write_xmp(path, &tags.xmp_tags) {
                    eprintln!(
                        "Warning: Failed to write XMP to {}: {}. Falling back to sidecar storage.",
                        path.display(),
                        e
                    );
                    // Fall back to sidecar DB for XMP tags
                    let db = SidecarDb::open(&self.root)
                        .map_err(|e| RepositoryError::Other(e.to_string()))?;
                    for (key, value) in &tags.xmp_tags {
                        let prefixed_key = format!("xmp:{key}");
                        db.set_tag(&stack.id, &prefixed_key, &serde_json::Value::String(value.clone()))
                            .map_err(|e| RepositoryError::Other(e.to_string()))?;
                    }
                }
            }
        }

        // Write custom tags to sidecar DB
        if !tags.custom_tags.is_empty() {
            let db = SidecarDb::open(&self.root)
                .map_err(|e| RepositoryError::Other(e.to_string()))?;
            db.set_tags(&stack.id, &tags.custom_tags)
                .map_err(|e| RepositoryError::Other(e.to_string()))?;
        }

        // Store EXIF tags in sidecar DB (EXIF writing to files is complex and risky)
        // This preserves user-provided EXIF values without modifying original EXIF data
        if !tags.exif_tags.is_empty() {
            let db = SidecarDb::open(&self.root)
                .map_err(|e| RepositoryError::Other(e.to_string()))?;
            for (key, value) in &tags.exif_tags {
                let prefixed_key = format!("exif:{key}");
                db.set_tag(&stack.id, &prefixed_key, &serde_json::Value::String(value.clone()))
                    .map_err(|e| RepositoryError::Other(e.to_string()))?;
            }
        }

        Ok(())
    }
}
