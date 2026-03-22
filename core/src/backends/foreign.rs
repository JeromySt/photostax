//! Foreign repository backend for host-language-provided I/O.
//!
//! This module enables host languages (.NET, TypeScript, etc.) to implement
//! custom storage backends (OneDrive, Google Drive, Azure Blob) by providing
//! I/O primitives via the [`RepositoryProvider`] trait. The Rust core handles
//! scanning, naming convention parsing, metadata loading, and all business logic.
//!
//! ## Architecture
//!
//! ```text
//! Host Language                           Rust Core
//! ┌─────────────────────┐    trait    ┌──────────────────────┐
//! │ MyCloudProvider      │ ────────── │  ForeignRepository   │
//! │   list_entries()     │  impl of   │    impl Repository   │
//! │   open_read()        │  Provider  │    impl FileAccess   │
//! │   open_write()       │            │                      │
//! │   location()         │            │  Uses scanner module │
//! └─────────────────────┘            └──────────────────────┘
//! ```
//!
//! ## Example
//!
//! ```rust,no_run
//! use photostax_core::backends::foreign::{ForeignRepository, RepositoryProvider};
//! use photostax_core::scanner::FileEntry;
//! use photostax_core::file_access::ReadSeek;
//! use std::io::{self, Write};
//!
//! struct MyCloudProvider;
//!
//! impl RepositoryProvider for MyCloudProvider {
//!     fn location(&self) -> &str { "cloud://my-bucket" }
//!     fn list_entries(&self, prefix: &str, recursive: bool) -> io::Result<Vec<FileEntry>> {
//!         // List objects from cloud API
//!         Ok(vec![])
//!     }
//!     fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
//!         todo!("Download object as seekable stream")
//!     }
//!     fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
//!         todo!("Upload object via streaming write")
//!     }
//! }
//!
//! let repo = ForeignRepository::new(Box::new(MyCloudProvider));
//! ```

use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::classifier::ImageClassifier;
use crate::events::{RepoEvent, StackEvent};
use crate::file_access::{FileAccess, ReadSeek};
use crate::photo_stack::{PhotoStack, ScanPhase, ScanProgress, ScannerProfile};
use crate::repository::{Repository, RepositoryError};
use crate::scanner::{self, FileEntry, ScannerConfig};

/// Trait for host-language-provided I/O primitives.
///
/// Implement this trait to create a custom storage backend. The implementor
/// provides file listing and streaming I/O; the Rust core handles all
/// scanning logic, naming conventions, and metadata operations.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` since the repository may be accessed
/// from multiple threads (e.g., background scan + UI query).
pub trait RepositoryProvider: Send + Sync {
    /// Returns the canonical URI of this repository.
    ///
    /// Must be stable across calls and unique among repositories.
    /// Examples: `"onedrive://user/Photos"`, `"gdrive://folder-id"`,
    /// `"azure://account/container/path"`.
    fn location(&self) -> &str;

    /// List file entries under the given prefix.
    ///
    /// # Arguments
    ///
    /// * `prefix` - The folder path to list (empty string for root).
    ///   Uses forward slashes as separators.
    /// * `recursive` - If `true`, list all files in all subdirectories.
    ///   If `false`, list only immediate children.
    ///
    /// # Returns
    ///
    /// A vector of [`FileEntry`] objects representing files (not directories).
    /// Each entry's `folder` field should contain the relative path from the
    /// repository root using forward slashes.
    fn list_entries(&self, prefix: &str, recursive: bool) -> io::Result<Vec<FileEntry>>;

    /// Open a file for shared concurrent reading.
    ///
    /// Returns a seekable reader. Multiple concurrent readers should be supported.
    fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>>;

    /// Open a file for exclusive writing.
    ///
    /// Returns a writer. Only one writer should be active for a given path.
    fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>>;
}

/// A repository backed by a host-language-provided [`RepositoryProvider`].
///
/// This struct bridges the FFI boundary: the host language implements
/// `RepositoryProvider` (via callbacks or interface), and `ForeignRepository`
/// wraps it into a full `Repository + FileAccess` implementation that the
/// Rust `StackManager` can use like any other backend.
pub struct ForeignRepository {
    provider: Box<dyn RepositoryProvider>,
    config: ScannerConfig,
    repo_id: String,
    generation: AtomicU64,
    classifier: Option<Arc<dyn ImageClassifier>>,
}

impl ForeignRepository {
    /// Create a new foreign repository from a provider.
    pub fn new(provider: Box<dyn RepositoryProvider>) -> Self {
        let repo_id = crate::hashing::make_stack_id(provider.location(), "", "");
        Self {
            provider,
            config: ScannerConfig::default(),
            repo_id,
            generation: AtomicU64::new(0),
            classifier: None,
        }
    }

    /// Create a new foreign repository with a custom scanner configuration.
    pub fn with_config(provider: Box<dyn RepositoryProvider>, config: ScannerConfig) -> Self {
        let repo_id = crate::hashing::make_stack_id(provider.location(), "", "");
        Self {
            provider,
            config,
            repo_id,
            generation: AtomicU64::new(0),
            classifier: None,
        }
    }
}

impl FileAccess for ForeignRepository {
    fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
        self.provider.open_read(path)
    }

    fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
        self.provider.open_write(path)
    }
}

impl Repository for ForeignRepository {
    fn location(&self) -> &str {
        self.provider.location()
    }

    fn id(&self) -> &str {
        &self.repo_id
    }

    fn scan_with_progress(
        &self,
        profile: ScannerProfile,
        mut progress: Option<&mut dyn FnMut(&ScanProgress)>,
    ) -> Result<Vec<PhotoStack>, RepositoryError> {
        // List entries from the provider
        let entries = self.provider.list_entries("", self.config.recursive)?;

        // Use the abstract scanner to group into stacks
        let mut stacks = scanner::scan_entries(&entries, &self.config, self.provider.location());

        let stack_count = stacks.len();
        for (i, stack) in stacks.iter_mut().enumerate() {
            {
                let mut inner = stack.inner.write().unwrap();
                inner.location = inner.folder.clone();
            }

            if let Some(ref mut cb) = progress {
                cb(&ScanProgress {
                    repo_id: self.repo_id.clone(),
                    phase: ScanPhase::Scanning,
                    current: i + 1,
                    total: stack_count,
                });
            }
        }

        // Classification pass for Auto profile
        if profile.needs_classification() {
            let ambiguous_indices: Vec<usize> = stacks
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    let inner = s.inner.read().unwrap();
                    inner.enhanced.is_present() && !inner.back.is_present()
                })
                .map(|(i, _)| i)
                .collect();

            let total = ambiguous_indices.len();
            for (step, idx) in ambiguous_indices.into_iter().enumerate() {
                crate::classify::classify_ambiguous(&mut stacks[idx])?;
                if let Some(ref mut cb) = progress {
                    cb(&ScanProgress {
                        repo_id: self.repo_id.clone(),
                        phase: ScanPhase::Classifying,
                        current: step + 1,
                        total,
                    });
                }
            }
        }

        if let Some(ref mut cb) = progress {
            cb(&ScanProgress {
                repo_id: self.repo_id.clone(),
                phase: ScanPhase::Complete,
                current: stacks.len(),
                total: stacks.len(),
            });
        }

        Ok(stacks)
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn set_classifier(&mut self, classifier: Arc<dyn ImageClassifier>) {
        self.classifier = Some(classifier);
    }

    fn subscribe(&self) -> Result<std::sync::mpsc::Receiver<RepoEvent>, RepositoryError> {
        let (_tx, rx) = std::sync::mpsc::channel();
        Ok(rx)
    }

    fn watch(&self) -> Result<std::sync::mpsc::Receiver<StackEvent>, RepositoryError> {
        // Foreign repos don't support filesystem watching by default.
        // Host languages can implement their own polling/notification mechanism.
        let (_tx, rx) = std::sync::mpsc::channel();
        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{Cursor, Read};
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// In-memory mock provider for testing.
    struct MockProvider {
        location: String,
        files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    impl MockProvider {
        fn new(location: &str) -> Self {
            Self {
                location: location.to_string(),
                files: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn add_file(&self, path: &str, content: &[u8]) {
            self.files
                .lock()
                .unwrap()
                .insert(path.to_string(), content.to_vec());
        }
    }

    impl RepositoryProvider for MockProvider {
        fn location(&self) -> &str {
            &self.location
        }

        fn list_entries(&self, _prefix: &str, _recursive: bool) -> io::Result<Vec<FileEntry>> {
            let files = self.files.lock().unwrap();
            let entries: Vec<FileEntry> = files
                .iter()
                .map(|(path, content)| {
                    let name = Path::new(path)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string();
                    let folder = Path::new(path)
                        .parent()
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_default();
                    FileEntry {
                        name,
                        folder,
                        path: path.clone(),
                        size: content.len() as u64,
                    }
                })
                .collect();
            Ok(entries)
        }

        fn open_read(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
            let files = self.files.lock().unwrap();
            let content = files
                .get(path)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, format!("Not found: {path}"))
                })?
                .clone();
            Ok(Box::new(Cursor::new(content)))
        }

        fn open_write(&self, path: &str) -> io::Result<Box<dyn Write + Send>> {
            let files = self.files.clone();
            let path = path.to_string();
            Ok(Box::new(MockWriter {
                path,
                buffer: Vec::new(),
                files,
            }))
        }
    }

    struct MockWriter {
        path: String,
        buffer: Vec<u8>,
        files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    impl Write for MockWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.buffer.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Drop for MockWriter {
        fn drop(&mut self) {
            let mut files = self.files.lock().unwrap();
            files.insert(self.path.clone(), std::mem::take(&mut self.buffer));
        }
    }

    #[test]
    fn test_foreign_repo_location_and_id() {
        let provider = MockProvider::new("cloud://test-bucket");
        let repo = ForeignRepository::new(Box::new(provider));
        assert_eq!(repo.location(), "cloud://test-bucket");
        assert_eq!(repo.id().len(), 16); // truncated SHA-256
    }

    #[test]
    fn test_foreign_repo_scan_empty() {
        let provider = MockProvider::new("cloud://empty");
        let repo = ForeignRepository::new(Box::new(provider));
        let stacks = repo.scan().unwrap();
        assert!(stacks.is_empty());
    }

    #[test]
    fn test_foreign_repo_scan_groups_stacks() {
        let provider = MockProvider::new("cloud://photos");
        provider.add_file("IMG_001.jpg", b"original");
        provider.add_file("IMG_001_a.jpg", b"enhanced");
        provider.add_file("IMG_001_b.jpg", b"back");
        provider.add_file("IMG_002.jpg", b"original2");

        let repo = ForeignRepository::new(Box::new(provider));
        let stacks = repo.scan().unwrap();

        assert_eq!(stacks.len(), 2);

        let s1 = stacks.iter().find(|s| s.name() == "IMG_001").unwrap();
        assert!(s1.original().is_present());
        assert!(s1.enhanced().is_present());
        assert!(s1.back().is_present());
        assert_eq!(s1.repo_id().as_deref(), Some("cloud://photos"));

        let s2 = stacks.iter().find(|s| s.name() == "IMG_002").unwrap();
        assert!(s2.original().is_present());
        assert!(!s2.enhanced().is_present());
        assert!(!s2.back().is_present());
    }

    #[test]
    fn test_foreign_repo_file_access() {
        let provider = MockProvider::new("cloud://test");
        provider.add_file("test.txt", b"hello");

        let repo = ForeignRepository::new(Box::new(provider));

        // Test open_read
        let mut reader = FileAccess::open_read(&repo, "test.txt").unwrap();
        let mut content = String::new();
        reader.read_to_string(&mut content).unwrap();
        assert_eq!(content, "hello");

        // Test open_write
        let mut writer = FileAccess::open_write(&repo, "new.txt").unwrap();
        writer.write_all(b"written").unwrap();
        drop(writer);

        // Verify write persisted
        let mut reader = FileAccess::open_read(&repo, "new.txt").unwrap();
        let mut content = String::new();
        reader.read_to_string(&mut content).unwrap();
        assert_eq!(content, "written");
    }

    #[test]
    fn test_foreign_repo_hash_file() {
        let provider = MockProvider::new("cloud://test");
        provider.add_file("data.bin", b"deterministic content");

        let repo = ForeignRepository::new(Box::new(provider));
        let hash1 = FileAccess::hash_file(&repo, "data.bin").unwrap();
        let hash2 = FileAccess::hash_file(&repo, "data.bin").unwrap();
        assert_eq!(hash1.len(), 16);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_foreign_repo_scan_with_progress() {
        let provider = MockProvider::new("cloud://test");
        provider.add_file("IMG_001.jpg", b"data");
        provider.add_file("IMG_002.jpg", b"data2");

        let repo = ForeignRepository::new(Box::new(provider));

        let mut progress_updates = Vec::new();
        let stacks = repo
            .scan_with_progress(
                ScannerProfile::EnhancedAndBack,
                Some(&mut |p: &ScanProgress| {
                    progress_updates.push((p.phase, p.current, p.total));
                }),
            )
            .unwrap();

        assert_eq!(stacks.len(), 2);
        // Should have scanning updates + complete
        assert!(!progress_updates.is_empty());
        let last = progress_updates.last().unwrap();
        assert_eq!(last.0, ScanPhase::Complete);
    }

    #[test]
    fn test_foreign_repo_with_subfolder() {
        let provider = MockProvider::new("cloud://test");
        provider.add_file("1984_Mexico/IMG_001.jpg", b"data");

        // Manually set up entry with folder
        struct SubfolderProvider;
        impl RepositoryProvider for SubfolderProvider {
            fn location(&self) -> &str {
                "cloud://subfolder-test"
            }
            fn list_entries(&self, _prefix: &str, _recursive: bool) -> io::Result<Vec<FileEntry>> {
                Ok(vec![FileEntry {
                    name: "IMG_001.jpg".to_string(),
                    folder: "1984_Mexico".to_string(),
                    path: "1984_Mexico/IMG_001.jpg".to_string(),
                    size: 4,
                }])
            }
            fn open_read(&self, _path: &str) -> io::Result<Box<dyn ReadSeek>> {
                Ok(Box::new(Cursor::new(b"data".to_vec())))
            }
            fn open_write(&self, _path: &str) -> io::Result<Box<dyn Write + Send>> {
                Ok(Box::new(io::sink()))
            }
        }

        let repo = ForeignRepository::new(Box::new(SubfolderProvider));
        let stacks = repo.scan().unwrap();

        assert_eq!(stacks.len(), 1);
        let stack = &stacks[0];
        assert_eq!(stack.name(), "IMG_001");
        assert_eq!(stack.folder().as_deref(), Some("1984_Mexico"));
        // Folder is stored for downstream metadata loading
        assert_eq!(stack.location().as_deref(), Some("1984_Mexico"));
    }

    #[test]
    fn test_foreign_repo_watch_returns_empty_receiver() {
        let provider = MockProvider::new("cloud://test");
        let repo = ForeignRepository::new(Box::new(provider));
        let rx = repo.watch().unwrap();
        // Should not receive any events
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_foreign_repo_with_custom_config() {
        let provider = MockProvider::new("cloud://test");
        provider.add_file("IMG_001_enhanced.jpg", b"data");

        let config = ScannerConfig {
            enhanced_suffix: "_enhanced".to_string(),
            ..ScannerConfig::default()
        };
        let repo = ForeignRepository::with_config(Box::new(provider), config);
        let stacks = repo.scan().unwrap();

        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name(), "IMG_001");
        assert!(stacks[0].enhanced().is_present());
    }

    #[test]
    fn test_foreign_repo_generation_and_classifier() {
        let provider = MockProvider::new("cloud://test");
        let mut repo = ForeignRepository::new(Box::new(provider));
        assert_eq!(repo.generation(), 0);

        let classifier: Arc<dyn crate::classifier::ImageClassifier> =
            Arc::new(crate::classifier::DefaultClassifier);
        repo.set_classifier(classifier);
        assert!(repo.classifier.is_some());
    }

    #[test]
    fn test_foreign_repo_subscribe() {
        let provider = MockProvider::new("cloud://test");
        let repo = ForeignRepository::new(Box::new(provider));
        let rx = repo.subscribe().unwrap();
        assert!(rx.try_recv().is_err());
    }
}
