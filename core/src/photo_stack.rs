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

    /// Unified metadata from EXIF, XMP, and XMP sidecar file sources.
    #[serde(default)]
    pub metadata: Metadata,
}

/// Metadata associated with a [`PhotoStack`].
///
/// Combines three sources of metadata into a unified view:
///
/// 1. **EXIF tags** — Embedded camera/scanner metadata read directly from image files
/// 2. **XMP tags** — Adobe XMP metadata embedded in images or sidecar `.xmp` files
/// 3. **Custom tags** — Application-specific metadata stored in XMP sidecar files
///
/// # Tag Sources
///
/// | Source | Description | Example Keys |
/// |--------|-------------|--------------|
/// | `exif_tags` | Standard EXIF fields from image | `Make`, `Model`, `DateTime` |
/// | `xmp_tags` | XMP/Dublin Core metadata | `description`, `creator`, `subject` |
/// | `custom_tags` | User/application metadata | `album`, `notes`, `rating` |
///
/// # Writing Metadata from AI or Other External Systems
///
/// The existing fields are designed to accept metadata from any source — including
/// AI/ML analysis, OCR engines, or manual curation.  `xmp_tags` map to the
/// industry-standard Dublin Core namespace, so the data is readable by any photo
/// application (Lightroom, darktable, Google Photos, Apple Photos, etc.).
///
/// ## Dublin Core XMP Keys for AI Write-Back
///
/// | Key | DC Field | Use for |
/// |-----|----------|---------|
/// | `description` | `dc:description` | AI-generated caption or scene description |
/// | `title` | `dc:title` | Photo title |
/// | `subject` | `dc:subject` | People, places, objects, events (comma-separated keywords) |
/// | `creator` | `dc:creator` | Photographer / attribution |
/// | `date` | `dc:date` | Estimated or corrected date |
/// | `rights` | `dc:rights` | Copyright notice |
///
/// Any key not in the above list is stored in the `photostax:` namespace in XMP,
/// which is still readable by standards-compliant viewers.
///
/// ## Custom Tags for Structured Data
///
/// Use `custom_tags` for richer data that doesn't fit Dublin Core's flat strings:
///
/// | Key | Value Type | Description |
/// |-----|------------|-------------|
/// | `people` | `["Alice", "Bob"]` | People identified in the photo |
/// | `places` | `["Paris", "Eiffel Tower"]` | Named places |
/// | `location` | `{"lat": 48.8, "lng": 2.3}` | Geo-coordinates |
/// | `events` | `["Wedding"]` | Events depicted |
/// | `holidays` | `["Christmas"]` | Holidays detected |
/// | `era` | `"1980s"` | Estimated decade |
/// | `mood` | `"joyful"` | Emotional tone |
/// | `ocr_back` | `"Happy Birthday!"` | OCR text from back of photo |
///
/// # Examples
///
/// ```
/// use photostax_core::photo_stack::Metadata;
///
/// let mut meta = Metadata::default();
///
/// // Standard XMP — readable by every photo viewer
/// meta.xmp_tags.insert("description".to_string(), "Family at the beach, July 1985".to_string());
/// meta.xmp_tags.insert("subject".to_string(), "beach, family, vacation, Alice, Bob".to_string());
/// meta.xmp_tags.insert("date".to_string(), "1985-07-04".to_string());
///
/// // Structured custom tags — richer data in the sidecar
/// meta.custom_tags.insert("people".to_string(), serde_json::json!(["Alice", "Bob"]));
/// meta.custom_tags.insert("events".to_string(), serde_json::json!(["Family Reunion"]));
/// meta.custom_tags.insert("mood".to_string(), serde_json::json!("joyful"));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    /// Standard EXIF tags read from the image files.
    ///
    /// Common keys include: `Make`, `Model`, `DateTime`, `DateTimeOriginal`,
    /// `ImageWidth`, `ImageLength`, `Artist`, `Copyright`, `GPSLatitude`, etc.
    ///
    /// EXIF overrides written via [`Repository::write_metadata`] are stored in
    /// the XMP sidecar file and merged on the next read — original EXIF data
    /// in the image file is never modified.
    ///
    /// [`Repository::write_metadata`]: crate::repository::Repository::write_metadata
    pub exif_tags: HashMap<String, String>,

    /// XMP metadata tags readable by any standard photo application.
    ///
    /// Keys are automatically mapped to Dublin Core when written:
    ///
    /// | Key | DC Field |
    /// |-----|----------|
    /// | `description` / `ImageDescription` | `dc:description` |
    /// | `creator` / `Artist` | `dc:creator` |
    /// | `title` | `dc:title` |
    /// | `subject` / `keywords` | `dc:subject` |
    /// | `rights` / `copyright` | `dc:rights` |
    /// | `date` / `DateTime` | `dc:date` |
    ///
    /// All other keys are stored in the `photostax:` XMP namespace.
    ///
    /// For JPEG files these are embedded directly in the image **and** mirrored
    /// to the sidecar.  For TIFF files they live in the `.xmp` sidecar only.
    pub xmp_tags: HashMap<String, String>,

    /// Application-specific custom metadata stored in the XMP sidecar file.
    ///
    /// Values are JSON to support rich types (strings, numbers, arrays, objects).
    /// Use this for structured data that doesn't map to a flat Dublin Core string,
    /// such as arrays of people names, geo-coordinate objects, or nested event details.
    pub custom_tags: HashMap<String, serde_json::Value>,
}

impl Metadata {
    /// Returns `true` if all metadata maps are empty.
    ///
    /// This is the state of a freshly-constructed `PhotoStack` before any
    /// metadata sources have been loaded.
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::photo_stack::Metadata;
    ///
    /// let meta = Metadata::default();
    /// assert!(meta.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.exif_tags.is_empty() && self.xmp_tags.is_empty() && self.custom_tags.is_empty()
    }
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

    /// Returns the name of the directory containing this stack's image files.
    ///
    /// Examines the `original`, then `enhanced`, then `back` path to extract
    /// the parent directory's final component. This is useful for deriving
    /// metadata from FastFoto folder naming conventions via
    /// [`parse_folder_name`].
    ///
    /// Returns `None` if no image paths are set or if the parent has no name
    /// (e.g. the file is at a filesystem root).
    ///
    /// [`parse_folder_name`]: crate::scanner::parse_folder_name
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::photo_stack::PhotoStack;
    /// use std::path::PathBuf;
    ///
    /// let mut stack = PhotoStack::new("1984_Mexico_0001");
    /// stack.original = Some(PathBuf::from("/photos/1984_Mexico/1984_Mexico_0001.jpg"));
    /// assert_eq!(stack.containing_folder(), Some("1984_Mexico".to_string()));
    /// ```
    pub fn containing_folder(&self) -> Option<String> {
        self.containing_dir()
            .and_then(|p| p.file_name().map(|n| n.to_os_string()))
            .and_then(|n| n.into_string().ok())
    }

    /// Returns the full path to the directory containing this stack's images.
    ///
    /// Examines the `original`, then `enhanced`, then `back` path to extract
    /// the parent directory. Useful for reading sidecars or other per-directory
    /// resources from the correct location during recursive scanning.
    ///
    /// Returns `None` if no image paths are set or if the parent cannot be
    /// determined.
    pub fn containing_dir(&self) -> Option<PathBuf> {
        self.original
            .as_ref()
            .or(self.enhanced.as_ref())
            .or(self.back.as_ref())
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
    }

    /// Returns the number of image files present in this stack.
    ///
    /// Counts the non-`None` image paths (original, enhanced, back).
    /// This is available immediately after scanning without loading metadata.
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::photo_stack::PhotoStack;
    /// use std::path::PathBuf;
    ///
    /// let mut stack = PhotoStack::new("test");
    /// assert_eq!(stack.image_count(), 0);
    ///
    /// stack.original = Some(PathBuf::from("photo.jpg"));
    /// stack.enhanced = Some(PathBuf::from("photo_a.jpg"));
    /// assert_eq!(stack.image_count(), 2);
    /// ```
    pub fn image_count(&self) -> usize {
        self.original.is_some() as usize
            + self.enhanced.is_some() as usize
            + self.back.is_some() as usize
    }

    /// Returns `true` if file-based metadata (EXIF, XMP, sidecar) has been loaded.
    ///
    /// After [`Repository::scan()`], stacks only contain folder-derived metadata.
    /// Call [`Repository::load_metadata()`] to populate EXIF, XMP, and sidecar
    /// tags. This method checks whether any EXIF tags are present as a heuristic
    /// for whether full metadata loading has occurred.
    ///
    /// Note: this returns `false` for stacks whose image files genuinely contain
    /// no EXIF data, even after `load_metadata()`. For an authoritative check,
    /// track loading state externally.
    ///
    /// [`Repository::scan()`]: crate::repository::Repository::scan
    /// [`Repository::load_metadata()`]: crate::repository::Repository::load_metadata
    pub fn is_metadata_loaded(&self) -> bool {
        !self.metadata.exif_tags.is_empty()
            || self
                .metadata
                .custom_tags
                .keys()
                .any(|k| !k.starts_with("folder_"))
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

    #[test]
    fn test_photo_stack_new_defaults() {
        let stack = PhotoStack::new("test_id");
        assert_eq!(stack.id, "test_id");
        assert!(stack.original.is_none());
        assert!(stack.enhanced.is_none());
        assert!(stack.back.is_none());
        assert!(stack.metadata.exif_tags.is_empty());
        assert!(stack.metadata.xmp_tags.is_empty());
        assert!(stack.metadata.custom_tags.is_empty());
    }

    #[test]
    fn test_has_any_image_none() {
        let stack = PhotoStack::new("test");
        assert!(!stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_original_only() {
        let mut stack = PhotoStack::new("test");
        stack.original = Some(PathBuf::from("photo.jpg"));
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_enhanced_only() {
        let mut stack = PhotoStack::new("test");
        stack.enhanced = Some(PathBuf::from("photo_a.jpg"));
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_back_only() {
        let mut stack = PhotoStack::new("test");
        stack.back = Some(PathBuf::from("photo_b.jpg"));
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_all() {
        let mut stack = PhotoStack::new("test");
        stack.original = Some(PathBuf::from("photo.jpg"));
        stack.enhanced = Some(PathBuf::from("photo_a.jpg"));
        stack.back = Some(PathBuf::from("photo_b.jpg"));
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut stack = PhotoStack::new("test_stack");
        stack.original = Some(PathBuf::from("photo.jpg"));
        stack.enhanced = Some(PathBuf::from("photo_a.jpg"));
        stack
            .metadata
            .exif_tags
            .insert("Make".to_string(), "EPSON".to_string());
        stack
            .metadata
            .custom_tags
            .insert("ocr".to_string(), serde_json::json!("Hello"));

        let json = serde_json::to_string(&stack).unwrap();
        let deserialized: PhotoStack = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "test_stack");
        assert!(deserialized.original.is_some());
        assert!(deserialized.enhanced.is_some());
        assert!(deserialized.back.is_none());
        assert_eq!(
            deserialized.metadata.exif_tags.get("Make"),
            Some(&"EPSON".to_string())
        );
        assert_eq!(
            deserialized.metadata.custom_tags.get("ocr"),
            Some(&serde_json::json!("Hello"))
        );
    }

    #[test]
    fn test_metadata_default() {
        let metadata = Metadata::default();
        assert!(metadata.exif_tags.is_empty());
        assert!(metadata.xmp_tags.is_empty());
        assert!(metadata.custom_tags.is_empty());
    }

    #[test]
    fn test_ai_writeback_via_xmp_tags() {
        // AI writes standard Dublin Core fields via xmp_tags — readable by any photo viewer.
        let mut meta = Metadata::default();
        meta.xmp_tags.insert(
            "description".to_string(),
            "Family at the beach, July 1985".to_string(),
        );
        meta.xmp_tags.insert(
            "subject".to_string(),
            "beach, family, vacation, Alice, Bob".to_string(),
        );
        meta.xmp_tags
            .insert("title".to_string(), "Summer Vacation 1985".to_string());
        meta.xmp_tags
            .insert("date".to_string(), "1985-07-04".to_string());
        meta.xmp_tags
            .insert("creator".to_string(), "Unknown".to_string());

        assert_eq!(meta.xmp_tags.len(), 5);
        assert_eq!(
            meta.xmp_tags.get("description"),
            Some(&"Family at the beach, July 1985".to_string())
        );
        assert_eq!(
            meta.xmp_tags.get("subject"),
            Some(&"beach, family, vacation, Alice, Bob".to_string())
        );
    }

    #[test]
    fn test_ai_writeback_via_custom_tags() {
        // AI writes structured data (arrays, objects) via custom_tags.
        let mut meta = Metadata::default();
        meta.custom_tags
            .insert("people".to_string(), serde_json::json!(["Alice", "Bob"]));
        meta.custom_tags
            .insert("events".to_string(), serde_json::json!(["Family Reunion"]));
        meta.custom_tags.insert(
            "location".to_string(),
            serde_json::json!({"lat": 37.82, "lng": -122.48}),
        );
        meta.custom_tags
            .insert("mood".to_string(), serde_json::json!("joyful"));
        meta.custom_tags.insert(
            "ocr_back".to_string(),
            serde_json::json!("Happy Birthday Mom!"),
        );

        assert_eq!(meta.custom_tags.len(), 5);
        assert_eq!(
            meta.custom_tags.get("people"),
            Some(&serde_json::json!(["Alice", "Bob"]))
        );
        assert_eq!(
            meta.custom_tags.get("location").unwrap().get("lat"),
            Some(&serde_json::json!(37.82))
        );
    }

    #[test]
    fn test_ai_writeback_roundtrip() {
        let mut stack = PhotoStack::new("ai_test");
        stack
            .metadata
            .xmp_tags
            .insert("description".to_string(), "Beach sunset".to_string());
        stack
            .metadata
            .xmp_tags
            .insert("subject".to_string(), "beach, sunset".to_string());
        stack
            .metadata
            .custom_tags
            .insert("people".to_string(), serde_json::json!(["Alice"]));
        stack
            .metadata
            .custom_tags
            .insert("mood".to_string(), serde_json::json!("nostalgic"));

        let json = serde_json::to_string(&stack).unwrap();
        let deser: PhotoStack = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deser.metadata.xmp_tags.get("description"),
            Some(&"Beach sunset".to_string())
        );
        assert_eq!(
            deser.metadata.custom_tags.get("people"),
            Some(&serde_json::json!(["Alice"]))
        );
        assert_eq!(
            deser.metadata.custom_tags.get("mood"),
            Some(&serde_json::json!("nostalgic"))
        );
    }

    #[test]
    fn test_metadata_with_xmp_tags() {
        let mut metadata = Metadata::default();
        metadata
            .xmp_tags
            .insert("description".to_string(), "Test photo".to_string());
        metadata
            .xmp_tags
            .insert("creator".to_string(), "John Doe".to_string());

        assert_eq!(metadata.xmp_tags.len(), 2);
        assert_eq!(
            metadata.xmp_tags.get("description"),
            Some(&"Test photo".to_string())
        );
    }

    #[test]
    fn test_photo_stack_clone() {
        let mut stack = PhotoStack::new("test");
        stack.original = Some(PathBuf::from("photo.jpg"));

        let cloned = stack.clone();
        assert_eq!(cloned.id, stack.id);
        assert_eq!(cloned.original, stack.original);
    }

    #[test]
    fn test_photo_stack_new_from_string() {
        let stack = PhotoStack::new(String::from("string_id"));
        assert_eq!(stack.id, "string_id");
    }

    // ── containing_folder tests ────────────────────────────────────────────

    #[test]
    fn test_containing_folder_from_original() {
        let mut stack = PhotoStack::new("IMG_001");
        stack.original = Some(PathBuf::from("/photos/1984_Mexico/IMG_001.jpg"));
        assert_eq!(stack.containing_folder(), Some("1984_Mexico".to_string()));
    }

    #[test]
    fn test_containing_folder_from_enhanced_fallback() {
        let mut stack = PhotoStack::new("IMG_001");
        stack.enhanced = Some(PathBuf::from("/photos/1984_Mexico/IMG_001_a.jpg"));
        assert_eq!(stack.containing_folder(), Some("1984_Mexico".to_string()));
    }

    #[test]
    fn test_containing_folder_from_back_fallback() {
        let mut stack = PhotoStack::new("IMG_001");
        stack.back = Some(PathBuf::from("/photos/SteveJones/IMG_001_b.jpg"));
        assert_eq!(stack.containing_folder(), Some("SteveJones".to_string()));
    }

    #[test]
    fn test_containing_folder_none_when_no_paths() {
        let stack = PhotoStack::new("IMG_001");
        assert_eq!(stack.containing_folder(), None);
    }

    #[test]
    fn test_containing_folder_prefers_original() {
        let mut stack = PhotoStack::new("IMG_001");
        stack.original = Some(PathBuf::from("/photos/1984/IMG_001.jpg"));
        stack.enhanced = Some(PathBuf::from("/photos/other/IMG_001_a.jpg"));
        assert_eq!(stack.containing_folder(), Some("1984".to_string()));
    }
}
