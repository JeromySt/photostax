use std::path::{Path, PathBuf};

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
}

impl Repository for LocalRepository {
    fn scan(&self) -> Result<Vec<PhotoStack>, RepositoryError> {
        Ok(scanner::scan_directory(&self.root, &self.config)?)
    }

    fn get_stack(&self, id: &str) -> Result<PhotoStack, RepositoryError> {
        let stacks = self.scan()?;
        stacks
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))
    }

    fn read_image(&self, path: &Path) -> Result<Vec<u8>, RepositoryError> {
        Ok(std::fs::read(path)?)
    }

    fn write_metadata(&self, _stack: &PhotoStack, _tags: &Metadata) -> Result<(), RepositoryError> {
        // TODO: implement EXIF/XMP and sidecar DB writes
        Err(RepositoryError::Other(
            "Metadata writing not yet implemented".to_string(),
        ))
    }
}
