use std::path::{Path, PathBuf};

use crate::metadata::exif;
use crate::metadata::sidecar::SidecarDb;
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

    /// Enrich a PhotoStack with EXIF and sidecar metadata.
    fn enrich_metadata(&self, stack: &mut PhotoStack) {
        stack.metadata.exif_tags = self.load_exif_tags(stack);
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
        // Write custom tags to sidecar DB
        if !tags.custom_tags.is_empty() {
            let db = SidecarDb::open(&self.root)
                .map_err(|e| RepositoryError::Other(e.to_string()))?;
            db.set_tags(&stack.id, &tags.custom_tags)
                .map_err(|e| RepositoryError::Other(e.to_string()))?;
        }

        // Note: EXIF writing to JPEG files is a future enhancement.
        // For now, EXIF tags in the Metadata are stored in the sidecar DB
        // alongside custom tags to preserve the original files.
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
