//! Concrete [`ImageHandle`] and [`MetadataHandle`] implementations for the
//! local filesystem backend.
//!
//! [`LocalImageHandle`] wraps a single image file on disk.
//! [`LocalMetadataHandle`] encapsulates EXIF / XMP / sidecar metadata I/O
//! for a single photo stack.

use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::file_access::ReadSeek;
use crate::hashing::{hash_file, HashingReader};
use crate::image_handle::ImageHandle;
use crate::metadata::{exif, sidecar, xmp, ImageFormat};
use crate::metadata_handle::MetadataHandle;
use crate::photo_stack::{Metadata, Rotation};
use crate::repository::RepositoryError;
use crate::scanner::parse_folder_name;

// ── LocalImageHandle ────────────────────────────────────────────────────────

/// Per-file image handle for the local filesystem.
///
/// Wraps a single file path and provides I/O operations (read, hash,
/// dimensions, rotate) via the [`ImageHandle`] trait.
pub struct LocalImageHandle {
    path: PathBuf,
    size: u64,
    valid: AtomicBool,
    cached_hash: Mutex<Option<String>>,
}

impl LocalImageHandle {
    /// Create a new handle for a local image file.
    pub fn new(path: impl Into<PathBuf>, size: u64) -> Self {
        Self {
            path: path.into(),
            size,
            valid: AtomicBool::new(true),
            cached_hash: Mutex::new(None),
        }
    }

    /// Borrow the backing file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ImageHandle for LocalImageHandle {
    fn read(&self) -> Result<Box<dyn ReadSeek>, RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        let file = std::fs::File::open(&self.path)?;
        Ok(Box::new(BufReader::with_capacity(64 * 1024, file)))
    }

    fn stream(&self) -> Result<HashingReader<Box<dyn Read + Send>>, RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        let file = std::fs::File::open(&self.path)?;
        let reader = BufReader::with_capacity(64 * 1024, file);
        Ok(HashingReader::new(Box::new(reader) as Box<dyn Read + Send>))
    }

    fn hash(&self) -> Result<String, RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        let mut guard = self.cached_hash.lock().unwrap();
        if let Some(ref h) = *guard {
            return Ok(h.clone());
        }
        let h = hash_file(&self.path)?;
        *guard = Some(h.clone());
        Ok(h)
    }

    fn dimensions(&self) -> Result<(u32, u32), RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        let (w, h) = image::image_dimensions(&self.path).map_err(|e| {
            RepositoryError::Other(format!(
                "Failed to read dimensions of {}: {e}",
                self.path.display()
            ))
        })?;
        Ok((w, h))
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn rotate(&self, rotation: Rotation) -> Result<(), RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        let img = image::open(&self.path).map_err(|e| {
            RepositoryError::Other(format!(
                "Failed to decode image {}: {e}",
                self.path.display()
            ))
        })?;
        let rotated = match rotation {
            Rotation::Cw90 => img.rotate90(),
            Rotation::Ccw90 => img.rotate270(),
            Rotation::Cw180 => img.rotate180(),
        };
        rotated.save(&self.path).map_err(|e| {
            RepositoryError::Other(format!(
                "Failed to save rotated image {}: {e}",
                self.path.display()
            ))
        })?;
        // Clear cached hash after mutation
        *self.cached_hash.lock().unwrap() = None;
        Ok(())
    }

    fn is_valid(&self) -> bool {
        self.valid.load(Ordering::Acquire)
    }

    fn invalidate(&self) {
        self.valid.store(false, Ordering::Release);
        *self.cached_hash.lock().unwrap() = None;
    }

    fn path(&self) -> Option<&Path> {
        Some(&self.path)
    }

    fn delete(&self) -> Result<(), RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        std::fs::remove_file(&self.path)?;
        self.invalidate();
        Ok(())
    }

    fn clear_caches(&self) {
        *self.cached_hash.lock().unwrap() = None;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn swap_with(&self, other: &dyn ImageHandle) -> Result<(), RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }

        let other_local = other
            .as_any()
            .downcast_ref::<LocalImageHandle>()
            .ok_or_else(|| {
                RepositoryError::Other("cannot swap with a different backend handle".into())
            })?;

        if !other_local.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }

        // Three-way rename: self → temp, other → self, temp → other
        let temp = self.path.with_extension("swap_tmp");
        std::fs::rename(&self.path, &temp)?;
        std::fs::rename(&other_local.path, &self.path)?;
        std::fs::rename(&temp, &other_local.path)?;

        // Clear caches on both handles (file contents changed)
        self.clear_caches();
        other_local.clear_caches();

        Ok(())
    }
}

// ── LocalMetadataHandle ─────────────────────────────────────────────────────

/// Per-stack metadata handle for the local filesystem.
///
/// Encapsulates reading and writing EXIF, embedded XMP, and sidecar XMP
/// metadata for a single photo stack on the local filesystem.
pub struct LocalMetadataHandle {
    /// Stack display name (used for sidecar file naming: `{name}.xmp`).
    stack_name: String,
    /// Directory containing the stack's image and sidecar files.
    sidecar_dir: PathBuf,
    /// Path to the best image for EXIF / embedded-XMP reading (enhanced preferred).
    image_path: Option<PathBuf>,
    /// Image format (affects XMP embedding strategy).
    image_format: Option<ImageFormat>,
    /// Folder name (last component) for folder-derived metadata.
    folder: Option<String>,
    valid: AtomicBool,
}

impl LocalMetadataHandle {
    /// Create a new metadata handle.
    pub fn new(
        stack_name: String,
        sidecar_dir: PathBuf,
        image_path: Option<PathBuf>,
        image_format: Option<ImageFormat>,
    ) -> Self {
        Self {
            stack_name,
            sidecar_dir,
            image_path,
            image_format,
            folder: None,
            valid: AtomicBool::new(true),
        }
    }

    /// Create a new metadata handle with folder metadata support.
    pub fn with_folder(
        stack_name: String,
        sidecar_dir: PathBuf,
        image_path: Option<PathBuf>,
        image_format: Option<ImageFormat>,
        folder: Option<String>,
    ) -> Self {
        Self {
            stack_name,
            sidecar_dir,
            image_path,
            image_format,
            folder,
            valid: AtomicBool::new(true),
        }
    }
}

impl MetadataHandle for LocalMetadataHandle {
    fn load(&self) -> Result<Metadata, RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        let mut meta = Metadata::default();

        // 1. EXIF from image file
        if let Some(ref img_path) = self.image_path {
            meta.exif_tags = exif::read_exif_tags(img_path).unwrap_or_default();
        }

        // 2. Embedded XMP (JPEG only)
        let mut xmp_tags = if let Some(ref img_path) = self.image_path {
            if matches!(self.image_format, Some(ImageFormat::Jpeg)) {
                xmp::read_xmp_from_jpeg(img_path).unwrap_or_default()
            } else {
                std::collections::HashMap::new()
            }
        } else {
            std::collections::HashMap::new()
        };

        // 3. XMP sidecar (overrides embedded XMP)
        let sidecar_data =
            sidecar::read_sidecar(&self.sidecar_dir, &self.stack_name).unwrap_or_default();
        for (k, v) in sidecar_data.xmp_tags {
            xmp_tags.insert(k, v);
        }
        meta.xmp_tags = xmp_tags;

        // 4. Custom tags from sidecar
        meta.custom_tags = sidecar_data.custom_tags;

        // 5. EXIF overrides from sidecar
        for (k, v) in sidecar_data.exif_overrides {
            meta.exif_tags.insert(k, v);
        }

        // 6. Folder-derived metadata (lowest priority — does not overwrite)
        if let Some(ref folder) = self.folder {
            let last = folder.rsplit('/').next().unwrap_or(folder);
            let fm = parse_folder_name(last);
            if !fm.is_empty() {
                if let Some(year) = fm.year {
                    if !meta.custom_tags.contains_key("folder_year") {
                        meta.custom_tags
                            .insert("folder_year".to_string(), serde_json::json!(year));
                    }
                    if !meta.xmp_tags.contains_key("date") {
                        meta.xmp_tags.insert("date".to_string(), format!("{year}"));
                    }
                }
                if let Some(ref ms) = fm.month_or_season {
                    if !meta.custom_tags.contains_key("folder_month_or_season") {
                        meta.custom_tags
                            .insert("folder_month_or_season".to_string(), serde_json::json!(ms));
                    }
                }
                if let Some(ref subj) = fm.subject {
                    if !meta.custom_tags.contains_key("folder_subject") {
                        meta.custom_tags
                            .insert("folder_subject".to_string(), serde_json::json!(subj));
                    }
                    if !meta.xmp_tags.contains_key("subject") {
                        meta.xmp_tags
                            .insert("subject".to_string(), subj.replace('_', " "));
                    }
                }
            }
        }

        Ok(meta)
    }

    fn write(&self, tags: &Metadata) -> Result<(), RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }

        // Embed XMP in JPEG files when possible
        if !tags.xmp_tags.is_empty() {
            if let Some(ref img_path) = self.image_path {
                let _ = xmp::write_xmp(img_path, &tags.xmp_tags);
            }
        }

        // Write everything to sidecar (authoritative)
        sidecar::merge_and_write(
            &self.sidecar_dir,
            &self.stack_name,
            &tags.xmp_tags,
            &tags.custom_tags,
            &tags.exif_tags,
        )
        .map_err(|e| RepositoryError::Other(e.to_string()))
    }

    fn is_valid(&self) -> bool {
        self.valid.load(Ordering::Acquire)
    }

    fn read_raw(&self) -> Result<Option<Vec<u8>>, RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        let path = sidecar::sidecar_path(&self.sidecar_dir, &self.stack_name);
        if path.exists() {
            let bytes = std::fs::read(&path).map_err(RepositoryError::Io)?;
            Ok(Some(bytes))
        } else {
            Ok(None)
        }
    }

    fn read_raw_stream(&self) -> Result<Option<Box<dyn ReadSeek>>, RepositoryError> {
        if !self.is_valid() {
            return Err(RepositoryError::StackDeleted);
        }
        let path = sidecar::sidecar_path(&self.sidecar_dir, &self.stack_name);
        if path.exists() {
            let file = std::fs::File::open(&path).map_err(RepositoryError::Io)?;
            Ok(Some(Box::new(std::io::BufReader::new(file))))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_image(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let img = image::RgbImage::from_fn(64, 64, |x, y| {
            image::Rgb([(x * 4) as u8, (y * 4) as u8, 128])
        });
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn test_local_image_handle_read() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "test.jpg");
        let size = std::fs::metadata(&path).unwrap().len();
        let handle = LocalImageHandle::new(&path, size);

        let mut reader = handle.read().unwrap();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_local_image_handle_hash_cached() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "hash.jpg");
        let size = std::fs::metadata(&path).unwrap().len();
        let handle = LocalImageHandle::new(&path, size);

        let h1 = handle.hash().unwrap();
        let h2 = handle.hash().unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn test_local_image_handle_dimensions() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "dim.jpg");
        let handle = LocalImageHandle::new(&path, 0);

        let (w, h) = handle.dimensions().unwrap();
        assert_eq!((w, h), (64, 64));
    }

    #[test]
    fn test_local_image_handle_invalidate() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "inv.jpg");
        let handle = LocalImageHandle::new(&path, 0);

        assert!(handle.is_valid());
        handle.invalidate();
        assert!(!handle.is_valid());
        assert!(handle.read().is_err());
    }

    #[test]
    fn test_local_image_handle_stream() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "stream.jpg");
        let size = std::fs::metadata(&path).unwrap().len();
        let handle = LocalImageHandle::new(&path, size);

        let mut stream = handle.stream().unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).unwrap();
        let hash = stream.finalize();
        assert_eq!(hash.len(), 16);
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_local_image_handle_size() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "sz.jpg");
        let size = std::fs::metadata(&path).unwrap().len();
        let handle = LocalImageHandle::new(&path, size);
        assert_eq!(handle.size(), size);
    }

    #[test]
    fn test_local_image_handle_via_image_ref() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "ref.jpg");
        let size = std::fs::metadata(&path).unwrap().len();
        let handle = Arc::new(LocalImageHandle::new(&path, size));

        let mut r = crate::image_handle::ImageRef::new(handle);
        assert!(r.is_present());
        assert!(r.is_valid());

        let hash = r.hash().unwrap().to_string();
        assert_eq!(hash.len(), 16);
    }

    #[test]
    fn test_local_metadata_handle_load_empty() {
        let tmp = TempDir::new().unwrap();
        let handle =
            LocalMetadataHandle::new("IMG_001".into(), tmp.path().to_path_buf(), None, None);

        let meta = handle.load().unwrap();
        assert!(meta.exif_tags.is_empty());
    }

    #[test]
    fn test_local_metadata_handle_write_and_load() {
        let tmp = TempDir::new().unwrap();
        let handle =
            LocalMetadataHandle::new("IMG_001".into(), tmp.path().to_path_buf(), None, None);

        let mut tags = Metadata::default();
        tags.custom_tags
            .insert("album".into(), serde_json::json!("Test"));
        handle.write(&tags).unwrap();

        let loaded = handle.load().unwrap();
        assert_eq!(
            loaded.custom_tags.get("album"),
            Some(&serde_json::json!("Test"))
        );
    }

    #[test]
    fn test_local_image_handle_path() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "path.jpg");
        let handle = LocalImageHandle::new(&path, 0);
        assert_eq!(handle.path(), &path);
    }

    #[test]
    fn test_local_image_handle_path_via_trait() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "trait_path.jpg");
        let handle: Box<dyn ImageHandle> = Box::new(LocalImageHandle::new(&path, 0));
        assert_eq!(handle.path(), Some(path.as_path()));
    }

    #[test]
    fn test_local_image_handle_delete() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "delete.jpg");
        let handle = LocalImageHandle::new(&path, 0);
        assert!(path.exists());
        handle.delete().unwrap();
        assert!(!path.exists());
        assert!(!handle.is_valid());
    }

    #[test]
    fn test_local_image_handle_delete_already_invalid() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "inv_del.jpg");
        let handle = LocalImageHandle::new(&path, 0);
        handle.invalidate();
        let result = handle.delete();
        assert!(result.is_err());
    }

    #[test]
    fn test_local_image_handle_clear_caches() {
        let tmp = TempDir::new().unwrap();
        let path = create_test_image(tmp.path(), "cc.jpg");
        let size = std::fs::metadata(&path).unwrap().len();
        let handle = LocalImageHandle::new(&path, size);
        // Populate cache
        let h1 = handle.hash().unwrap();
        assert!(!h1.is_empty());
        // Clear and recompute
        handle.clear_caches();
        let h2 = handle.hash().unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_swap_front_back_with_local_files() {
        use crate::image_handle::ImageRef;
        use crate::photo_stack::PhotoStack;

        let tmp = TempDir::new().unwrap();

        // Create three image files simulating a backwards scan
        let orig_path = tmp.path().join("IMG_001.jpg");
        let enh_path = tmp.path().join("IMG_001_a.jpg");
        let back_path = tmp.path().join("IMG_001_b.jpg");

        // Write distinct content to each file (small PNGs with different pixels)
        let orig_img = image::RgbImage::from_fn(4, 4, |_, _| image::Rgb([255, 0, 0]));
        let enh_img = image::RgbImage::from_fn(4, 4, |_, _| image::Rgb([0, 255, 0]));
        let back_img = image::RgbImage::from_fn(4, 4, |_, _| image::Rgb([0, 0, 255]));
        orig_img.save(&orig_path).unwrap();
        enh_img.save(&enh_path).unwrap();
        back_img.save(&back_path).unwrap();

        let orig_size = std::fs::metadata(&orig_path).unwrap().len();
        let enh_size = std::fs::metadata(&enh_path).unwrap().len();
        let back_size = std::fs::metadata(&back_path).unwrap().len();

        // Compute hashes before swap
        let orig_hash_before = {
            let h = LocalImageHandle::new(&orig_path, orig_size);
            h.hash().unwrap()
        };
        let back_hash_before = {
            let h = LocalImageHandle::new(&back_path, back_size);
            h.hash().unwrap()
        };

        // Build the PhotoStack
        let stack = PhotoStack::new("IMG_001");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = ImageRef::new(Arc::new(LocalImageHandle::new(&orig_path, orig_size)));
            inner.enhanced = ImageRef::new(Arc::new(LocalImageHandle::new(&enh_path, enh_size)));
            inner.back = ImageRef::new(Arc::new(LocalImageHandle::new(&back_path, back_size)));
        }

        // Perform the swap
        let result = stack.swap_front_back();
        assert!(result.is_ok());

        // Enhanced file should be deleted from disk
        assert!(!enh_path.exists());
        assert!(!stack.enhanced().is_present());

        // Original and back files should still exist
        assert!(orig_path.exists());
        assert!(back_path.exists());

        // File contents should be swapped:
        // IMG_001.jpg should now contain what was in IMG_001_b.jpg
        let orig_hash_after = stack.original().hash().unwrap().to_string();
        assert_eq!(orig_hash_after, back_hash_before);

        // IMG_001_b.jpg should now contain what was in IMG_001.jpg
        let back_hash_after = stack.back().hash().unwrap().to_string();
        assert_eq!(back_hash_after, orig_hash_before);
    }
}
