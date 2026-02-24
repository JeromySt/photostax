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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_error_io_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let repo_err = RepositoryError::Io(io_err);
        let display = format!("{}", repo_err);
        assert!(display.contains("I/O error"));
        assert!(display.contains("file not found"));
    }

    #[test]
    fn test_repository_error_not_found_display() {
        let err = RepositoryError::NotFound("IMG_001".to_string());
        let display = format!("{}", err);
        assert!(display.contains("Photo stack not found"));
        assert!(display.contains("IMG_001"));
    }

    #[test]
    fn test_repository_error_other_display() {
        let err = RepositoryError::Other("something went wrong".to_string());
        let display = format!("{}", err);
        assert!(display.contains("something went wrong"));
    }

    #[test]
    fn test_repository_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let repo_err: RepositoryError = io_err.into();
        
        match repo_err {
            RepositoryError::Io(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
            }
            _ => panic!("Expected Io variant"),
        }
    }

    #[test]
    fn test_repository_error_debug() {
        let err = RepositoryError::NotFound("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotFound"));
    }
}
