use std::path::{Path, PathBuf};

use crate::metadata::exif;
use crate::metadata::sidecar::SidecarDb;
use crate::metadata::xmp;
use crate::photo_stack::{Metadata, PhotoStack};
use crate::repository::{Repository, RepositoryError};
use crate::scanner::{self, ScannerConfig};

/// A repository backed by a local filesystem directory.
pub struct LocalRepository {
    root: PathBuf,
    config: ScannerConfig,
}

impl LocalRepository {
    /// Create a new `LocalRepository` rooted at the given directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            config: ScannerConfig::default(),
        }
    }

    /// Create a new `LocalRepository` with a custom scanner configuration.
    pub fn with_config(root: impl Into<PathBuf>, config: ScannerConfig) -> Self {
        Self {
            root: root.into(),
            config,
        }
    }

    /// Returns the root directory of this repository.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Load EXIF tags from the best available image in the stack.
    /// Prefers the enhanced image, falls back to original.
    fn load_exif_tags(&self, stack: &PhotoStack) -> std::collections::HashMap<String, String> {
        let candidate = stack.enhanced.as_ref().or(stack.original.as_ref());
        match candidate {
            Some(path) => exif::read_exif_tags(path).unwrap_or_default(),
            None => std::collections::HashMap::new(),
        }
    }

    /// Load XMP tags from the best available image in the stack.
    /// Prefers the enhanced image, falls back to original.
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
        jpeg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00]); // SOF0
        jpeg.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x1F, 0x00]); // DHT
        jpeg.extend_from_slice(&[0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        jpeg.extend_from_slice(&[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B]);
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
        metadata.custom_tags.insert("ocr_text".to_string(), serde_json::json!("Hello World"));
        metadata.custom_tags.insert("processed".to_string(), serde_json::json!(true));

        repo.write_metadata(&stack, &metadata).unwrap();

        // Verify tags were written by reading them back
        let db = SidecarDb::open(dir).unwrap();
        let tags = db.get_tags("IMG_001").unwrap();
        assert_eq!(tags.get("ocr_text"), Some(&serde_json::json!("Hello World")));
        assert_eq!(tags.get("processed"), Some(&serde_json::json!(true)));
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
        metadata.xmp_tags.insert("description".to_string(), "Test description".to_string());

        repo.write_metadata(&stack, &metadata).unwrap();

        // Verify XMP was written by reading it back
        let xmp_tags = xmp::read_xmp(&stack.original.unwrap()).unwrap();
        assert_eq!(xmp_tags.get("description"), Some(&"Test description".to_string()));
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
        metadata.exif_tags.insert("CustomMake".to_string(), "TestMake".to_string());

        repo.write_metadata(&stack, &metadata).unwrap();

        // EXIF tags are stored in sidecar with exif: prefix
        let db = SidecarDb::open(dir).unwrap();
        let tags = db.get_tags("IMG_001").unwrap();
        assert_eq!(tags.get("exif:CustomMake"), Some(&serde_json::json!("TestMake")));
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
        let db = SidecarDb::open(dir).unwrap();
        db.set_tag("IMG_001", "custom_tag", &serde_json::json!("custom_value")).unwrap();
        drop(db);

        let repo = LocalRepository::new(dir);
        let stack = repo.get_stack("IMG_001").unwrap();

        // Sidecar tags should be loaded
        assert_eq!(stack.metadata.custom_tags.get("custom_tag"), Some(&serde_json::json!("custom_value")));
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
    fn test_load_sidecar_tags_no_db() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create a file but no sidecar DB
        let jpeg_data = create_test_jpeg();
        fs::write(dir.join("IMG_001.jpg"), &jpeg_data).unwrap();

        let repo = LocalRepository::new(dir);
        let stack = repo.get_stack("IMG_001").unwrap();

        // Should return empty custom tags, not error
        assert!(stack.metadata.custom_tags.is_empty());
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
    fn test_load_xmp_no_images() {
        let tmp = TempDir::new().unwrap();
        let repo = LocalRepository::new(tmp.path());
        
        let stack = PhotoStack::new("empty");
        let xmp_tags = repo.load_xmp_tags(&stack);
        assert!(xmp_tags.is_empty());
    }
}
