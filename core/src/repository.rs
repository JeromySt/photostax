use std::path::Path;

use crate::photo_stack::{Metadata, PhotoStack};

/// Errors that can occur when interacting with a photo repository.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Photo stack not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

/// Abstraction over a storage backend containing Epson FastFoto photo stacks.
///
/// Implementations exist for local filesystem access, with cloud storage
/// backends (OneDrive, Google Drive) planned for future releases.
pub trait Repository {
    /// Scan the repository and return all discovered photo stacks.
    fn scan(&self) -> Result<Vec<PhotoStack>, RepositoryError>;

    /// Retrieve a single photo stack by its ID.
    fn get_stack(&self, id: &str) -> Result<PhotoStack, RepositoryError>;

    /// Read the raw bytes of an image file within the repository.
    fn read_image(&self, path: &Path) -> Result<Vec<u8>, RepositoryError>;

    /// Write metadata tags to the files in a photo stack.
    fn write_metadata(&self, stack: &PhotoStack, tags: &Metadata) -> Result<(), RepositoryError>;
}
