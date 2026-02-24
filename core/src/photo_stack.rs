//! Photo stack data structures representing grouped FastFoto scans.
//!
//! This module defines the core [`PhotoStack`] type that groups related image files
//! from an Epson FastFoto scanner into a single logical unit. Each stack can contain:
//!
//! - An **original** front scan
//! - An **enhanced** (color-corrected) front scan
//! - A **back** scan of the photo
//!
//! ## Naming Convention
//!
//! FastFoto uses a consistent naming convention with `_a` and `_b` suffixes:
//!
//! ```text
//! IMG_0001.jpg      # Original front scan
//! IMG_0001_a.jpg    # Enhanced front (color-corrected)
//! IMG_0001_b.jpg    # Back of photo
//! ```
//!
//! The stack ID is derived from the base filename without suffix or extension
//! (e.g., `IMG_0001` in the example above).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::metadata::{detect_image_format, ImageFormat};

/// A unified representation of a single scanned photo from an Epson FastFoto scanner.
///
/// Groups the original scan, enhanced version, and back-of-photo image into
/// a single logical unit with associated metadata. Supports both JPEG and TIFF formats.
///
/// # Naming Convention
///
/// The `id` is derived from the base filename. For files like:
/// - `Family_001.jpg` (original)
/// - `Family_001_a.jpg` (enhanced)
/// - `Family_001_b.jpg` (back)
///
/// The stack `id` would be `Family_001`.
///
/// # Examples
///
/// ```
/// use photostax_core::photo_stack::PhotoStack;
/// use std::path::PathBuf;
///
/// let mut stack = PhotoStack::new("Vacation_042");
/// stack.original = Some(PathBuf::from("/photos/Vacation_042.jpg"));
/// stack.enhanced = Some(PathBuf::from("/photos/Vacation_042_a.jpg"));
///
/// assert!(stack.has_any_image());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoStack {
    /// Unique identifier derived from the base filename (without `_a`/`_b` suffix or extension).
    ///
    /// For example, files `IMG_001.jpg`, `IMG_001_a.jpg`, and `IMG_001_b.jpg`
    /// all share the ID `IMG_001`.
    pub id: String,

    /// Path to the original front scan (e.g., `IMG_001.jpg` or `IMG_001.tif`).
    ///
    /// This is the unprocessed scan directly from the FastFoto scanner.
    pub original: Option<PathBuf>,

    /// Path to the enhanced front scan (e.g., `IMG_001_a.jpg` or `IMG_001_a.tif`).
    ///
    /// This is the color-corrected, enhanced version produced by FastFoto software.
    /// The `_a` suffix indicates the "auto-enhanced" variant.
    pub enhanced: Option<PathBuf>,

    /// Path to the back-of-photo scan (e.g., `IMG_001_b.jpg` or `IMG_001_b.tif`).
    ///
    /// Captures any handwriting, dates, or notes on the photo's reverse side.
    /// Useful for OCR workflows to extract written metadata.
    pub back: Option<PathBuf>,

    /// Unified metadata from EXIF, XMP, and sidecar database sources.
    #[serde(default)]
    pub metadata: Metadata,
}

/// Metadata associated with a [`PhotoStack`].
///
/// Combines three sources of metadata into a unified view:
///
/// 1. **EXIF tags** — Embedded camera/scanner metadata read directly from image files
/// 2. **XMP tags** — Adobe XMP metadata embedded in images or sidecar `.xmp` files
/// 3. **Custom tags** — Application-specific metadata stored in the sidecar SQLite database
///
/// # Tag Sources
///
/// | Source | Description | Example Keys |
/// |--------|-------------|--------------|
/// | `exif_tags` | Standard EXIF fields from image | `Make`, `Model`, `DateTime` |
/// | `xmp_tags` | XMP/Dublin Core metadata | `description`, `creator` |
/// | `custom_tags` | User/application metadata | `ocr_text`, `album`, `people` |
///
/// # Examples
///
/// ```
/// use photostax_core::photo_stack::Metadata;
///
/// let mut meta = Metadata::default();
/// meta.exif_tags.insert("Make".to_string(), "EPSON".to_string());
/// meta.custom_tags.insert("album".to_string(), serde_json::json!("Family Reunion"));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    /// Standard EXIF tags read from the image files.
    ///
    /// Common keys include: `Make`, `Model`, `DateTime`, `DateTimeOriginal`,
    /// `ImageWidth`, `ImageLength`, `Artist`, `Copyright`, `GPSLatitude`, etc.
    pub exif_tags: HashMap<String, String>,

    /// XMP metadata tags that are readable by standard photo applications.
    ///
    /// Uses Dublin Core namespace for standard fields (`description`, `creator`,
    /// `title`, `subject`, `rights`, `date`) and a photostax namespace for custom fields.
    pub xmp_tags: HashMap<String, String>,

    /// Extended custom metadata stored in the sidecar database.
    ///
    /// Values are JSON to support rich types (strings, numbers, arrays, objects).
    /// Common keys include: `ocr_text` (from back scan), `album`, `people`, `tags`.
    pub custom_tags: HashMap<String, serde_json::Value>,
}

impl PhotoStack {
    /// Creates a new `PhotoStack` with only an ID and no associated files.
    ///
    /// Use this as a starting point when building stacks programmatically.
    /// Files and metadata can be added after construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::photo_stack::PhotoStack;
    /// use std::path::PathBuf;
    ///
    /// let mut stack = PhotoStack::new("Wedding_001");
    /// stack.original = Some(PathBuf::from("/photos/Wedding_001.jpg"));
    /// ```
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
    ///
    /// A stack without any images is typically an error condition from scanning.
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::photo_stack::PhotoStack;
    /// use std::path::PathBuf;
    ///
    /// let empty = PhotoStack::new("test");
    /// assert!(!empty.has_any_image());
    ///
    /// let mut with_image = PhotoStack::new("test");
    /// with_image.back = Some(PathBuf::from("test_b.jpg"));
    /// assert!(with_image.has_any_image());
    /// ```
    pub fn has_any_image(&self) -> bool {
        self.original.is_some() || self.enhanced.is_some() || self.back.is_some()
    }

    /// Determines the image format from the original (preferred) or enhanced path's extension.
    ///
    /// Returns `None` if no paths are set or if the format cannot be determined.
    /// Only checks original and enhanced images; the back image is not used for
    /// format detection since it's not always present.
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::photo_stack::PhotoStack;
    /// use photostax_core::metadata::ImageFormat;
    /// use std::path::PathBuf;
    ///
    /// let mut stack = PhotoStack::new("test");
    /// stack.original = Some(PathBuf::from("photo.tif"));
    /// assert_eq!(stack.format(), Some(ImageFormat::Tiff));
    /// ```
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
