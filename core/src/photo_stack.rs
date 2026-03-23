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
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::file_access::ReadSeek;
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
    /// Which repository this progress report belongs to.
    pub repo_id: String,
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

/// Flags indicating which image variants are present in a [`PhotoStack`].
///
/// This is a convenience type for checking presence of multiple variants
/// in a single call, e.g. `stack.images_present().contains(ImageVariants::BACK)`.
///
/// # Examples
///
/// ```
/// use photostax_core::photo_stack::{PhotoStack, ImageVariants};
///
/// let stack = PhotoStack::new("test");
/// assert!(stack.images_present().is_empty());
/// assert!(!stack.images_present().contains(ImageVariants::ORIGINAL));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageVariants(u8);

impl ImageVariants {
    /// No image variants present.
    pub const NONE: Self = Self(0);
    /// Original (raw scan) image.
    pub const ORIGINAL: Self = Self(1);
    /// Enhanced (color-corrected) image.
    pub const ENHANCED: Self = Self(2);
    /// Back-of-photo image.
    pub const BACK: Self = Self(4);

    /// Returns true if no variants are present.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns true if this flags set contains the given flag.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }

    /// Returns the raw bits value.
    pub fn bits(self) -> u8 {
        self.0
    }
}

impl std::ops::BitOr for ImageVariants {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for ImageVariants {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for ImageVariants {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

/// Which image variant a proxy refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageVariantSlot {
    Original,
    Enhanced,
    Back,
}

/// Internal mutable state of a photo stack.
///
/// This is the single source of truth. All [`PhotoStack`] handles
/// (including those in query results and snapshots) share the same
/// `PhotoStackInner` via `Arc<RwLock<>>`.
#[derive(Debug)]
pub(crate) struct PhotoStackInner {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) folder: Option<String>,
    pub(crate) repo_id: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) writable: bool,
    pub(crate) original: ImageRef,
    pub(crate) enhanced: ImageRef,
    pub(crate) back: ImageRef,
    pub(crate) metadata: MetadataRef,
}

/// A unified representation of a single scanned photo from an Epson FastFoto scanner.
///
/// Groups the original scan, enhanced version, and back-of-photo image into
/// a single logical unit with associated metadata. Supports both JPEG and TIFF formats.
///
/// `PhotoStack` is a lightweight handle backed by `Arc<RwLock<>>`. Cloning
/// a stack produces another handle to the **same** data — mutations
/// (metadata loading, hash caching, etc.) are visible through all handles.
/// This means query result snapshots share state with the StackManager cache.
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
#[derive(Clone)]
pub struct PhotoStack {
    pub(crate) inner: Arc<RwLock<PhotoStackInner>>,
}

impl std::fmt::Debug for PhotoStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.inner.try_read() {
            Ok(inner) => f
                .debug_struct("PhotoStack")
                .field("id", &inner.id)
                .field("name", &inner.name)
                .field("folder", &inner.folder)
                .finish(),
            Err(_) => f
                .debug_struct("PhotoStack")
                .field("state", &"<locked>")
                .finish(),
        }
    }
}

/// Proxy for accessing a single image variant (original, enhanced, or back)
/// on a [`PhotoStack`].
///
/// Obtained via [`PhotoStack::original()`], [`PhotoStack::enhanced()`],
/// or [`PhotoStack::back()`]. Each method call acquires the lock
/// independently — no lock is held between calls.
pub struct ImageProxy<'a> {
    inner: &'a Arc<RwLock<PhotoStackInner>>,
    variant: ImageVariantSlot,
}

impl<'a> ImageProxy<'a> {
    /// Whether this image variant exists in the stack.
    pub fn is_present(&self) -> bool {
        let inner = self.inner.read().unwrap();
        self.get(&inner).is_present()
    }

    /// Whether the underlying file handle is still valid.
    pub fn is_valid(&self) -> bool {
        let inner = self.inner.read().unwrap();
        self.get(&inner).is_valid()
    }

    /// File size in bytes, or `None` if the variant is absent.
    pub fn size(&self) -> Option<u64> {
        let inner = self.inner.read().unwrap();
        self.get(&inner).size()
    }

    /// Read the full image as a seekable byte stream.
    pub fn read(&self) -> Result<Box<dyn ReadSeek>, RepositoryError> {
        let inner = self.inner.read().unwrap();
        self.get(&inner).read()
    }

    /// Open a streaming reader that computes the content hash as bytes
    /// are consumed.
    pub fn stream(
        &self,
    ) -> Result<crate::hashing::HashingReader<Box<dyn std::io::Read + Send>>, RepositoryError> {
        let inner = self.inner.read().unwrap();
        self.get(&inner).stream()
    }

    /// Return the content hash, computing and caching it on first call.
    pub fn hash(&self) -> Result<String, RepositoryError> {
        let mut inner = self.inner.write().unwrap();
        self.get_mut(&mut inner).hash().map(|s| s.to_owned())
    }

    /// Return the cached hash without triggering computation.
    pub fn cached_hash(&self) -> Option<String> {
        let inner = self.inner.read().unwrap();
        self.get(&inner).cached_hash().map(|s| s.to_owned())
    }

    /// Return image dimensions `(width, height)`, computing and caching
    /// on first call.
    pub fn dimensions(&self) -> Result<(u32, u32), RepositoryError> {
        let mut inner = self.inner.write().unwrap();
        self.get_mut(&mut inner).dimensions()
    }

    /// Rotate the image on disk.
    pub fn rotate(&self, rotation: Rotation) -> Result<(), RepositoryError> {
        let inner = self.inner.read().unwrap();
        if !inner.writable {
            return Err(RepositoryError::ReadOnly(
                "cannot rotate an image on a read-only stack".into(),
            ));
        }
        self.get(&inner).rotate(rotation)
    }

    /// Returns the file path of the backing file, if available.
    pub fn path(&self) -> Option<PathBuf> {
        let inner = self.inner.read().unwrap();
        self.get(&inner).path().map(|p| p.to_owned())
    }

    /// Deletes the backing file from disk and invalidates the handle.
    pub fn delete(&self) -> Result<(), RepositoryError> {
        let inner = self.inner.read().unwrap();
        if !inner.writable {
            return Err(RepositoryError::ReadOnly(
                "cannot delete an image on a read-only stack".into(),
            ));
        }
        self.get(&inner).delete()
    }

    /// Clear cached hash and dimensions.
    pub fn invalidate_caches(&self) {
        let mut inner = self.inner.write().unwrap();
        self.get_mut(&mut inner).invalidate_caches();
    }

    fn get<'b>(&self, inner: &'b PhotoStackInner) -> &'b ImageRef {
        match self.variant {
            ImageVariantSlot::Original => &inner.original,
            ImageVariantSlot::Enhanced => &inner.enhanced,
            ImageVariantSlot::Back => &inner.back,
        }
    }

    fn get_mut<'b>(&self, inner: &'b mut PhotoStackInner) -> &'b mut ImageRef {
        match self.variant {
            ImageVariantSlot::Original => &mut inner.original,
            ImageVariantSlot::Enhanced => &mut inner.enhanced,
            ImageVariantSlot::Back => &mut inner.back,
        }
    }
}

/// Proxy for accessing metadata on a [`PhotoStack`].
///
/// Obtained via [`PhotoStack::metadata()`]. Each method call acquires
/// the lock independently.
pub struct MetadataProxy<'a> {
    inner: &'a Arc<RwLock<PhotoStackInner>>,
}

impl<'a> MetadataProxy<'a> {
    /// Whether metadata has been loaded from the backing store.
    pub fn is_loaded(&self) -> bool {
        let inner = self.inner.read().unwrap();
        inner.metadata.is_loaded()
    }

    /// Whether the underlying handle is still valid.
    pub fn is_valid(&self) -> bool {
        let inner = self.inner.read().unwrap();
        inner.metadata.is_valid()
    }

    /// Load metadata from the backing store, caching the result.
    /// Returns an owned clone of the metadata.
    pub fn read(&self) -> Result<Metadata, RepositoryError> {
        let mut inner = self.inner.write().unwrap();
        inner.metadata.read().cloned()
    }

    /// Get cached metadata without triggering a load.
    pub fn cached(&self) -> Option<Metadata> {
        let inner = self.inner.read().unwrap();
        inner.metadata.cached().cloned()
    }

    /// Write metadata to the backing store.
    pub fn write(&self, tags: &Metadata) -> Result<(), RepositoryError> {
        let inner = self.inner.read().unwrap();
        if !inner.writable {
            return Err(RepositoryError::ReadOnly(
                "cannot write metadata on a read-only stack".into(),
            ));
        }
        inner.metadata.write(tags)
    }

    /// Invalidate the cached metadata, forcing a re-read on next access.
    pub fn invalidate(&self) {
        let mut inner = self.inner.write().unwrap();
        inner.metadata.invalidate();
    }

    /// Read the raw sidecar file bytes without parsing.
    ///
    /// Returns `Ok(None)` if no sidecar file exists.
    pub fn read_raw(&self) -> Result<Option<Vec<u8>>, RepositoryError> {
        let inner = self.inner.read().unwrap();
        inner.metadata.read_raw()
    }

    /// Open a stream to the raw sidecar file without parsing.
    ///
    /// Returns `Ok(None)` if no sidecar file exists.
    pub fn read_raw_stream(
        &self,
    ) -> Result<Option<Box<dyn crate::file_access::ReadSeek>>, RepositoryError> {
        let inner = self.inner.read().unwrap();
        inner.metadata.read_raw_stream()
    }
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
            inner: Arc::new(RwLock::new(PhotoStackInner {
                name: id.clone(),
                id,
                folder: None,
                repo_id: None,
                location: None,
                writable: true,
                original: ImageRef::absent(),
                enhanced: ImageRef::absent(),
                back: ImageRef::absent(),
                metadata: MetadataRef::new(Arc::new(NullMetadataHandle)),
            })),
        }
    }

    // ── Read accessors ──────────────────────────────────────────────────

    /// Unique identifier derived from the base filename.
    pub fn id(&self) -> String {
        self.inner.read().unwrap().id.clone()
    }

    /// Human-readable stem name (e.g., `"IMG_001"`).
    pub fn name(&self) -> String {
        self.inner.read().unwrap().name.clone()
    }

    /// Subfolder name this stack was scanned from.
    pub fn folder(&self) -> Option<String> {
        self.inner.read().unwrap().folder.clone()
    }

    /// Which repository this stack belongs to.
    pub fn repo_id(&self) -> Option<String> {
        self.inner.read().unwrap().repo_id.clone()
    }

    /// Base directory where this stack's files live.
    pub fn location(&self) -> Option<String> {
        self.inner.read().unwrap().location.clone()
    }

    /// Whether this stack supports write operations (rotate, delete,
    /// metadata write, swap).
    ///
    /// Returns `false` when the stack comes from a read-only repository.
    pub fn is_writable(&self) -> bool {
        self.inner.read().unwrap().writable
    }

    // ── Sub-object proxies ──────────────────────────────────────────────

    /// Access the original (front scan) image variant.
    pub fn original(&self) -> ImageProxy<'_> {
        ImageProxy {
            inner: &self.inner,
            variant: ImageVariantSlot::Original,
        }
    }

    /// Access the enhanced (color-corrected) image variant.
    pub fn enhanced(&self) -> ImageProxy<'_> {
        ImageProxy {
            inner: &self.inner,
            variant: ImageVariantSlot::Enhanced,
        }
    }

    /// Access the back-of-photo image variant.
    pub fn back(&self) -> ImageProxy<'_> {
        ImageProxy {
            inner: &self.inner,
            variant: ImageVariantSlot::Back,
        }
    }

    /// Access the unified metadata (EXIF + XMP + custom tags).
    pub fn metadata(&self) -> MetadataProxy<'_> {
        MetadataProxy { inner: &self.inner }
    }

    // ── Setters ────────────────────────────────────────────────────────

    /// Replace the original image reference.
    pub fn set_original(&self, image_ref: ImageRef) {
        self.inner.write().unwrap().original = image_ref;
    }

    /// Replace the enhanced image reference.
    pub fn set_enhanced(&self, image_ref: ImageRef) {
        self.inner.write().unwrap().enhanced = image_ref;
    }

    /// Replace the back image reference.
    pub fn set_back(&self, image_ref: ImageRef) {
        self.inner.write().unwrap().back = image_ref;
    }

    /// Replace the metadata reference.
    pub fn set_metadata(&self, metadata_ref: MetadataRef) {
        self.inner.write().unwrap().metadata = metadata_ref;
    }

    // ── Convenience queries ─────────────────────────────────────────────

    /// Returns `true` if at least one image file is present in the stack.
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
        let inner = self.inner.read().unwrap();
        inner.original.is_present() || inner.enhanced.is_present() || inner.back.is_present()
    }

    /// Returns a flags set indicating which image variants are present.
    ///
    /// # Examples
    ///
    /// ```
    /// use photostax_core::photo_stack::{PhotoStack, ImageVariants};
    ///
    /// let stack = PhotoStack::new("test");
    /// let present = stack.images_present();
    /// assert!(present.is_empty());
    /// assert!(!present.contains(ImageVariants::ORIGINAL));
    /// ```
    pub fn images_present(&self) -> ImageVariants {
        let inner = self.inner.read().unwrap();
        let mut flags = ImageVariants::NONE;
        if inner.original.is_present() {
            flags |= ImageVariants::ORIGINAL;
        }
        if inner.enhanced.is_present() {
            flags |= ImageVariants::ENHANCED;
        }
        if inner.back.is_present() {
            flags |= ImageVariants::BACK;
        }
        flags
    }

    /// Returns the number of image files present in this stack.
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
        let inner = self.inner.read().unwrap();
        inner.original.is_present() as usize
            + inner.enhanced.is_present() as usize
            + inner.back.is_present() as usize
    }

    /// Swaps front and back images when a photo was scanned backwards.
    ///
    /// After swap:
    /// - `original` ← old `back` content (the actual front of the photo)
    /// - `back` ← old `original` content (the actual back of the photo)
    /// - `enhanced` ← absent (old enhanced deleted — it was of the wrong side)
    ///
    /// The actual swap is delegated to the [`ImageHandle`](crate::image_handle::ImageHandle)
    /// backend.
    pub fn swap_front_back(&self) -> Result<(), RepositoryError> {
        let mut inner = self.inner.write().unwrap();
        if !inner.writable {
            return Err(RepositoryError::ReadOnly(
                "cannot swap front/back on a read-only stack".into(),
            ));
        }
        if !inner.back.is_present() {
            return Err(RepositoryError::Other(
                "Cannot swap front/back: back image is not present".into(),
            ));
        }

        // 1. Delete enhanced via handle (it was of the wrong side)
        if inner.enhanced.is_present() {
            let _ = inner.enhanced.delete();
        }
        inner.enhanced = ImageRef::absent();

        // 2. Swap original ↔ back via the handle's backend implementation
        // Split the borrows by using raw pointers to work around the borrow checker.
        // Safety: original and back are distinct fields of the same struct.
        let original_ptr = &mut inner.original as *mut ImageRef;
        let back_ptr = &mut inner.back as *mut ImageRef;
        unsafe {
            (*original_ptr).swap_with(&mut *back_ptr)?;
        }

        Ok(())
    }

    /// Compute a Merkle-style content hash over all present image files.
    ///
    /// Returns `Ok(None)` when the stack contains no image files.
    pub fn content_hash(&self) -> Result<Option<String>, RepositoryError> {
        let mut inner = self.inner.write().unwrap();
        let mut hashes: Vec<String> = Vec::new();

        if inner.original.is_present() {
            hashes.push(inner.original.hash()?.to_string());
        }
        if inner.enhanced.is_present() {
            hashes.push(inner.enhanced.hash()?.to_string());
        }
        if inner.back.is_present() {
            hashes.push(inner.back.hash()?.to_string());
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
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn swap_with(
            &self,
            _other: &dyn crate::image_handle::ImageHandle,
        ) -> Result<(), crate::repository::RepositoryError> {
            // Mock: no-op (no physical storage to swap)
            Ok(())
        }
        fn delete(&self) -> Result<(), crate::repository::RepositoryError> {
            self.invalidate();
            Ok(())
        }
    }

    fn mock_ref() -> ImageRef {
        ImageRef::new(Arc::new(MockImageHandle::new()))
    }

    #[test]
    fn test_photo_stack_new_defaults() {
        let stack = PhotoStack::new("test_id");
        assert_eq!(stack.id(), "test_id");
        assert_eq!(stack.name(), "test_id");
        assert!(stack.folder().is_none());
        assert!(stack.repo_id().is_none());
        assert!(!stack.original().is_present());
        assert!(!stack.enhanced().is_present());
        assert!(!stack.back().is_present());
    }

    #[test]
    fn test_has_any_image_none() {
        let stack = PhotoStack::new("test");
        assert!(!stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_original_only() {
        let stack = PhotoStack::new("test");
        stack.inner.write().unwrap().original = mock_ref();
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_enhanced_only() {
        let stack = PhotoStack::new("test");
        stack.inner.write().unwrap().enhanced = mock_ref();
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_back_only() {
        let stack = PhotoStack::new("test");
        stack.inner.write().unwrap().back = mock_ref();
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_has_any_image_all() {
        let stack = PhotoStack::new("test");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = mock_ref();
            inner.enhanced = mock_ref();
            inner.back = mock_ref();
        }
        assert!(stack.has_any_image());
    }

    #[test]
    fn test_image_count() {
        let stack = PhotoStack::new("test");
        assert_eq!(stack.image_count(), 0);

        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = mock_ref();
            inner.enhanced = mock_ref();
        }
        assert_eq!(stack.image_count(), 2);

        stack.inner.write().unwrap().back = mock_ref();
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
        let stack = PhotoStack::new("test");
        stack.inner.write().unwrap().original = mock_ref();

        let cloned = stack.clone();
        assert_eq!(cloned.id(), stack.id());
        assert!(cloned.original().is_present());
    }

    #[test]
    fn test_photo_stack_new_from_string() {
        let stack = PhotoStack::new(String::from("string_id"));
        assert_eq!(stack.id(), "string_id");
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

    // ── swap_front_back tests ──────────────────────────────────────────────

    #[test]
    fn test_swap_front_back_no_back_returns_error() {
        let stack = PhotoStack::new("test");
        stack.inner.write().unwrap().original = mock_ref();
        let result = stack.swap_front_back();
        assert!(result.is_err());
    }

    #[test]
    fn test_swap_front_back_logical_swap_with_mocks() {
        // Mocks have no path(), so the logical (handle-swap) branch runs.
        let stack = PhotoStack::new("test");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = mock_ref();
            inner.enhanced = mock_ref();
            inner.back = mock_ref();
        }

        assert_eq!(stack.image_count(), 3);
        let result = stack.swap_front_back();
        assert!(result.is_ok());

        // Enhanced should be absent after swap
        assert!(!stack.enhanced().is_present());
        // Original and back should still be present
        assert!(stack.original().is_present());
        assert!(stack.back().is_present());
        assert_eq!(stack.image_count(), 2);
    }

    #[test]
    fn test_swap_front_back_no_enhanced() {
        let stack = PhotoStack::new("test");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = mock_ref();
            inner.back = mock_ref();
        }

        let result = stack.swap_front_back();
        assert!(result.is_ok());
        assert!(!stack.enhanced().is_present());
        assert!(stack.original().is_present());
        assert!(stack.back().is_present());
    }

    #[test]
    fn test_swap_front_back_no_original_with_back() {
        // Only back present, no original — logical swap moves back→original
        let stack = PhotoStack::new("test");
        stack.inner.write().unwrap().back = mock_ref();

        let result = stack.swap_front_back();
        assert!(result.is_ok());
        assert!(stack.original().is_present());
        assert!(!stack.back().is_present());
    }

    #[test]
    fn test_read_only_stack_blocks_rotate() {
        let stack = PhotoStack::new("test");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = mock_ref();
            inner.writable = false;
        }
        let err = stack.original().rotate(Rotation::Cw90).unwrap_err();
        assert!(matches!(err, RepositoryError::ReadOnly(_)));
        assert!(err.to_string().contains("read-only"));
    }

    #[test]
    fn test_read_only_stack_blocks_delete() {
        let stack = PhotoStack::new("test");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = mock_ref();
            inner.writable = false;
        }
        let err = stack.original().delete().unwrap_err();
        assert!(matches!(err, RepositoryError::ReadOnly(_)));
    }

    #[test]
    fn test_read_only_stack_blocks_metadata_write() {
        let stack = PhotoStack::new("test");
        stack.inner.write().unwrap().writable = false;
        let err = stack.metadata().write(&Metadata::default()).unwrap_err();
        assert!(matches!(err, RepositoryError::ReadOnly(_)));
    }

    #[test]
    fn test_read_only_stack_blocks_swap_front_back() {
        let stack = PhotoStack::new("test");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = mock_ref();
            inner.back = mock_ref();
            inner.writable = false;
        }
        let err = stack.swap_front_back().unwrap_err();
        assert!(matches!(err, RepositoryError::ReadOnly(_)));
    }

    #[test]
    fn test_writable_stack_allows_operations() {
        let stack = PhotoStack::new("test");
        assert!(stack.is_writable());
        // Default stacks are writable — operations pass the writable check
        // (they may still fail at the I/O level, but NOT with ReadOnly)
        {
            let mut inner = stack.inner.write().unwrap();
            inner.writable = true;
            inner.original = mock_ref();
        }
        // Rotate on a mock handle succeeds (no real file to open)
        // The important thing is it doesn't return ReadOnly
        let result = stack.original().rotate(Rotation::Cw90);
        assert!(
            !matches!(result, Err(RepositoryError::ReadOnly(_))),
            "writable stack should not return ReadOnly"
        );
    }

    #[test]
    fn test_is_writable_reflects_inner_flag() {
        let stack = PhotoStack::new("test");
        assert!(stack.is_writable());

        stack.inner.write().unwrap().writable = false;
        assert!(!stack.is_writable());

        stack.inner.write().unwrap().writable = true;
        assert!(stack.is_writable());
    }
}
