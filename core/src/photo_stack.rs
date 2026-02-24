use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::metadata::{detect_image_format, ImageFormat};

/// A unified representation of a single scanned photo from an Epson FastFoto scanner.
///
/// Groups the original scan, enhanced version, and back-of-photo image into
/// a single logical unit with associated metadata. Supports both JPEG and TIFF formats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoStack {
    /// Unique identifier derived from the base filename (without suffix/extension).
    pub id: String,
    /// Path to the original front scan (e.g., `<name>.jpg` or `<name>.tif`).
    pub original: Option<PathBuf>,
    /// Path to the enhanced front scan (e.g., `<name>_a.jpg` or `<name>_a.tif`).
    pub enhanced: Option<PathBuf>,
    /// Path to the back-of-photo scan (e.g., `<name>_b.jpg` or `<name>_b.tif`).
    pub back: Option<PathBuf>,
    /// Unified metadata from EXIF/XMP and sidecar sources.
    #[serde(default)]
    pub metadata: Metadata,
}

/// Metadata associated with a [`PhotoStack`].
///
/// Combines standard EXIF/IPTC/XMP tags with extended custom metadata
/// stored in a sidecar database.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    /// Standard EXIF/IPTC/XMP tags read from the image files.
    pub exif_tags: HashMap<String, String>,
    /// XMP metadata tags readable by standard photo applications.
    pub xmp_tags: HashMap<String, String>,
    /// Extended custom metadata stored in the sidecar database.
    pub custom_tags: HashMap<String, serde_json::Value>,
}

impl PhotoStack {
    /// Creates a new `PhotoStack` with only an ID and no associated files.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            original: None,
            enhanced: None,
            back: None,
            metadata: Metadata::default(),
        }
    }

    /// Returns `true` if at least one image file is present in the stack.
    pub fn has_any_image(&self) -> bool {
        self.original.is_some() || self.enhanced.is_some() || self.back.is_some()
    }

    /// Determines the image format from the original (preferred) or enhanced path's extension.
    ///
    /// Returns `None` if no paths are set or if the format cannot be determined.
    pub fn format(&self) -> Option<ImageFormat> {
        self.original
            .as_ref()
            .and_then(|p| detect_image_format(p))
            .or_else(|| self.enhanced.as_ref().and_then(|p| detect_image_format(p)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_from_original_jpeg() {
        let mut stack = PhotoStack::new("test");
        stack.original = Some(PathBuf::from("photo.jpg"));
        assert_eq!(stack.format(), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn test_format_from_original_tiff() {
        let mut stack = PhotoStack::new("test");
        stack.original = Some(PathBuf::from("photo.tif"));
        assert_eq!(stack.format(), Some(ImageFormat::Tiff));

        let mut stack2 = PhotoStack::new("test2");
        stack2.original = Some(PathBuf::from("photo.tiff"));
        assert_eq!(stack2.format(), Some(ImageFormat::Tiff));
    }

    #[test]
    fn test_format_fallback_to_enhanced() {
        let mut stack = PhotoStack::new("test");
        stack.enhanced = Some(PathBuf::from("photo_a.tiff"));
        assert_eq!(stack.format(), Some(ImageFormat::Tiff));
    }

    #[test]
    fn test_format_original_takes_precedence() {
        let mut stack = PhotoStack::new("test");
        stack.original = Some(PathBuf::from("photo.jpg"));
        stack.enhanced = Some(PathBuf::from("photo_a.tif"));
        assert_eq!(stack.format(), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn test_format_none_when_no_paths() {
        let stack = PhotoStack::new("test");
        assert_eq!(stack.format(), None);
    }

    #[test]
    fn test_format_none_when_only_back_set() {
        let mut stack = PhotoStack::new("test");
        stack.back = Some(PathBuf::from("photo_b.jpg"));
        assert_eq!(stack.format(), None);
    }
}
