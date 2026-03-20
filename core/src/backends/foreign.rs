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
use std::path::Path;

use crate::events::StackEvent;
use crate::file_access::{FileAccess, ReadSeek};
use crate::photo_stack::{
    Metadata, PhotoStack, Rotation, RotationTarget, ScanProgress, ScanPhase,
    ScannerProfile,
};
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
}

impl ForeignRepository {
    /// Create a new foreign repository from a provider.
    pub fn new(provider: Box<dyn RepositoryProvider>) -> Self {
        let repo_id =
            crate::hashing::make_stack_id(provider.location(), "", "");
        Self {
            provider,
            config: ScannerConfig::default(),
            repo_id,
        }
    }

    /// Create a new foreign repository with a custom scanner configuration.
    pub fn with_config(provider: Box<dyn RepositoryProvider>, config: ScannerConfig) -> Self {
        let repo_id =
            crate::hashing::make_stack_id(provider.location(), "", "");
        Self {
            provider,
            config,
            repo_id,
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
            // Apply folder metadata parsing (same as LocalRepository)
            if let Some(ref folder) = stack.folder {
                let last_component = folder.rsplit('/').next().unwrap_or(folder);
                let folder_meta = scanner::parse_folder_name(last_component);
                if let Some(year) = folder_meta.year {
                    stack
                        .metadata
                        .exif_tags
                        .entry("Year".to_string())
                        .or_insert_with(|| year.to_string());
                }
                if let Some(ref mos) = folder_meta.month_or_season {
                    stack
                        .metadata
                        .exif_tags
                        .entry("MonthOrSeason".to_string())
                        .or_insert_with(|| mos.clone());
                }
                if let Some(ref subject) = folder_meta.subject {
                    stack
                        .metadata
                        .exif_tags
                        .entry("Subject".to_string())
                        .or_insert_with(|| subject.clone());
                }
            }

            if let Some(ref mut cb) = progress {
                cb(&ScanProgress {
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
                .filter(|(_, s)| s.enhanced.is_some() && s.back.is_none())
                .map(|(i, _)| i)
                .collect();

            let total = ambiguous_indices.len();
            for (step, idx) in ambiguous_indices.into_iter().enumerate() {
                crate::classify::classify_ambiguous(&mut stacks[idx])?;
                if let Some(ref mut cb) = progress {
                    cb(&ScanProgress {
                        phase: ScanPhase::Classifying,
                        current: step + 1,
                        total,
                    });
                }
            }
        }

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
        // For foreign repos, metadata loading reads from the provider streams.
        // Use the same EXIF/XMP/sidecar pipeline as local, but via open_read().
        let target = stack.enhanced.as_ref().or(stack.original.as_ref());
        if let Some(file) = target {
            if let Ok(reader) = self.open_read(&file.path) {
                let mut buf_reader = io::BufReader::new(reader);
                if let Ok(exif) = exif::Reader::new().read_from_container(&mut buf_reader) {
                    for field in exif.fields() {
                        let tag_name = format!("{}", field.tag);
                        let tag_value = field.display_value().to_string();
                        stack
                            .metadata
                            .exif_tags
                            .entry(tag_name)
                            .or_insert(tag_value);
                    }
                }
            }
        }
        Ok(())
    }

    fn get_stack(&self, id: &str) -> Result<PhotoStack, RepositoryError> {
        let entries = self.provider.list_entries("", self.config.recursive)?;
        let stacks = scanner::scan_entries(&entries, &self.config, self.provider.location());
        stacks
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))
    }

    fn read_image(
        &self,
        path: &str,
    ) -> Result<Box<dyn ReadSeek>, RepositoryError> {
        Ok(self.open_read(path)?)
    }

    fn write_metadata(&self, stack: &PhotoStack, tags: &Metadata) -> Result<(), RepositoryError> {
        // For foreign repos, write to sidecar via the provider's write stream.
        // XMP embedded writes require the full img-parts pipeline which needs
        // local file paths — for foreign repos we only support sidecar writes.
        if !tags.xmp_tags.is_empty() || !tags.custom_tags.is_empty() || !tags.exif_tags.is_empty()
        {
            let sidecar_name = format!("{}.xmp", stack.name);
            let sidecar_path = if let Some(ref folder) = stack.folder {
                format!("{}/{}", folder, sidecar_name)
            } else {
                sidecar_name
            };

            // Build XMP content
            let mut xmp_content = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
                 <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
                 <rdf:Description",
            );

            for (key, value) in &tags.xmp_tags {
                xmp_content.push_str(&format!(" {}=\"{}\"", key, value));
            }
            for (key, value) in &tags.custom_tags {
                xmp_content.push_str(&format!(" custom:{}=\"{}\"", key, value));
            }

            xmp_content.push_str("/>\n</rdf:RDF>\n</x:xmpmeta>\n");

            let mut writer = self
                .open_write(&sidecar_path)
                .map_err(|e| RepositoryError::Other(e.to_string()))?;
            writer
                .write_all(xmp_content.as_bytes())
                .map_err(|e| RepositoryError::Other(e.to_string()))?;
        }
        Ok(())
    }

    fn rotate_stack(
        &self,
        id: &str,
        rotation: Rotation,
        target: RotationTarget,
    ) -> Result<PhotoStack, RepositoryError> {
        let stack = self.get_stack(id)?;

        let paths: Vec<&str> = match target {
            RotationTarget::All => [&stack.original, &stack.enhanced, &stack.back]
                .iter()
                .filter_map(|opt| opt.as_ref().map(|f| f.path.as_str()))
                .collect(),
            RotationTarget::Front => [&stack.original, &stack.enhanced]
                .iter()
                .filter_map(|opt| opt.as_ref().map(|f| f.path.as_str()))
                .collect(),
            RotationTarget::Back => [&stack.back]
                .iter()
                .filter_map(|opt| opt.as_ref().map(|f| f.path.as_str()))
                .collect(),
        };

        if paths.is_empty() {
            return Err(RepositoryError::Other(format!(
                "Stack '{id}' has no image files to rotate for target {target:?}"
            )));
        }

        for path in paths {
            self.rotate_foreign_image(path, rotation)?;
        }

        self.get_stack(id)
    }

    fn watch(&self) -> Result<std::sync::mpsc::Receiver<StackEvent>, RepositoryError> {
        // Foreign repos don't support filesystem watching by default.
        // Host languages can implement their own polling/notification mechanism.
        let (_tx, rx) = std::sync::mpsc::channel();
        Ok(rx)
    }
}

impl ForeignRepository {
    /// Rotate an image file using provider I/O streams.
    fn rotate_foreign_image(&self, path: &str, rotation: Rotation) -> Result<(), RepositoryError> {
        // Read the image via the provider
        let mut reader = self.open_read(path)?;
        let mut buf = Vec::new();
        io::Read::read_to_end(&mut reader, &mut buf)?;
        drop(reader);

        // Determine format and rotate
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let rotated = match ext.as_str() {
            "jpg" | "jpeg" => {
                let img = image::load_from_memory_with_format(&buf, image::ImageFormat::Jpeg)
                    .map_err(|e| RepositoryError::Other(format!("Failed to decode JPEG: {e}")))?;
                let rotated_img = apply_rotation(img, rotation);
                let mut output = Vec::new();
                rotated_img
                    .write_to(
                        &mut io::Cursor::new(&mut output),
                        image::ImageFormat::Jpeg,
                    )
                    .map_err(|e| {
                        RepositoryError::Other(format!("Failed to encode JPEG: {e}"))
                    })?;
                output
            }
            "tif" | "tiff" => {
                let img = image::load_from_memory_with_format(&buf, image::ImageFormat::Tiff)
                    .map_err(|e| RepositoryError::Other(format!("Failed to decode TIFF: {e}")))?;
                let rotated_img = apply_rotation(img, rotation);
                let mut output = Vec::new();
                rotated_img
                    .write_to(
                        &mut io::Cursor::new(&mut output),
                        image::ImageFormat::Tiff,
                    )
                    .map_err(|e| {
                        RepositoryError::Other(format!("Failed to encode TIFF: {e}"))
                    })?;
                output
            }
            _ => {
                return Err(RepositoryError::Other(format!(
                    "Unsupported image format: {ext}"
                )));
            }
        };

        // Write back via the provider
        let mut writer = self.open_write(path)?;
        writer.write_all(&rotated)?;
        Ok(())
    }
}

/// Apply rotation to a DynamicImage.
fn apply_rotation(img: image::DynamicImage, rotation: Rotation) -> image::DynamicImage {
    match rotation {
        Rotation::Cw90 => img.rotate90(),
        Rotation::Ccw90 => img.rotate270(),
        Rotation::Cw180 => img.rotate180(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{Cursor, Read};
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
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("Not found: {path}")))?
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

        let s1 = stacks.iter().find(|s| s.name == "IMG_001").unwrap();
        assert!(s1.original.is_some());
        assert!(s1.enhanced.is_some());
        assert!(s1.back.is_some());
        assert_eq!(s1.repo_id.as_deref(), Some("cloud://photos"));

        let s2 = stacks.iter().find(|s| s.name == "IMG_002").unwrap();
        assert!(s2.original.is_some());
        assert!(s2.enhanced.is_none());
        assert!(s2.back.is_none());
    }

    #[test]
    fn test_foreign_repo_get_stack() {
        let provider = MockProvider::new("cloud://test");
        provider.add_file("IMG_001.jpg", b"data");

        let repo = ForeignRepository::new(Box::new(provider));
        let stacks = repo.scan().unwrap();
        let id = &stacks[0].id;

        let stack = repo.get_stack(id).unwrap();
        assert_eq!(stack.name, "IMG_001");
    }

    #[test]
    fn test_foreign_repo_get_stack_not_found() {
        let provider = MockProvider::new("cloud://test");
        let repo = ForeignRepository::new(Box::new(provider));
        let result = repo.get_stack("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RepositoryError::NotFound(_)));
    }

    #[test]
    fn test_foreign_repo_read_image() {
        let provider = MockProvider::new("cloud://test");
        provider.add_file("photo.jpg", b"image bytes");

        let repo = ForeignRepository::new(Box::new(provider));
        let mut reader = repo.read_image("photo.jpg").unwrap();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"image bytes");
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
    fn test_foreign_repo_write_metadata() {
        let provider = MockProvider::new("cloud://test");
        provider.add_file("IMG_001.jpg", b"data");

        let repo = ForeignRepository::new(Box::new(provider));
        let stacks = repo.scan().unwrap();
        let stack = &stacks[0];

        let mut tags = Metadata::default();
        tags.xmp_tags
            .insert("dc:title".to_string(), "Test Photo".to_string());

        let result = repo.write_metadata(stack, &tags);
        assert!(result.is_ok());

        // Verify sidecar was written
        let mut reader = FileAccess::open_read(&repo, "IMG_001.xmp").unwrap();
        let mut content = String::new();
        reader.read_to_string(&mut content).unwrap();
        assert!(content.contains("dc:title"));
        assert!(content.contains("Test Photo"));
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
        assert_eq!(stack.name, "IMG_001");
        assert_eq!(stack.folder.as_deref(), Some("1984_Mexico"));
        // Folder metadata should be parsed
        assert_eq!(stack.metadata.exif_tags.get("Year").map(|s| s.as_str()), Some("1984"));
        assert_eq!(
            stack.metadata.exif_tags.get("Subject").map(|s| s.as_str()),
            Some("Mexico")
        );
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
        assert_eq!(stacks[0].name, "IMG_001");
        assert!(stacks[0].enhanced.is_some());
    }
}
