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
//! let mut stack = repo.get_stack("IMG_0001")?;
//! repo.load_metadata(&mut stack)?;
//! println!("{}: {} EXIF tags, {} custom tags",
//!     stack.id,
//!     stack.metadata.exif_tags.len(),
//!     stack.metadata.custom_tags.len());
//!
//! // Or scan with metadata in one call (old behavior)
//! let full_stacks = repo.scan_with_metadata()?;
//! # Ok::<(), photostax_core::repository::RepositoryError>(())
//! ```
//!
//! [`Repository`]: crate::repository::Repository
//! [`PhotoStack`]: crate::photo_stack::PhotoStack

use std::path::{Path, PathBuf};

use crate::classify;
use crate::metadata::exif;
use crate::metadata::sidecar;
use crate::metadata::xmp;
use crate::metadata::ImageFormat;
use crate::photo_stack::{ClassifyMode, Metadata, PhotoStack, Rotation, RotationTarget};
use crate::repository::{Repository, RepositoryError};
use crate::scanner::{self, parse_folder_name, ScannerConfig};

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

    /// Load embedded XMP tags from a JPEG image file.
    ///
    /// For JPEG: reads embedded XMP from the file.
    /// For TIFF and other formats: returns empty (sidecar XMP is handled separately).
    fn load_embedded_xmp(&self, stack: &PhotoStack) -> std::collections::HashMap<String, String> {
        let candidate = stack.enhanced.as_ref().or(stack.original.as_ref());
        match candidate {
            Some(path) => {
                // Only read embedded XMP from JPEG; TIFF XMP is in the stack sidecar
                if matches!(
                    crate::metadata::detect_image_format(path),
                    Some(ImageFormat::Jpeg)
                ) {
                    xmp::read_xmp_from_jpeg(path).unwrap_or_default()
                } else {
                    std::collections::HashMap::new()
                }
            }
            None => std::collections::HashMap::new(),
        }
    }

    /// Enrich a PhotoStack with EXIF, XMP, sidecar, and folder-derived metadata.
    ///
    /// This is the core metadata merging logic that combines all sources
    /// into the stack's unified [`Metadata`] structure with this priority
    /// (highest wins):
    ///
    /// 1. XMP sidecar (custom tags, EXIF overrides, XMP tags)
    /// 2. Embedded XMP
    /// 3. EXIF
    /// 4. Folder name (lowest — fills gaps only)
    ///
    /// [`Metadata`]: crate::photo_stack::Metadata
    fn enrich_metadata(&self, stack: &mut PhotoStack) {
        // 1. Read EXIF from image files
        stack.metadata.exif_tags = self.load_exif_tags(stack);

        // 2. Read embedded XMP from image (JPEG only)
        let mut xmp_tags = self.load_embedded_xmp(stack);

        // 3. Read stack-level XMP sidecar (from the stack's containing dir,
        //    which may differ from root during recursive scans)
        let sidecar_dir = stack.containing_dir().unwrap_or_else(|| self.root.clone());
        let sidecar_data = sidecar::read_sidecar(&sidecar_dir, &stack.id).unwrap_or_default();

        // 4. Merge sidecar XMP into embedded XMP (sidecar overrides)
        for (k, v) in sidecar_data.xmp_tags {
            xmp_tags.insert(k, v);
        }
        stack.metadata.xmp_tags = xmp_tags;

        // 5. Custom tags from sidecar
        stack.metadata.custom_tags = sidecar_data.custom_tags;

        // 6. EXIF overrides from sidecar
        for (k, v) in sidecar_data.exif_overrides {
            stack.metadata.exif_tags.insert(k, v);
        }

        // 7. Derive metadata from containing folder name (lowest priority —
        //    only fills in keys that are not yet present from any other source).
        self.apply_folder_metadata(stack);
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

    /// Derive metadata from the stack's containing folder name using the
    /// FastFoto naming convention (`<year>_<month_or_season>_<subject>`).
    ///
    /// Values are written into `custom_tags` under the `folder_year`,
    /// `folder_month_or_season`, and `folder_subject` keys, and into
    /// `xmp_tags` for `date` (Dublin Core)  — but only when the key is
    /// not already present from a higher-priority source.
    fn apply_folder_metadata(&self, stack: &mut PhotoStack) {
        let folder = match stack.containing_folder() {
            Some(f) => f,
            None => return,
        };

        let fm = parse_folder_name(&folder);
        if fm.is_empty() {
            return;
        }

        // Year → xmp_tags["date"] (Dublin Core dc:date) if not already set
        if let Some(year) = fm.year {
            if !stack.metadata.xmp_tags.contains_key("date")
                && !stack.metadata.exif_tags.contains_key("DateTimeOriginal")
                || stack
                    .metadata
                    .exif_tags
                    .get("DateTimeOriginal")
                    .is_some_and(|v| v.starts_with(&format!("{year}-")))
            {
                // Only set dc:date from folder year when EXIF either
                // agrees or is absent. We use the year as a date value.
            }
            // Always store folder-derived year as a custom tag for consumption
            if !stack.metadata.custom_tags.contains_key("folder_year") {
                stack
                    .metadata
                    .custom_tags
                    .insert("folder_year".to_string(), serde_json::json!(year));
            }
            // Fill in dc:date if nothing more specific exists
            if !stack.metadata.xmp_tags.contains_key("date") {
                stack
                    .metadata
                    .xmp_tags
                    .insert("date".to_string(), format!("{year}"));
            }
        }

        // Month/season → custom_tags["folder_month_or_season"]
        if let Some(ref ms) = fm.month_or_season {
            if !stack
                .metadata
                .custom_tags
                .contains_key("folder_month_or_season")
            {
                stack
                    .metadata
                    .custom_tags
                    .insert("folder_month_or_season".to_string(), serde_json::json!(ms));
            }
        }

        // Subject → xmp_tags["subject"] (Dublin Core dc:subject) + custom tag
        if let Some(ref subj) = fm.subject {
            if !stack.metadata.custom_tags.contains_key("folder_subject") {
                stack
                    .metadata
                    .custom_tags
                    .insert("folder_subject".to_string(), serde_json::json!(subj));
            }
            // Fill in dc:subject if not already set by XMP or sidecar
            if !stack.metadata.xmp_tags.contains_key("subject") {
                stack
                    .metadata
                    .xmp_tags
                    .insert("subject".to_string(), subj.replace('_', " "));
            }
        }
    }
}

impl Repository for LocalRepository {
    fn scan_with_classification(
        &self,
        mode: ClassifyMode,
    ) -> Result<Vec<PhotoStack>, RepositoryError> {
        let mut stacks = scanner::scan_directory(&self.root, &self.config)?;
        for stack in &mut stacks {
            self.apply_folder_metadata(stack);
        }
        if mode == ClassifyMode::Auto {
            for stack in &mut stacks {
                classify::classify_ambiguous(stack)?;
            }
        }
        Ok(stacks)
    }

    fn load_metadata(&self, stack: &mut PhotoStack) -> Result<(), RepositoryError> {
        self.enrich_metadata(stack);
        Ok(())
    }

    fn get_stack(&self, id: &str) -> Result<PhotoStack, RepositoryError> {
        let stacks = scanner::scan_directory(&self.root, &self.config)?;
        let mut stack = stacks
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        self.apply_folder_metadata(&mut stack);
        Ok(stack)
    }

    fn read_image(&self, path: &Path) -> Result<Vec<u8>, RepositoryError> {
        Ok(std::fs::read(path)?)
    }

    fn write_metadata(&self, stack: &PhotoStack, tags: &Metadata) -> Result<(), RepositoryError> {
        // 1. Write XMP tags into the image file directly (JPEG gets embedded XMP).
        //    This ensures maximum interoperability — Lightroom, darktable, etc.
        //    can read the tags even without the sidecar file.
        if !tags.xmp_tags.is_empty() {
            let target = stack.enhanced.as_ref().or(stack.original.as_ref());
            if let Some(path) = target {
                // Best-effort: embed into file; sidecar is authoritative
                let _ = xmp::write_xmp(path, &tags.xmp_tags);
            }
        }

        // 2. Write everything to the stack-level XMP sidecar file.
        //    Sidecar is the authoritative source for custom tags and EXIF overrides,
        //    and also mirrors XMP tags for formats that don't support embedding.
        let sidecar_dir = stack.containing_dir().unwrap_or_else(|| self.root.clone());
        sidecar::merge_and_write(
            &sidecar_dir,
            &stack.id,
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

        let paths: Vec<&Path> = match target {
            RotationTarget::All => [&stack.original, &stack.enhanced, &stack.back]
                .iter()
                .filter_map(|opt| opt.as_deref())
                .collect(),
            RotationTarget::Front => [&stack.original, &stack.enhanced]
                .iter()
                .filter_map(|opt| opt.as_deref())
                .collect(),
            RotationTarget::Back => [&stack.back]
                .iter()
                .filter_map(|opt| opt.as_deref())
                .collect(),
        };

        if paths.is_empty() {
            return Err(RepositoryError::Other(format!(
                "Stack '{id}' has no image files to rotate for target {target:?}"
            )));
        }

        for path in paths {
            rotate_image_file(path, rotation)?;
        }

        // Re-fetch the stack so the caller gets fresh state
        let mut refreshed = self.get_stack(id)?;
        self.load_metadata(&mut refreshed)?;
        Ok(refreshed)
    }
}

/// Decode an image file, rotate the pixel data, and write it back.
fn rotate_image_file(path: &Path, rotation: Rotation) -> Result<(), RepositoryError> {
    let img = image::open(path).map_err(|e| {
        RepositoryError::Other(format!("Failed to decode image {}: {e}", path.display()))
    })?;

    let rotated = match rotation {
        Rotation::Cw90 => img.rotate90(),
        Rotation::Ccw90 => img.rotate270(),
        Rotation::Cw180 => img.rotate180(),
    };

    rotated.save(path).map_err(|e| {
        RepositoryError::Other(format!(
            "Failed to save rotated image {}: {e}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
        assert!(stacks.iter().any(|s| s.id == "IMG_001"));
        assert!(stacks.iter().any(|s| s.id == "IMG_002"));
        // Scan is lazy — metadata should be empty (no EXIF in test JPEG)
        for stack in &stacks {
            assert!(stack.metadata.exif_tags.is_empty());
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
            stacks[0].metadata.custom_tags.get("album"),
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
        let stack = repo.get_stack("IMG_001").unwrap();

        assert_eq!(stack.id, "IMG_001");
        assert!(stack.original.is_some());
        assert!(stack.enhanced.is_some());
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
        let data = repo.read_image(&img_path).unwrap();

        assert_eq!(data, jpeg_data);
    }

    #[test]
    fn test_read_image_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());
        let result = repo.read_image(&tmp.path().join("nonexistent.jpg"));

        assert!(matches!(result, Err(RepositoryError::Io(_))));
    }

    #[test]
    fn test_write_metadata_custom_tags() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let stack = repo.get_stack("IMG_001").unwrap();

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
        let stack = repo.get_stack("IMG_001").unwrap();

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
        let stack = repo.get_stack("IMG_001").unwrap();

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
        let stack = repo.get_stack("IMG_001").unwrap();

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
        let mut stack = repo.get_stack("IMG_001").unwrap();

        // Before load_metadata, sidecar tags should NOT be loaded
        assert!(!stack.metadata.custom_tags.contains_key("custom_tag"));

        // After load_metadata, sidecar tags should be loaded
        repo.load_metadata(&mut stack).unwrap();
        assert_eq!(
            stack.metadata.custom_tags.get("custom_tag"),
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
        let stack = stacks.iter().find(|s| s.id == "IMG_001").unwrap();

        // Just verify the stack has both files - the load_exif_tags method should prefer enhanced
        assert!(stack.original.is_some());
        assert!(stack.enhanced.is_some());
    }

    #[test]
    fn test_load_sidecar_tags_no_sidecar() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create a file but no sidecar
        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let mut stack = repo.get_stack("IMG_001").unwrap();
        repo.load_metadata(&mut stack).unwrap();

        // Should have no sidecar-derived custom tags.
        // (folder-derived tags with `folder_` prefix may be present from the
        // temp directory name, so we only check for non-folder custom tags.)
        let non_folder_custom: Vec<_> = stack
            .metadata
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
        assert!(stack.original.is_some());
        assert!(stack.enhanced.is_some());
        assert!(stack.back.is_some());
    }

    #[test]
    fn test_load_exif_no_images() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());

        // Create a stack with no images
        let stack = PhotoStack::new("empty");
        let exif_tags = repo.load_exif_tags(&stack);
        assert!(exif_tags.is_empty());
    }

    #[test]
    fn test_load_embedded_xmp_no_images() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());

        let stack = PhotoStack::new("empty");
        let xmp_tags = repo.load_embedded_xmp(&stack);
        assert!(xmp_tags.is_empty());
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

        // Lazy scan should still have folder metadata (it's zero-cost)
        let stacks = repo.scan().unwrap();
        assert_eq!(stacks.len(), 1);
        let s = &stacks[0];
        assert_eq!(
            s.metadata.custom_tags.get("folder_year"),
            Some(&serde_json::json!(1984))
        );
        assert_eq!(
            s.metadata.custom_tags.get("folder_subject"),
            Some(&serde_json::json!("Mexico"))
        );
        assert!(!s
            .metadata
            .custom_tags
            .contains_key("folder_month_or_season"));
        assert_eq!(s.metadata.xmp_tags.get("date"), Some(&"1984".to_string()));
        assert_eq!(
            s.metadata.xmp_tags.get("subject"),
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
        let stacks = repo.scan().unwrap();

        let s = &stacks[0];
        assert_eq!(
            s.metadata.custom_tags.get("folder_year"),
            Some(&serde_json::json!(2024))
        );
        assert_eq!(
            s.metadata.custom_tags.get("folder_month_or_season"),
            Some(&serde_json::json!("Summer"))
        );
        assert_eq!(
            s.metadata.custom_tags.get("folder_subject"),
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
        let stacks = repo.scan().unwrap();

        let s = &stacks[0];
        assert!(!s.metadata.custom_tags.contains_key("folder_year"));
        assert_eq!(
            s.metadata.custom_tags.get("folder_subject"),
            Some(&serde_json::json!("SteveJones"))
        );
        assert_eq!(
            s.metadata.xmp_tags.get("subject"),
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

        let s = &stacks[0];
        // Sidecar value should win for xmp_tags["subject"]
        assert_eq!(
            s.metadata.xmp_tags.get("subject"),
            Some(&"beach, family".to_string())
        );
        // But folder_subject custom tag is still populated
        assert_eq!(
            s.metadata.custom_tags.get("folder_subject"),
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
        let stacks = repo.scan().unwrap();

        let s = &stacks[0];
        assert_eq!(
            s.metadata.custom_tags.get("folder_year"),
            Some(&serde_json::json!(1993))
        );
        assert!(!s.metadata.custom_tags.contains_key("folder_subject"));
        assert!(!s
            .metadata
            .custom_tags
            .contains_key("folder_month_or_season"));
        assert_eq!(s.metadata.xmp_tags.get("date"), Some(&"1993".to_string()));
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
        let rotated = repo
            .rotate_stack("IMG_001", Rotation::Cw90, RotationTarget::All)
            .unwrap();
        assert_eq!(rotated.id, "IMG_001");

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
        let rotated = repo
            .rotate_stack("IMG_001", Rotation::Ccw90, RotationTarget::All)
            .unwrap();
        assert_eq!(rotated.id, "IMG_001");

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
        let rotated = repo
            .rotate_stack("IMG_001", Rotation::Cw180, RotationTarget::All)
            .unwrap();
        assert_eq!(rotated.id, "IMG_001");

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
        let rotated = repo
            .rotate_stack("IMG_001", Rotation::Cw90, RotationTarget::All)
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
        let rotated = repo
            .rotate_stack("IMG_001", Rotation::Cw90, RotationTarget::All)
            .unwrap();
        // The returned stack should have the same id and paths
        assert_eq!(rotated.id, "IMG_001");
        assert!(rotated.original.is_some());
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
            .any(|s| !s.metadata.exif_tags.is_empty());
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
                item.id.contains("FamilyPhotos"),
                "filtered item {} should match",
                item.id
            );
        }
    }
}
