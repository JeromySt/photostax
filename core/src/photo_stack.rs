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
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::image_handle::ImageRef;
use crate::metadata_handle::{MetadataRef, NullMetadataHandle};
use crate::repository::RepositoryError;

/// Whether to classify ambiguous `_a` images using pixel analysis.
///
/// When the FastFoto scanner produces only an original and an `_a` image
/// (no `_b` file), the `_a` image could be either an enhanced front scan
/// or a back-of-photo scan. `ClassifyMode` controls whether the scanner
/// performs image I/O to resolve this ambiguity.
///
/// | Mode | Behaviour |
/// |------|-----------|
/// | [`Auto`](Self::Auto) | Analyse ambiguous `_a` images and reclassify as `back` when appropriate (default) |
/// | [`Skip`](Self::Skip) | Always treat `_a` as enhanced — no image I/O during scan |
///
/// **Note:** Prefer [`ScannerProfile`] over `ClassifyMode` — it captures
/// your FastFoto configuration and avoids unnecessary disk I/O.
///
/// # Examples
///
/// ```
/// use photostax_core::photo_stack::ClassifyMode;
///
/// let mode = ClassifyMode::default(); // Auto
/// assert_eq!(mode, ClassifyMode::Auto);
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassifyMode {
    /// Analyse ambiguous `_a` images using pixel variance to classify as
    /// enhanced front or back-of-photo. This is the default.
    #[default]
    Auto,
    /// Skip classification. `_a` is always treated as enhanced.
    Skip,
}

/// FastFoto scanner configuration profile.
///
/// Tells the scan engine how the Epson FastFoto was configured so it can
/// correctly classify `_a` / `_b` images **without disk I/O** in most cases.
///
/// | Profile | `_a` meaning | `_b` meaning | Disk I/O? |
/// |---------|--------------|--------------|-----------|
/// | [`EnhancedAndBack`](Self::EnhancedAndBack) | enhanced | back | No |
/// | [`EnhancedOnly`](Self::EnhancedOnly) | enhanced | — | No |
/// | [`OriginalOnly`](Self::OriginalOnly) | — | — | No |
/// | [`Auto`](Self::Auto) | ambiguous | back | Yes (pixel analysis) |
///
/// # Examples
///
/// ```
/// use photostax_core::photo_stack::ScannerProfile;
///
/// // Caller knows their FastFoto config — no disk I/O needed
/// let profile = ScannerProfile::EnhancedAndBack;
///
/// // Unknown config — will use pixel analysis for ambiguous _a files
/// let profile = ScannerProfile::Auto;
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScannerProfile {
    /// Enhanced image and back capture both enabled.
    /// `_a` is always enhanced, `_b` is always back. No classification I/O.
    EnhancedAndBack,
    /// Enhanced image enabled, back capture disabled.
    /// `_a` is always enhanced, no `_b` files expected. No classification I/O.
    EnhancedOnly,
    /// Only original capture — no enhanced or back images.
    /// No `_a` or `_b` files expected. No classification I/O.
    OriginalOnly,
    /// Unknown configuration (default). Ambiguous `_a` images (present
    /// without a `_b`) are analysed via pixel variance to determine if
    /// they are enhanced fronts or backs of photos. Requires disk I/O.
    #[default]
    Auto,
}

impl ScannerProfile {
    /// Convert from an integer value for FFI.
    ///
    /// | Value | Profile |
    /// |-------|---------|
    /// | `0` | [`Auto`](Self::Auto) |
    /// | `1` | [`EnhancedAndBack`](Self::EnhancedAndBack) |
    /// | `2` | [`EnhancedOnly`](Self::EnhancedOnly) |
    /// | `3` | [`OriginalOnly`](Self::OriginalOnly) |
    pub fn from_int(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Auto),
            1 => Some(Self::EnhancedAndBack),
            2 => Some(Self::EnhancedOnly),
            3 => Some(Self::OriginalOnly),
            _ => None,
        }
    }

    /// Whether this profile requires disk I/O for classification.
    pub fn needs_classification(&self) -> bool {
        matches!(self, Self::Auto)
    }
}

impl From<ClassifyMode> for ScannerProfile {
    fn from(mode: ClassifyMode) -> Self {
        match mode {
            ClassifyMode::Auto => ScannerProfile::Auto,
            ClassifyMode::Skip => ScannerProfile::EnhancedAndBack,
        }
    }
}

/// Phase of a multi-pass scan operation.
///
/// Used in [`ScanProgress`] to indicate which stage the scan is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanPhase {
    /// Pass 1: fast directory scan — discovering files and grouping stacks.
    Scanning = 0,
    /// Pass 2: classifying ambiguous `_a` images via pixel analysis
    /// (only when [`ScannerProfile::Auto`]).
    Classifying = 1,
    /// All passes complete.
    Complete = 2,
}

/// Progress information emitted during a scan operation.
///
/// Callers receive this through the progress callback passed to
/// [`Repository::scan_with_progress`](crate::repository::Repository::scan_with_progress).
///
/// # Fields
///
/// - `phase` — which pass is currently executing
/// - `current` — items processed so far in the current phase
/// - `total` — total items in the current phase (0 = indeterminate)
#[derive(Debug, Clone)]
pub struct ScanProgress {
    /// Current scan phase.
    pub phase: ScanPhase,
    /// Items processed so far in this phase.
    pub current: usize,
    /// Total items in this phase (0 means indeterminate / not yet known).
    pub total: usize,
}

/// Which images in a [`PhotoStack`] to rotate.
///
/// | Target | Images rotated |
/// |--------|----------------|
/// | [`All`](Self::All) | original + enhanced + back |
/// | [`Front`](Self::Front) | original + enhanced |
/// | [`Back`](Self::Back) | back only |
///
/// # Examples
///
/// ```
/// use photostax_core::photo_stack::RotationTarget;
///
/// let target = RotationTarget::default(); // All
/// assert_eq!(target, RotationTarget::All);
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotationTarget {
    /// Rotate all images in the stack (original + enhanced + back).
    #[default]
    All,
    /// Rotate front-side images only (original + enhanced).
    Front,
    /// Rotate back-side image only.
    Back,
}

impl RotationTarget {
    /// Convert from an integer value for FFI.
    ///
    /// | Value | Target |
    /// |-------|--------|
    /// | `0` | [`All`](Self::All) |
    /// | `1` | [`Front`](Self::Front) |
    /// | `2` | [`Back`](Self::Back) |
    pub fn from_int(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::All),
            1 => Some(Self::Front),
            2 => Some(Self::Back),
            _ => None,
        }
    }
}

/// Rotation angle for rotating all images in a [`PhotoStack`].
///
/// Each variant corresponds to a fixed rotation applied to every image
/// file in the stack. Pixel data is re-encoded on disk.
///
/// # Mapping from Degrees
///
/// | Input | Variant |
/// |-------|---------|
/// | `90` | [`Cw90`](Self::Cw90) |
/// | `-90` / `270` | [`Ccw90`](Self::Ccw90) |
/// | `180` / `-180` | [`Cw180`](Self::Cw180) |
///
/// # Examples
///
/// ```
/// use photostax_core::photo_stack::Rotation;
///
/// let r = Rotation::from_degrees(90).unwrap();
/// assert_eq!(r, Rotation::Cw90);
///
/// let r = Rotation::from_degrees(-90).unwrap();
/// assert_eq!(r, Rotation::Ccw90);
///
/// let r = Rotation::from_degrees(180).unwrap();
/// assert_eq!(r, Rotation::Cw180);
///
/// assert_eq!(Rotation::from_degrees(-180), Some(Rotation::Cw180));
/// assert_eq!(Rotation::from_degrees(45), None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rotation {
    /// 90° clockwise.
    Cw90,
    /// 90° counter-clockwise (equivalently 270° clockwise).
    Ccw90,
    /// 180° rotation (same as −180°).
    Cw180,
}

impl Rotation {
    /// Convert a degree value to a [`Rotation`].
    ///
    /// Accepts `90`, `-90`, `270`, `180`, and `-180`.
    /// Returns `None` for unsupported angles.
    pub fn from_degrees(degrees: i32) -> Option<Self> {
        match degrees {
            90 => Some(Self::Cw90),
            -90 | 270 => Some(Self::Ccw90),
            180 | -180 => Some(Self::Cw180),
            _ => None,
        }
    }

    /// Return the rotation as a positive degree value.
    pub fn as_degrees(&self) -> i32 {
        match self {
            Self::Cw90 => 90,
            Self::Ccw90 => 270,
            Self::Cw180 => 180,
        }
    }
}

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
///
/// let stack = PhotoStack::new("Vacation_042");
///
/// assert!(!stack.has_any_image());
/// ```
#[derive(Debug, Clone)]
pub struct PhotoStack {
    /// Unique identifier derived from the base filename (without `_a`/`_b` suffix or extension).
    ///
    /// For example, files `IMG_001.jpg`, `IMG_001_a.jpg`, and `IMG_001_b.jpg`
    /// all share the ID `IMG_001`.
    pub id: String,

    /// Human-readable stem name (e.g., `"IMG_001"`).
    pub name: String,

    /// Subfolder name this stack was scanned from (e.g., `"1984_Mexico"`).
    pub folder: Option<String>,

    /// Which repository this stack belongs to.
    pub repo_id: Option<String>,

    /// Base directory where this stack's files live (for sidecar I/O).
    pub location: Option<String>,

    /// Original front scan (e.g., `IMG_001.jpg` or `IMG_001.tif`).
    ///
    /// This is the unprocessed scan directly from the FastFoto scanner.
    pub original: ImageRef,

    /// Enhanced front scan (e.g., `IMG_001_a.jpg` or `IMG_001_a.tif`).
    ///
    /// This is the color-corrected, enhanced version produced by FastFoto software.
    /// The `_a` suffix indicates the "auto-enhanced" variant.
    pub enhanced: ImageRef,

    /// Back-of-photo scan (e.g., `IMG_001_b.jpg` or `IMG_001_b.tif`).
    ///
    /// Captures any handwriting, dates, or notes on the photo's reverse side.
    /// Useful for OCR workflows to extract written metadata.
    pub back: ImageRef,

    /// Unified metadata from EXIF, XMP, and XMP sidecar file sources.
    pub metadata: MetadataRef,
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
    ///
    /// let stack = PhotoStack::new("Wedding_001");
    /// assert!(!stack.has_any_image());
    /// ```
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            folder: None,
            repo_id: None,
            location: None,
            original: ImageRef::absent(),
            enhanced: ImageRef::absent(),
            back: ImageRef::absent(),
            metadata: MetadataRef::new(Arc::new(NullMetadataHandle)),
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
    ///
    /// let empty = PhotoStack::new("test");
    /// assert!(!empty.has_any_image());
    /// ```
    pub fn has_any_image(&self) -> bool {
        self.original.is_present() || self.enhanced.is_present() || self.back.is_present()
    }

    /// Returns the number of image files present in this stack.
    ///
    /// Counts the present image variants (original, enhanced, back).
    /// This is available immediately after scanning without loading metadata.
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::photo_stack::PhotoStack;
    ///
    /// let stack = PhotoStack::new("test");
    /// assert_eq!(stack.image_count(), 0);
    /// ```
    pub fn image_count(&self) -> usize {
        self.original.is_present() as usize
            + self.enhanced.is_present() as usize
            + self.back.is_present() as usize
    }

    /// Compute a Merkle-style content hash over all present image files.
    ///
    /// Iterates over `original`, `enhanced`, and `back` (in order), computes
    /// each file's content hash (lazy — cached after first call), then feeds
    /// all individual hashes into a single SHA-256 to produce a combined hash.
    ///
    /// Returns `Ok(None)` when the stack contains no image files.
    ///
    /// # Errors
    ///
    /// Returns a [`RepositoryError`] if any image file cannot be read.
    pub fn content_hash(&mut self) -> Result<Option<String>, RepositoryError> {
        let mut hashes: Vec<String> = Vec::new();

        for r in [&mut self.original, &mut self.enhanced, &mut self.back] {
            if r.is_present() {
                hashes.push(r.hash()?.to_string());
            }
        }

        if hashes.is_empty() {
            return Ok(None);
        }

        let mut hasher = Sha256::new();
        for h in &hashes {
            hasher.update(h.as_bytes());
        }
        let digest = hasher.finalize();
        let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
        Ok(Some(hex))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::image_handle::ImageRef;

    // Mock ImageHandle for test stacks that don't need real files
    struct MockImageHandle {
        valid: std::sync::atomic::AtomicBool,
    }

    impl MockImageHandle {
        fn new() -> Self {
            Self {
                valid: std::sync::atomic::AtomicBool::new(true),
            }
        }
    }

    impl crate::image_handle::ImageHandle for MockImageHandle {
        fn read(
            &self,
        ) -> Result<Box<dyn crate::file_access::ReadSeek>, crate::repository::RepositoryError>
        {
            Ok(Box::new(std::io::Cursor::new(vec![])))
        }
        fn stream(
            &self,
        ) -> Result<
            crate::hashing::HashingReader<Box<dyn std::io::Read + Send>>,
            crate::repository::RepositoryError,
        > {
            Ok(crate::hashing::HashingReader::new(Box::new(
                std::io::Cursor::new(vec![]),
            )))
        }
        fn hash(&self) -> Result<String, crate::repository::RepositoryError> {
            Ok("0000000000000000".to_string())
        }
        fn dimensions(&self) -> Result<(u32, u32), crate::repository::RepositoryError> {
            Ok((640, 480))
        }
        fn size(&self) -> u64 {
            0
        }
        fn rotate(&self, _: Rotation) -> Result<(), crate::repository::RepositoryError> {
            Ok(())
        }
        fn is_valid(&self) -> bool {
            self.valid.load(std::sync::atomic::Ordering::Relaxed)
        }
        fn invalidate(&self) {
            self.valid
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn mock_ref() -> ImageRef {
        ImageRef::new(Arc::new(MockImageHandle::new()))
    }

    #[test]
    fn test_photo_stack_new_defaults() {
        let stack = PhotoStack::new("test_id");
        assert_eq!(stack.id, "test_id");
        assert_eq!(stack.name, "test_id");
        assert!(stack.folder.is_none());
        assert!(stack.repo_id.is_none());
        assert!(!stack.original.is_present());
        assert!(!stack.enhanced.is_present());
        assert!(!stack.back.is_present());
    }

    #[test]
    fn test_has_any_image_none() {
        let stack = PhotoStack::new("test");
        assert!(!stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_original_only() {
        let mut stack = PhotoStack::new("test");
        stack.original = mock_ref();
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_enhanced_only() {
        let mut stack = PhotoStack::new("test");
        stack.enhanced = mock_ref();
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_back_only() {
        let mut stack = PhotoStack::new("test");
        stack.back = mock_ref();
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_all() {
        let mut stack = PhotoStack::new("test");
        stack.original = mock_ref();
        stack.enhanced = mock_ref();
        stack.back = mock_ref();
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_image_count() {
        let mut stack = PhotoStack::new("test");
        assert_eq!(stack.image_count(), 0);

        stack.original = mock_ref();
        stack.enhanced = mock_ref();
        assert_eq!(stack.image_count(), 2);

        stack.back = mock_ref();
        assert_eq!(stack.image_count(), 3);
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
    }

    #[test]
    fn test_ai_writeback_via_custom_tags() {
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
    }

    #[test]
    fn test_photo_stack_clone() {
        let mut stack = PhotoStack::new("test");
        stack.original = mock_ref();

        let cloned = stack.clone();
        assert_eq!(cloned.id, stack.id);
        assert!(cloned.original.is_present());
    }

    #[test]
    fn test_photo_stack_new_from_string() {
        let stack = PhotoStack::new(String::from("string_id"));
        assert_eq!(stack.id, "string_id");
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
    }

    // ── Rotation enum tests ────────────────────────────────────────────────

    #[test]
    fn test_rotation_from_degrees_90() {
        assert_eq!(Rotation::from_degrees(90), Some(Rotation::Cw90));
    }

    #[test]
    fn test_rotation_from_degrees_neg90() {
        assert_eq!(Rotation::from_degrees(-90), Some(Rotation::Ccw90));
    }

    #[test]
    fn test_rotation_from_degrees_270() {
        assert_eq!(Rotation::from_degrees(270), Some(Rotation::Ccw90));
    }

    #[test]
    fn test_rotation_from_degrees_180() {
        assert_eq!(Rotation::from_degrees(180), Some(Rotation::Cw180));
    }

    #[test]
    fn test_rotation_from_degrees_neg180() {
        assert_eq!(Rotation::from_degrees(-180), Some(Rotation::Cw180));
    }

    #[test]
    fn test_rotation_from_degrees_invalid() {
        assert_eq!(Rotation::from_degrees(0), None);
        assert_eq!(Rotation::from_degrees(45), None);
        assert_eq!(Rotation::from_degrees(360), None);
    }

    #[test]
    fn test_rotation_as_degrees() {
        assert_eq!(Rotation::Cw90.as_degrees(), 90);
        assert_eq!(Rotation::Ccw90.as_degrees(), 270);
        assert_eq!(Rotation::Cw180.as_degrees(), 180);
    }

    #[test]
    fn test_rotation_roundtrip_serde() {
        let r = Rotation::Cw90;
        let json = serde_json::to_string(&r).unwrap();
        let deser: Rotation = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, r);
    }
}
