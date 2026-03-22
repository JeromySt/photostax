//! Node.js native addon for photostax using napi-rs.
//!
//! This crate provides JavaScript/TypeScript bindings for the photostax-core library,
//! enabling Node.js applications to work with Epson FastFoto photo stacks.

use std::collections::HashMap;
use std::io::{self, Cursor, Read};
use std::path::PathBuf;
use std::rc::Rc;

use napi::bindgen_prelude::*;
use napi::{NapiRaw, NapiValue};
use napi_derive::napi;

use photostax_core::backends::local::LocalRepository;
use photostax_core::photo_stack::{
    Metadata as CoreMetadata, PhotoStack as CorePhotoStack, Rotation as CoreRotation,
    RotationTarget as CoreRotationTarget, ScannerProfile as CoreScannerProfile,
};
use photostax_core::search::{filter_stacks, SearchQuery as CoreSearchQuery};
use photostax_core::snapshot::ScanSnapshot as CoreScanSnapshot;
use photostax_core::stack_manager::StackManager;

// ── Thread-local Env for JS callback bridge ────────────────────────────

// The NapiProvider needs to call back into JS from within Rust trait methods
// that don't have access to napi::Env. Since StackManager uses RefCell (single-
// threaded), all calls happen on the main Node thread. We stash the current
// Env before each operation and clear it after.
thread_local! {
    static NAPI_ENV: std::cell::Cell<Option<napi::sys::napi_env>> = const { std::cell::Cell::new(None) };
}

fn with_env_stashed<F, R>(env: Env, f: F) -> R
where
    F: FnOnce() -> R,
{
    NAPI_ENV.with(|cell| cell.set(Some(env.raw())));
    let result = f();
    NAPI_ENV.with(|cell| cell.set(None));
    result
}

fn get_stashed_env() -> io::Result<Env> {
    NAPI_ENV.with(|cell| {
        cell.get()
            .map(|raw| unsafe { Env::from_raw(raw) })
            .ok_or_else(|| io::Error::other("napi Env not available (not in a napi call context)"))
    })
}

/// Metadata associated with a photo stack.
///
/// Combines EXIF tags (from image files), XMP tags (embedded or sidecar),
/// and custom tags (from the sidecar database) into a unified view.
#[napi(object)]
#[derive(Clone)]
pub struct JsMetadata {
    /// Standard EXIF tags from the image file (Make, Model, DateTime, etc.)
    pub exif_tags: HashMap<String, String>,
    /// XMP/Dublin Core metadata tags
    pub xmp_tags: HashMap<String, String>,
    /// Custom application metadata stored in the sidecar database
    pub custom_tags: HashMap<String, serde_json::Value>,
}

impl From<CoreMetadata> for JsMetadata {
    fn from(m: CoreMetadata) -> Self {
        Self {
            exif_tags: m.exif_tags,
            xmp_tags: m.xmp_tags,
            custom_tags: m.custom_tags,
        }
    }
}

impl From<JsMetadata> for CoreMetadata {
    fn from(m: JsMetadata) -> Self {
        Self {
            exif_tags: m.exif_tags,
            xmp_tags: m.xmp_tags,
            custom_tags: m.custom_tags,
        }
    }
}

/// Image dimensions in pixels.
#[napi(object)]
#[derive(Clone)]
pub struct JsDimensions {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Which image variants are present in a photo stack.
#[napi(object)]
#[derive(Clone)]
pub struct JsImagesPresent {
    /// Whether the original (raw scan) image is present.
    pub original: bool,
    /// Whether the enhanced (color-corrected) image is present.
    pub enhanced: bool,
    /// Whether the back-of-photo image is present.
    pub back: bool,
}

/// Accessor for a single image variant (original, enhanced, or back) in a photo stack.
///
/// Holds an Arc clone of the underlying PhotoStack — mutations are visible
/// through all clones.
#[napi(js_name = "ImageRef")]
pub struct JsImageRef {
    stack: CorePhotoStack,
    variant: u8, // 0=original, 1=enhanced, 2=back
}

// SAFETY: Single-threaded Node.js — all access on main thread via RefCell.
unsafe impl Send for JsImageRef {}

#[napi]
impl JsImageRef {
    /// Whether this image variant exists in the stack.
    #[napi(getter)]
    pub fn is_present(&self) -> bool {
        Self::get_image_ref(&self.stack, self.variant).is_present()
    }

    /// Whether the underlying file handle is still valid.
    #[napi(getter)]
    pub fn is_valid(&self) -> bool {
        Self::get_image_ref(&self.stack, self.variant).is_valid()
    }

    /// File size in bytes, or null if the variant is absent.
    #[napi(getter)]
    pub fn size(&self) -> Option<f64> {
        Self::get_image_ref(&self.stack, self.variant).size().map(|v| v as f64)
    }

    /// Read the full image data into a Buffer.
    #[napi]
    pub fn read(&self, env: Env) -> napi::Result<Buffer> {
        with_env_stashed(env, || {
            let img = Self::get_image_ref(&self.stack, self.variant);
            if !img.is_present() {
                return Err(napi::Error::from_reason(format!(
                    "{} image is not present",
                    Self::variant_name(self.variant)
                )));
            }
            let mut reader = img
                .read()
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
            let mut buf = Vec::new();
            reader
                .read_to_end(&mut buf)
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
            Ok(buf.into())
        })
    }

    /// Compute or return the cached SHA-256 content hash.
    #[napi]
    pub fn hash(&self, env: Env) -> napi::Result<String> {
        with_env_stashed(env, || {
            let img = Self::get_image_ref(&self.stack, self.variant);
            if !img.is_present() {
                return Err(napi::Error::from_reason(format!(
                    "{} image is not present",
                    Self::variant_name(self.variant)
                )));
            }
            let hash = img
                .hash()
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
            Ok(hash)
        })
    }

    /// Return the image dimensions { width, height } in pixels.
    #[napi]
    pub fn dimensions(&self, env: Env) -> napi::Result<JsDimensions> {
        with_env_stashed(env, || {
            let img = Self::get_image_ref(&self.stack, self.variant);
            if !img.is_present() {
                return Err(napi::Error::from_reason(format!(
                    "{} image is not present",
                    Self::variant_name(self.variant)
                )));
            }
            let (w, h) = img
                .dimensions()
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
            Ok(JsDimensions {
                width: w,
                height: h,
            })
        })
    }

    /// Rotate this image variant on disk.
    #[napi]
    pub fn rotate(&self, env: Env, degrees: i32) -> napi::Result<()> {
        with_env_stashed(env, || {
            let rotation = CoreRotation::from_degrees(degrees).ok_or_else(|| {
                napi::Error::from_reason(format!(
                    "Invalid rotation: {degrees}°. Use 90, -90, 180, or -180."
                ))
            })?;
            let img = Self::get_image_ref(&self.stack, self.variant);
            if !img.is_present() {
                return Err(napi::Error::from_reason(format!(
                    "{} image is not present",
                    Self::variant_name(self.variant)
                )));
            }
            img.rotate(rotation)
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        })
    }

    /// Clear cached hash and dimensions, forcing re-computation on next access.
    #[napi]
    pub fn invalidate_caches(&self) {
        Self::get_image_ref(&self.stack, self.variant).invalidate_caches();
    }
}

impl JsImageRef {
    fn get_image_ref(
        stack: &photostax_core::photo_stack::PhotoStack,
        variant: u8,
    ) -> photostax_core::photo_stack::ImageProxy<'_> {
        match variant {
            0 => stack.original(),
            1 => stack.enhanced(),
            2 => stack.back(),
            _ => stack.original(),
        }
    }

    fn variant_name(variant: u8) -> &'static str {
        match variant {
            0 => "Original",
            1 => "Enhanced",
            2 => "Back",
            _ => "Unknown",
        }
    }
}

/// Accessor for a photo stack's metadata (EXIF, XMP, sidecar).
///
/// Metadata is lazy-loaded: not read from disk until `read()` is called.
/// Once loaded, it is cached and returned on subsequent calls until
/// `invalidate()` is called.
#[napi(js_name = "MetadataRef")]
pub struct JsMetadataRef {
    stack: CorePhotoStack,
}

// SAFETY: Single-threaded Node.js.
unsafe impl Send for JsMetadataRef {}

#[napi]
impl JsMetadataRef {
    /// Whether metadata has been loaded from disk.
    #[napi(getter)]
    pub fn is_loaded(&self) -> bool {
        self.stack.metadata().is_loaded()
    }

    /// Load and return metadata (cached after first call).
    #[napi]
    pub fn read(&self, env: Env) -> napi::Result<JsMetadata> {
        with_env_stashed(env, || {
            let meta = self.stack
                .metadata()
                .read()
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
            Ok(JsMetadata::from(meta))
        })
    }

    /// Return cached metadata without triggering a load, or null if not yet loaded.
    #[napi(getter)]
    pub fn cached(&self) -> Option<JsMetadata> {
        self.stack.metadata().cached().map(JsMetadata::from)
    }

    /// Write metadata to the sidecar file.
    #[napi]
    pub fn write(&self, env: Env, metadata: JsMetadata) -> napi::Result<()> {
        with_env_stashed(env, || {
            let core_metadata: CoreMetadata = metadata.into();
            self.stack
                .metadata()
                .write(&core_metadata)
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        })
    }

    /// Clear cached metadata, forcing re-load on next read().
    #[napi]
    pub fn invalidate(&self) {
        self.stack.metadata().invalidate();
    }
}

/// A photo stack with sub-object accessors for images and metadata.
///
/// Get stacks via `PhotostaxRepository.scan()`, `StackManager.query()`, etc.
#[napi(js_name = "PhotoStack")]
pub struct JsPhotoStack {
    stack: CorePhotoStack,
}

// SAFETY: Single-threaded Node.js — all access on main thread via RefCell.
unsafe impl Send for JsPhotoStack {}

#[napi]
impl JsPhotoStack {
    /// Unique stack identifier (opaque hash derived from base filename).
    #[napi(getter)]
    pub fn id(&self) -> String {
        self.stack.id()
    }

    /// Human-readable display name.
    #[napi(getter)]
    pub fn name(&self) -> String {
        self.stack.name()
    }

    /// Subfolder within the repository, or null for root-level stacks.
    #[napi(getter)]
    pub fn folder(&self) -> Option<String> {
        self.stack.folder()
    }

    /// Base directory where stack files live.
    #[napi(getter)]
    pub fn location(&self) -> Option<String> {
        self.stack.location()
    }

    /// Which image variants are present in this stack.
    #[napi(getter)]
    pub fn images_present(&self) -> JsImagesPresent {
        JsImagesPresent {
            original: self.stack.original().is_present(),
            enhanced: self.stack.enhanced().is_present(),
            back: self.stack.back().is_present(),
        }
    }

    /// Whether this stack has any image variant present.
    #[napi(getter)]
    pub fn has_any_image(&self) -> bool {
        self.stack.has_any_image()
    }

    /// Accessor for the original (raw scan) image.
    #[napi(getter)]
    pub fn original(&self) -> JsImageRef {
        JsImageRef {
            stack: self.stack.clone(),
            variant: 0,
        }
    }

    /// Accessor for the enhanced (color-corrected) image.
    #[napi(getter)]
    pub fn enhanced(&self) -> JsImageRef {
        JsImageRef {
            stack: self.stack.clone(),
            variant: 1,
        }
    }

    /// Accessor for the back-of-photo image.
    #[napi(getter)]
    pub fn back(&self) -> JsImageRef {
        JsImageRef {
            stack: self.stack.clone(),
            variant: 2,
        }
    }

    /// Accessor for stack metadata (lazy-loading).
    #[napi(getter)]
    pub fn metadata(&self) -> JsMetadataRef {
        JsMetadataRef {
            stack: self.stack.clone(),
        }
    }

    /// Swap front and back images (for accidentally backward-scanned photos).
    ///
    /// After swap: original ← old back, back ← old original, enhanced ← deleted.
    #[napi]
    pub fn swap_front_back(&self, env: Env) -> napi::Result<()> {
        with_env_stashed(env, || {
            self.stack
                .swap_front_back()
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        })
    }

    /// Rotate images in this stack.
    /// @param degrees - Rotation degrees: 90, -90, 180, -180
    /// @param target - Which images: "all" (default), "front", "back"
    #[napi]
    pub fn rotate(&self, env: Env, degrees: i32, target: Option<String>) -> napi::Result<()> {
        with_env_stashed(env, || {
            let rotation = CoreRotation::from_degrees(degrees).ok_or_else(|| {
                napi::Error::from_reason(format!(
                    "Invalid rotation: {degrees}°. Use 90, -90, 180, or -180."
                ))
            })?;
            let rotation_target = match target.as_deref() {
                None | Some("all") => CoreRotationTarget::All,
                Some("front") => CoreRotationTarget::Front,
                Some("back") => CoreRotationTarget::Back,
                Some(other) => {
                    return Err(napi::Error::from_reason(format!(
                        "Invalid rotation target: '{other}'. Accepted values: all, front, back"
                    )));
                }
            };
            let stack = &self.stack;
            match rotation_target {
                CoreRotationTarget::All => {
                    if stack.original().is_present() {
                        stack
                            .original()
                            .rotate(rotation)
                            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
                    }
                    if stack.enhanced().is_present() {
                        stack
                            .enhanced()
                            .rotate(rotation)
                            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
                    }
                    if stack.back().is_present() {
                        stack
                            .back()
                            .rotate(rotation)
                            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
                    }
                }
                CoreRotationTarget::Front => {
                    if stack.original().is_present() {
                        stack
                            .original()
                            .rotate(rotation)
                            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
                    }
                    if stack.enhanced().is_present() {
                        stack
                            .enhanced()
                            .rotate(rotation)
                            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
                    }
                }
                CoreRotationTarget::Back => {
                    if stack.back().is_present() {
                        stack
                            .back()
                            .rotate(rotation)
                            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
                    }
                }
            }
            Ok(())
        })
    }
}

impl JsPhotoStack {
    fn from_core(stack: &CorePhotoStack) -> Self {
        Self {
            stack: stack.clone(),
        }
    }
}

/// A key-value pair for search filters.
#[napi(object)]
pub struct JsKeyValue {
    /// The tag name to filter on
    pub key: String,
    /// The value substring to search for
    pub value: String,
}

/// Query parameters for searching photo stacks.
///
/// All filters use AND logic - a stack must match all specified criteria.
#[napi(object)]
pub struct JsSearchQuery {
    /// Free-text search across ID and all metadata
    pub text: Option<String>,
    /// EXIF tag filters (all must match)
    pub exif_filters: Option<Vec<JsKeyValue>>,
    /// Custom tag filters (all must match)
    pub custom_filters: Option<Vec<JsKeyValue>>,
    /// Filter by presence of back scan
    pub has_back: Option<bool>,
    /// Filter by presence of enhanced scan
    pub has_enhanced: Option<bool>,
    /// Allowlist of stack IDs — only stacks with matching IDs are returned
    pub stack_ids: Option<Vec<String>>,
}

impl From<JsSearchQuery> for CoreSearchQuery {
    fn from(q: JsSearchQuery) -> Self {
        let mut query = CoreSearchQuery::new();

        if let Some(text) = q.text {
            query = query.with_text(text);
        }

        if let Some(filters) = q.exif_filters {
            for kv in filters {
                query = query.with_exif_filter(kv.key, kv.value);
            }
        }

        if let Some(filters) = q.custom_filters {
            for kv in filters {
                query = query.with_custom_filter(kv.key, kv.value);
            }
        }

        if let Some(has_back) = q.has_back {
            query = query.with_has_back(has_back);
        }

        if let Some(has_enhanced) = q.has_enhanced {
            query = query.with_has_enhanced(has_enhanced);
        }

        if let Some(ids) = q.stack_ids {
            query = query.with_ids(ids);
        }

        query
    }
}

/// A paginated result containing a page of photo stacks and pagination metadata.
#[napi(js_name = "PaginatedResult")]
pub struct JsPaginatedResult {
    stacks: Vec<CorePhotoStack>,
    total_count: u32,
    offset: u32,
    limit: u32,
    has_more: bool,
}

// SAFETY: Single-threaded Node.js.
unsafe impl Send for JsPaginatedResult {}

#[napi]
impl JsPaginatedResult {
    /// The photo stacks in this page.
    #[napi(getter)]
    pub fn items(&self) -> Vec<JsPhotoStack> {
        self.stacks
            .iter()
            .map(|s| JsPhotoStack::from_core(s))
            .collect()
    }

    /// Total number of stacks across all pages.
    #[napi(getter)]
    pub fn total_count(&self) -> u32 {
        self.total_count
    }

    /// The offset used for this page.
    #[napi(getter)]
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// The page size limit used for this page.
    #[napi(getter)]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// Whether there are more items beyond this page.
    #[napi(getter)]
    pub fn has_more(&self) -> bool {
        self.has_more
    }
}

/// A query result with page-based navigation.
///
/// Returned by `query()` methods on `PhotostaxRepository` and `StackManager`.
/// Provides page-based navigation over the result set: `nextPage()`, `previousPage()`,
/// `setPage()`, `getPage()`, plus `allStacks` for the full result.
#[napi(js_name = "QueryResult")]
pub struct JsQueryResult {
    stacks: Vec<CorePhotoStack>,
    page_size: usize,
    current_page_index: usize,
}

// SAFETY: Single-threaded Node.js.
unsafe impl Send for JsQueryResult {}

#[napi]
impl JsQueryResult {
    /// Total number of stacks across all pages.
    #[napi(getter)]
    pub fn total_count(&self) -> u32 {
        self.stacks.len() as u32
    }

    /// Number of pages (ceiling division of totalCount / pageSize).
    #[napi(getter)]
    pub fn page_count(&self) -> u32 {
        if self.page_size == 0 {
            1
        } else {
            ((self.stacks.len() + self.page_size - 1) / self.page_size) as u32
        }
    }

    /// The page size used for pagination.
    #[napi(getter)]
    pub fn page_size(&self) -> u32 {
        self.page_size as u32
    }

    /// Zero-based index of the current page.
    #[napi(getter)]
    pub fn current_page_index(&self) -> u32 {
        self.current_page_index as u32
    }

    /// Whether there are more pages after the current one.
    #[napi(getter)]
    pub fn has_more(&self) -> bool {
        let pc = self.page_count();
        pc > 0 && (self.current_page_index as u32 + 1) < pc
    }

    /// Stacks in the current page.
    #[napi(getter)]
    pub fn current_page(&self) -> Vec<JsPhotoStack> {
        self.get_page_stacks(self.current_page_index)
    }

    /// Advance to the next page. Returns the stacks, or null if already on the last page.
    #[napi]
    pub fn next_page(&mut self) -> Option<Vec<JsPhotoStack>> {
        let next = self.current_page_index + 1;
        if next >= self.page_count() as usize {
            None
        } else {
            self.current_page_index = next;
            Some(self.get_page_stacks(next))
        }
    }

    /// Go back to the previous page. Returns the stacks, or null if already on the first page.
    #[napi]
    pub fn previous_page(&mut self) -> Option<Vec<JsPhotoStack>> {
        if self.current_page_index == 0 {
            None
        } else {
            self.current_page_index -= 1;
            Some(self.get_page_stacks(self.current_page_index))
        }
    }

    /// Jump to a specific page (0-based). Returns the stacks, or null if out of bounds.
    #[napi]
    pub fn set_page(&mut self, page_index: u32) -> Option<Vec<JsPhotoStack>> {
        let idx = page_index as usize;
        if idx >= self.page_count() as usize {
            None
        } else {
            self.current_page_index = idx;
            Some(self.get_page_stacks(idx))
        }
    }

    /// Get stacks from a specific page (0-based) without changing current page. Returns null if out of bounds.
    #[napi]
    pub fn get_page(&self, page_index: u32) -> Option<Vec<JsPhotoStack>> {
        let idx = page_index as usize;
        if idx >= self.page_count() as usize {
            None
        } else {
            Some(self.get_page_stacks(idx))
        }
    }

    /// All stacks across all pages.
    #[napi(getter)]
    pub fn all_stacks(&self) -> Vec<JsPhotoStack> {
        self.stacks
            .iter()
            .map(|s| JsPhotoStack { stack: s.clone() })
            .collect()
    }

    /// Sub-query: filter this result and return a new QueryResult.
    ///
    /// @param query - Search criteria (null/undefined for all stacks)
    /// @param pageSize - Page size for the new result (default: same as this result)
    #[napi]
    pub fn query(
        &self,
        query: Option<JsSearchQuery>,
        page_size: Option<u32>,
    ) -> JsQueryResult {
        let filtered = match query {
            Some(q) => {
                let core_query: CoreSearchQuery = q.into();
                filter_stacks(&self.stacks, &core_query)
            }
            None => self.stacks.clone(),
        };
        let ps = page_size.map(|p| p as usize).unwrap_or(self.page_size);
        JsQueryResult {
            stacks: filtered,
            page_size: ps,
            current_page_index: 0,
        }
    }
}

impl JsQueryResult {
    fn get_page_stacks(&self, page_index: usize) -> Vec<JsPhotoStack> {
        if self.stacks.is_empty() || self.page_size == 0 {
            return self.stacks
                .iter()
                .map(|s| JsPhotoStack { stack: s.clone() })
                .collect();
        }
        let start = page_index * self.page_size;
        if start >= self.stacks.len() {
            return Vec::new();
        }
        let end = (start + self.page_size).min(self.stacks.len());
        self.stacks[start..end]
            .iter()
            .map(|s| JsPhotoStack { stack: s.clone() })
            .collect()
    }

    fn from_all_stacks(stacks: Vec<CorePhotoStack>) -> Self {
        Self {
            stacks,
            page_size: 0,
            current_page_index: 0,
        }
    }
}

/// Options for creating a PhotostaxRepository.
#[napi(object)]
pub struct RepositoryOptions {
    /// Whether to recurse into subdirectories (default: false).
    ///
    /// Set to `true` when the photo library uses FastFoto's folder-based
    /// organisation (e.g. `1984_Mexico/`, `SteveJones/`).
    pub recursive: Option<bool>,
}

/// A repository for accessing Epson FastFoto photo stacks from a local directory.
///
/// Provides methods to scan, retrieve, and modify photo stacks and their metadata.
#[napi]
pub struct PhotostaxRepository {
    inner: Rc<std::cell::RefCell<StackManager>>,
}

// SAFETY: Single-threaded Node.js.
unsafe impl Send for PhotostaxRepository {}

#[napi]
impl PhotostaxRepository {
    /// Create a new repository rooted at the given directory path.
    ///
    /// @param directoryPath - Path to the directory containing FastFoto photo files
    /// @param options - Optional configuration (e.g. `{ recursive: true }`)
    /// @throws Error if the path is invalid
    #[napi(constructor)]
    pub fn new(directory_path: String, options: Option<RepositoryOptions>) -> napi::Result<Self> {
        let recursive = options.as_ref().and_then(|o| o.recursive).unwrap_or(false);
        let config = photostax_core::scanner::ScannerConfig {
            recursive,
            ..photostax_core::scanner::ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(PathBuf::from(directory_path), config);
        let mgr = StackManager::single(Box::new(repo), CoreScannerProfile::Auto)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(Self {
            inner: Rc::new(std::cell::RefCell::new(mgr)),
        })
    }

    /// Scan the repository and return all discovered photo stacks (fast, no file-based metadata).
    ///
    /// Returns stacks with paths and folder-derived metadata only.
    /// Use `scanWithMetadata()` to load EXIF/XMP/sidecar data for all stacks,
    /// or `loadMetadata(stackId)` to load metadata for individual stacks on demand.
    ///
    /// @returns Array of photo stacks found in the repository
    /// @throws Error if the directory cannot be accessed
    #[napi]
    pub fn scan(&self) -> napi::Result<JsQueryResult> {
        let mut mgr = self.inner.borrow_mut();
        let snapshot = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(JsQueryResult::from_all_stacks(snapshot.all_stacks().to_vec()))
    }

    /// Scan with a scanner profile and progress callback.
    ///
    /// The `profile` tells the engine how the FastFoto was configured:
    /// - `"auto"` — unknown config, uses pixel analysis for ambiguous `_a` (default)
    /// - `"enhanced_and_back"` — `_a` = enhanced, `_b` = back (no I/O)
    /// - `"enhanced_only"` — `_a` = enhanced, no back files (no I/O)
    /// - `"original_only"` — no `_a` or `_b` expected (no I/O)
    ///
    /// The `callback` receives `{ phase: string, current: number, total: number }`.
    /// Phase is one of `"scanning"`, `"classifying"`, or `"complete"`.
    ///
    /// @param profile - Scanner profile string (default: "auto")
    /// @param callback - Progress callback function
    /// @returns Array of photo stacks
    /// @throws Error if the directory cannot be accessed
    #[napi(
        ts_args_type = "profile?: string, callback?: (phase: string, current: number, total: number) => void"
    )]
    pub fn scan_with_progress(
        &self,
        profile: Option<String>,
        callback: Option<JsFunction>,
    ) -> napi::Result<JsQueryResult> {
        let scanner_profile = match profile.as_deref() {
            Some("enhanced_and_back") => CoreScannerProfile::EnhancedAndBack,
            Some("enhanced_only") => CoreScannerProfile::EnhancedOnly,
            Some("original_only") => CoreScannerProfile::OriginalOnly,
            _ => CoreScannerProfile::Auto,
        };

        let mut cb_wrapper;
        let progress: Option<&mut dyn FnMut(&photostax_core::photo_stack::ScanProgress)> =
            if let Some(ref js_fn) = callback {
                cb_wrapper = |p: &photostax_core::photo_stack::ScanProgress| {
                    let phase = match p.phase {
                        photostax_core::photo_stack::ScanPhase::Scanning => "scanning",
                        photostax_core::photo_stack::ScanPhase::Classifying => "classifying",
                        photostax_core::photo_stack::ScanPhase::Complete => "complete",
                    };
                    let _ = js_fn.call3::<String, u32, u32, Unknown>(
                        phase.to_string(),
                        p.current as u32,
                        p.total as u32,
                    );
                };
                Some(&mut cb_wrapper)
            } else {
                None
            };

        let mut mgr = self.inner.borrow_mut();
        mgr.set_profile(scanner_profile);
        let snapshot = mgr
            .query(None, None, progress)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(JsQueryResult::from_all_stacks(snapshot.all_stacks().to_vec()))
    }

    /// Scan the repository and return all photo stacks with full metadata loaded.
    ///
    /// This is the slower path that reads EXIF, XMP, and sidecar data for every stack.
    /// Prefer `scan()` + `loadMetadata()` for lazy-loading in large repositories.
    ///
    /// @returns Array of photo stacks with complete metadata
    /// @throws Error if the directory cannot be accessed
    #[napi]
    pub fn scan_with_metadata(&self) -> napi::Result<JsQueryResult> {
        let mut mgr = self.inner.borrow_mut();
        mgr.invalidate_cache();
        let initial = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        for s in initial.all_stacks() {
            let _ = s.metadata().read();
        }
        let snapshot = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(JsQueryResult::from_all_stacks(snapshot.all_stacks().to_vec()))
    }

    /// Search for photo stacks matching the given query.
    ///
    /// @param query - Search criteria (all filters are AND'd together)
    /// @returns Array of matching photo stacks
    /// @throws Error if the repository cannot be scanned
    #[napi]
    pub fn search(&self, query: JsSearchQuery) -> napi::Result<Vec<JsPhotoStack>> {
        let mut mgr = self.inner.borrow_mut();
        mgr.invalidate_cache();
        let initial = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        for s in initial.all_stacks() {
            let _ = s.metadata().read();
        }
        let core_query: CoreSearchQuery = query.into();
        let snapshot = mgr
            .query(Some(&core_query), None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        drop(mgr);

        Ok(snapshot
            .all_stacks()
            .iter()
            .map(|s| JsPhotoStack::from_core(s))
            .collect())
    }

    /// Unified query: search + paginate in a single call.
    ///
    /// This is the preferred way to retrieve stacks. Combines filtering and
    /// page-based navigation into one operation. Triggers an auto-scan if the
    /// cache is empty.
    ///
    /// @param query - Search criteria (null/undefined for all stacks)
    /// @param pageSize - Number of stacks per page (null/undefined = all on one page)
    /// @param callback - Optional progress callback invoked during auto-scan
    /// @returns QueryResult with page navigation methods
    #[napi(ts_args_type = "query?: SearchQuery, pageSize?: number, callback?: (phase: string, current: number, total: number) => void")]
    pub fn query(
        &self,
        query: Option<JsSearchQuery>,
        page_size: Option<u32>,
        callback: Option<JsFunction>,
    ) -> napi::Result<JsQueryResult> {
        let mut cb_wrapper;
        let progress: Option<&mut dyn FnMut(&photostax_core::photo_stack::ScanProgress)> =
            if let Some(ref js_fn) = callback {
                cb_wrapper = |p: &photostax_core::photo_stack::ScanProgress| {
                    let phase = match p.phase {
                        photostax_core::photo_stack::ScanPhase::Scanning => "scanning",
                        photostax_core::photo_stack::ScanPhase::Classifying => "classifying",
                        photostax_core::photo_stack::ScanPhase::Complete => "complete",
                    };
                    let _ = js_fn.call3::<String, u32, u32, Unknown>(
                        phase.to_string(),
                        p.current as u32,
                        p.total as u32,
                    );
                };
                Some(&mut cb_wrapper)
            } else {
                None
            };

        let mut mgr = self.inner.borrow_mut();
        let core_query = match query {
            Some(q) => q.into(),
            None => CoreSearchQuery::new(),
        };
        let ps = page_size.map(|p| p as usize);
        let snapshot = mgr
            .query(Some(&core_query), ps, progress)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(JsQueryResult {
            stacks: snapshot.all_stacks().to_vec(),
            page_size: ps.unwrap_or(0),
            current_page_index: 0,
        })
    }

    /// Scan the repository and return a paginated page of photo stacks.
    ///
    /// @param offset - Number of stacks to skip (0-based)
    /// @param limit - Maximum number of stacks to return per page
    /// @param loadMetadata - When true, loads EXIF/XMP/sidecar metadata for each stack in the page
    /// @returns Paginated result with items and metadata
    /// @throws Error if the directory cannot be accessed
    #[napi]
    pub fn scan_paginated(
        &self,
        offset: u32,
        limit: u32,
        load_metadata: Option<bool>,
    ) -> napi::Result<JsPaginatedResult> {
        let mut mgr = self.inner.borrow_mut();
        mgr.invalidate_cache();
        let initial = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        if load_metadata.unwrap_or(false) {
            for s in initial.all_stacks() {
                let _ = s.metadata().read();
            }
        }
        let snapshot = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let paginated = snapshot.snapshot().get_page(offset as usize, limit as usize);
        drop(mgr);

        Ok(JsPaginatedResult {
            stacks: paginated.items.to_vec(),
            total_count: paginated.total_count as u32,
            offset: paginated.offset as u32,
            limit: paginated.limit as u32,
            has_more: paginated.has_more,
        })
    }

    /// Search for photo stacks with pagination.
    ///
    /// @param query - Search criteria (all filters are AND'd together)
    /// @param offset - Number of stacks to skip (0-based)
    /// @param limit - Maximum number of stacks to return per page
    /// @returns Paginated result with matching items and metadata
    /// @throws Error if the repository cannot be scanned
    #[napi]
    pub fn search_paginated(
        &self,
        query: JsSearchQuery,
        offset: u32,
        limit: u32,
    ) -> napi::Result<JsPaginatedResult> {
        let mut mgr = self.inner.borrow_mut();
        mgr.invalidate_cache();
        let initial = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        for s in initial.all_stacks() {
            let _ = s.metadata().read();
        }

        let core_query: CoreSearchQuery = query.into();
        let snapshot = mgr
            .query(Some(&core_query), None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let paginated = snapshot.snapshot().get_page(offset as usize, limit as usize);
        drop(mgr);

        Ok(JsPaginatedResult {
            stacks: paginated.items.to_vec(),
            total_count: paginated.total_count as u32,
            offset: paginated.offset as u32,
            limit: paginated.limit as u32,
            has_more: paginated.has_more,
        })
    }

    /// Create a point-in-time snapshot for consistent pagination.
    ///
    /// The snapshot captures the current set of stacks so that page requests
    /// always see the same total count and ordering, even if files are added
    /// or removed on disk between page calls.
    ///
    /// @param loadMetadata - When true, loads EXIF/XMP/sidecar metadata for every stack
    /// @returns A frozen snapshot that supports `getPage()` and `filter()`
    /// @throws Error if the scan fails
    #[napi]
    pub fn create_snapshot(&self, load_metadata: Option<bool>) -> napi::Result<JsScanSnapshot> {
        let mut mgr = self.inner.borrow_mut();
        mgr.invalidate_cache();
        let initial = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        if load_metadata.unwrap_or(false) {
            for s in initial.all_stacks() {
                let _ = s.metadata().read();
            }
        }
        let snapshot = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?
            .into_snapshot();
        Ok(JsScanSnapshot {
            inner: snapshot,
        })
    }

    /// Create a snapshot with a scanner profile and progress callback.
    ///
    /// Combines scanning, classification, optional metadata loading, and
    /// snapshot creation in a single pass — no redundant re-scanning.
    ///
    /// @param profile - Scanner profile (default: "auto")
    /// @param loadMetadata - When true, loads metadata for every stack
    /// @param callback - Progress callback `(phase, current, total) => void`
    /// @returns A frozen snapshot
    /// @throws Error if the scan fails
    #[napi(
        ts_args_type = "profile?: string, loadMetadata?: boolean, callback?: (phase: string, current: number, total: number) => void"
    )]
    pub fn create_snapshot_with_progress(
        &self,
        profile: Option<String>,
        load_metadata: Option<bool>,
        callback: Option<JsFunction>,
    ) -> napi::Result<JsScanSnapshot> {
        let scanner_profile = match profile.as_deref() {
            Some("enhanced_and_back") => CoreScannerProfile::EnhancedAndBack,
            Some("enhanced_only") => CoreScannerProfile::EnhancedOnly,
            Some("original_only") => CoreScannerProfile::OriginalOnly,
            _ => CoreScannerProfile::Auto,
        };

        let mut cb_wrapper;
        let progress: Option<&mut dyn FnMut(&photostax_core::photo_stack::ScanProgress)> =
            if let Some(ref js_fn) = callback {
                cb_wrapper = |p: &photostax_core::photo_stack::ScanProgress| {
                    let phase = match p.phase {
                        photostax_core::photo_stack::ScanPhase::Scanning => "scanning",
                        photostax_core::photo_stack::ScanPhase::Classifying => "classifying",
                        photostax_core::photo_stack::ScanPhase::Complete => "complete",
                    };
                    let _ = js_fn.call3::<String, u32, u32, Unknown>(
                        phase.to_string(),
                        p.current as u32,
                        p.total as u32,
                    );
                };
                Some(&mut cb_wrapper)
            } else {
                None
            };

        let mut mgr = self.inner.borrow_mut();
        mgr.set_profile(scanner_profile);
        mgr.invalidate_cache();
        let initial = mgr
            .query(None, None, progress)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        if load_metadata.unwrap_or(false) {
            for s in initial.all_stacks() {
                let _ = s.metadata().read();
            }
        }
        let snapshot = mgr
            .query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?
            .into_snapshot();
        Ok(JsScanSnapshot {
            inner: snapshot,
        })
    }

    /// Check whether a snapshot is still current.
    ///
    /// Performs a fast re-scan and compares against the snapshot to detect
    /// added or removed stacks. Use this to decide when to create a new snapshot.
    ///
    /// @param snapshot - The snapshot to check
    /// @returns Status information including staleness and change counts
    /// @throws Error if the re-scan fails
    #[napi]
    pub fn check_snapshot_status(
        &self,
        snapshot: &JsScanSnapshot,
    ) -> napi::Result<JsSnapshotStatus> {
        let mut mgr = self.inner.borrow_mut();
        mgr.invalidate_cache();
        mgr.query(None, None, None)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let status = mgr.check_status(&snapshot.inner);
        Ok(JsSnapshotStatus {
            is_stale: status.is_stale,
            snapshot_count: status.snapshot_count as u32,
            current_count: status.current_count as u32,
            added: status.added as u32,
            removed: status.removed as u32,
        })
    }
}

/// Result of checking a snapshot's staleness.
#[napi(object)]
#[derive(Clone)]
pub struct JsSnapshotStatus {
    /// True when the filesystem no longer matches the snapshot.
    pub is_stale: bool,
    /// Number of stacks in the snapshot.
    pub snapshot_count: u32,
    /// Number of stacks currently on disk.
    pub current_count: u32,
    /// New stacks on disk that were not in the snapshot.
    pub added: u32,
    /// Snapshot stacks no longer present on disk.
    pub removed: u32,
}

/// A point-in-time snapshot of scanned photo stacks.
///
/// Pages from a snapshot always have a consistent total count, even if the
/// underlying filesystem changes between page requests.
#[napi]
pub struct JsScanSnapshot {
    inner: CoreScanSnapshot,
}

// SAFETY: Single-threaded Node.js.
unsafe impl Send for JsScanSnapshot {}

#[napi]
impl JsScanSnapshot {
    /// Total number of stacks in the snapshot.
    #[napi(getter)]
    pub fn total_count(&self) -> u32 {
        self.inner.total_count() as u32
    }

    /// Get a page of stacks from the snapshot.
    ///
    /// This is a pure in-memory operation — it never touches the filesystem
    /// and always returns a consistent page.
    ///
    /// @param offset - Number of stacks to skip (0-based)
    /// @param limit - Maximum number of stacks to return per page
    /// @returns Paginated result with items and metadata
    #[napi]
    pub fn get_page(&self, offset: u32, limit: u32) -> JsPaginatedResult {
        let paginated = self.inner.get_page(offset as usize, limit as usize);
        JsPaginatedResult {
            stacks: paginated.items.to_vec(),
            total_count: paginated.total_count as u32,
            offset: paginated.offset as u32,
            limit: paginated.limit as u32,
            has_more: paginated.has_more,
        }
    }

    /// Filter the snapshot by a search query, returning a new snapshot.
    ///
    /// The resulting snapshot contains only stacks matching the query.
    /// All page counts are recalculated against the filtered set.
    ///
    /// @param query - Search criteria (all filters are AND'd together)
    /// @returns A new snapshot containing only matching stacks
    #[napi]
    pub fn filter(&self, query: JsSearchQuery) -> JsScanSnapshot {
        let core_query: CoreSearchQuery = query.into();
        JsScanSnapshot {
            inner: self.inner.filter(&core_query),
        }
    }
}

// ── Shared helpers for StackManager operations ─────────────────────────

fn mgr_scan(mgr: &Rc<std::cell::RefCell<StackManager>>) -> napi::Result<Vec<JsPhotoStack>> {
    let mut m = mgr.borrow_mut();
    let snapshot = m
        .query(None, None, None)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(snapshot
        .all_stacks()
        .iter()
        .map(|s| JsPhotoStack::from_core(s))
        .collect())
}

fn mgr_scan_with_metadata(
    mgr: &Rc<std::cell::RefCell<StackManager>>,
) -> napi::Result<Vec<JsPhotoStack>> {
    let mut m = mgr.borrow_mut();
    m.invalidate_cache();
    let initial = m
        .query(None, None, None)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    for s in initial.all_stacks() {
        let _ = s.metadata().read();
    }
    let snapshot = m
        .query(None, None, None)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(snapshot
        .all_stacks()
        .iter()
        .map(|s| JsPhotoStack::from_core(s))
        .collect())
}

fn mgr_query(
    mgr: &Rc<std::cell::RefCell<StackManager>>,
    query: Option<JsSearchQuery>,
    page_size: Option<u32>,
    callback: Option<JsFunction>,
) -> napi::Result<JsQueryResult> {
    let mut cb_wrapper;
    let progress: Option<&mut dyn FnMut(&photostax_core::photo_stack::ScanProgress)> =
        if let Some(ref js_fn) = callback {
            cb_wrapper = |p: &photostax_core::photo_stack::ScanProgress| {
                let phase = match p.phase {
                    photostax_core::photo_stack::ScanPhase::Scanning => "scanning",
                    photostax_core::photo_stack::ScanPhase::Classifying => "classifying",
                    photostax_core::photo_stack::ScanPhase::Complete => "complete",
                };
                let _ = js_fn.call3::<String, u32, u32, Unknown>(
                    phase.to_string(),
                    p.current as u32,
                    p.total as u32,
                );
            };
            Some(&mut cb_wrapper)
        } else {
            None
        };

    let mut m = mgr.borrow_mut();
    let core_query = match query {
        Some(q) => q.into(),
        None => CoreSearchQuery::new(),
    };
    let ps = page_size.map(|p| p as usize);
    let snapshot = m
        .query(Some(&core_query), ps, progress)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(JsQueryResult {
        stacks: snapshot.all_stacks().to_vec(),
        page_size: ps.unwrap_or(0),
        current_page_index: 0,
    })
}

fn mgr_create_snapshot(
    mgr: &Rc<std::cell::RefCell<StackManager>>,
    load_metadata: Option<bool>,
) -> napi::Result<JsScanSnapshot> {
    let mut m = mgr.borrow_mut();
    m.invalidate_cache();
    let initial = m
        .query(None, None, None)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    if load_metadata.unwrap_or(false) {
        for s in initial.all_stacks() {
            let _ = s.metadata().read();
        }
    }
    let snapshot = m
        .query(None, None, None)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
        .into_snapshot();
    Ok(JsScanSnapshot {
        inner: snapshot,
    })
}

// ── NapiProvider: JS object → RepositoryProvider bridge ────────────────

/// A file entry for foreign repository providers.
#[napi(object)]
pub struct JsFileEntry {
    /// File name including extension (e.g., "IMG_001_a.jpg").
    pub name: String,
    /// Relative folder path (empty string for root).
    pub folder: String,
    /// Full path or URI to the file.
    pub path: String,
    /// File size in bytes.
    pub size: f64, // JS numbers are f64
}

/// A RepositoryProvider backed by a JS object reference.
///
/// Stores pre-collected file entries and a reference to the JS provider object
/// for lazy file I/O. The JS provider must implement:
/// - `listEntries(prefix: string, recursive: boolean): FileEntry[]`
/// - `readFile(path: string): Buffer`
/// - `writeFile(path: string, data: Buffer): void`
struct NapiProvider {
    location: String,
    cached_entries: Vec<photostax_core::scanner::FileEntry>,
    /// Raw napi_ref to the JS provider object (prevents GC).
    provider_ref: napi::sys::napi_ref,
    /// The napi_env at creation time (valid for the addon's lifetime on main thread).
    env_raw: napi::sys::napi_env,
}

// SAFETY: StackManager uses RefCell (single-threaded). All access happens on
// the main Node.js thread. The napi references and env pointer are only used
// from that same thread.
unsafe impl Send for NapiProvider {}
unsafe impl Sync for NapiProvider {}

impl Drop for NapiProvider {
    fn drop(&mut self) {
        unsafe {
            napi::sys::napi_delete_reference(self.env_raw, self.provider_ref);
        }
    }
}

impl photostax_core::backends::foreign::RepositoryProvider for NapiProvider {
    fn location(&self) -> &str {
        &self.location
    }

    fn list_entries(
        &self,
        _prefix: &str,
        _recursive: bool,
    ) -> io::Result<Vec<photostax_core::scanner::FileEntry>> {
        Ok(self.cached_entries.clone())
    }

    fn open_read(&self, path: &str) -> io::Result<Box<dyn photostax_core::file_access::ReadSeek>> {
        let env = get_stashed_env()?;

        let provider = self.get_js_object(&env)?;
        let read_fn: napi::JsFunction = provider
            .get_named_property("readFile")
            .map_err(|e| io::Error::other(format!("provider.readFile not found: {e}")))?;

        let path_val = env
            .create_string(path)
            .map_err(|e| io::Error::other(format!("failed to create string: {e}")))?;

        let result = read_fn
            .call(Some(&provider), &[path_val])
            .map_err(|e| io::Error::other(format!("readFile() failed: {e}")))?;

        let buffer = unsafe {
            result
                .cast::<napi::JsBuffer>()
                .into_value()
                .map_err(|e| io::Error::other(format!("readFile() did not return Buffer: {e}")))?
        };

        Ok(Box::new(Cursor::new(buffer.to_vec())))
    }

    fn open_write(&self, path: &str) -> io::Result<Box<dyn io::Write + Send>> {
        // Validate we can access the provider (env is stashed)
        let env = get_stashed_env()?;
        let _provider = self.get_js_object(&env)?;

        // Return a writer that collects bytes, then calls writeFile on drop
        Ok(Box::new(NapiWriter {
            path: path.to_string(),
            buffer: Vec::new(),
            provider_ref: self.provider_ref,
        }))
    }
}

impl NapiProvider {
    fn get_js_object(&self, env: &Env) -> io::Result<napi::JsObject> {
        let mut result = std::ptr::null_mut();
        let status = unsafe {
            napi::sys::napi_get_reference_value(env.raw(), self.provider_ref, &mut result)
        };
        if status != napi::sys::Status::napi_ok || result.is_null() {
            return Err(io::Error::other("failed to get JS provider reference"));
        }
        Ok(unsafe { napi::JsObject::from_raw_unchecked(env.raw(), result) })
    }
}

/// Writer that collects bytes and calls provider.writeFile() on flush/drop.
struct NapiWriter {
    path: String,
    buffer: Vec<u8>,
    provider_ref: napi::sys::napi_ref,
}

// SAFETY: Same single-thread guarantee as NapiProvider.
unsafe impl Send for NapiWriter {}

impl io::Write for NapiWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for NapiWriter {
    fn drop(&mut self) {
        if self.buffer.is_empty() {
            return;
        }

        // Try to call writeFile on the provider
        if let Ok(env) = get_stashed_env() {
            let mut provider_raw = std::ptr::null_mut();
            let status = unsafe {
                napi::sys::napi_get_reference_value(env.raw(), self.provider_ref, &mut provider_raw)
            };
            if status != napi::sys::Status::napi_ok || provider_raw.is_null() {
                return;
            }

            let provider = unsafe { napi::JsObject::from_raw_unchecked(env.raw(), provider_raw) };

            if let Ok(write_fn) = provider.get_named_property::<napi::JsFunction>("writeFile") {
                if let Ok(path_val) = env.create_string(&self.path) {
                    if let Ok(buf_val) =
                        env.create_buffer_with_data(std::mem::take(&mut self.buffer))
                    {
                        let _: napi::Result<napi::JsUnknown> = write_fn.call(
                            Some(&provider),
                            &[path_val.into_unknown(), buf_val.into_raw().into_unknown()],
                        );
                    }
                }
            }
        }
    }
}

// ── PhotostaxStackManager: multi-repo manager ──────────────────────────

/// Options for adding a repository to a StackManager.
#[napi(object)]
pub struct AddRepoOptions {
    /// Whether to recurse into subdirectories (default: false).
    pub recursive: Option<bool>,
    /// Scanner profile: "auto", "enhanced_and_back", "enhanced_only", "original_only"
    pub profile: Option<String>,
}

/// A multi-repository stack manager.
///
/// Use this when you need to manage multiple photo directories as a single
/// unified collection. All stacks from every registered repository are
/// accessible through a single O(1) cache.
///
/// For single-repo convenience, use `PhotostaxRepository` instead.
#[napi(js_name = "StackManager")]
pub struct PhotostaxStackManager {
    inner: Rc<std::cell::RefCell<StackManager>>,
}

// SAFETY: Single-threaded Node.js.
unsafe impl Send for PhotostaxStackManager {}

impl Default for PhotostaxStackManager {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl PhotostaxStackManager {
    /// Create an empty StackManager with no repositories.
    ///
    /// Use `addRepo()` to register one or more directories before scanning.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Rc::new(std::cell::RefCell::new(StackManager::new())),
        }
    }

    /// Register a repository directory.
    ///
    /// Multiple directories can be added — all will be scanned together and
    /// their stacks merged into a single cache with globally unique IDs.
    /// Overlapping directories within the same URI scheme are rejected.
    ///
    /// @param directoryPath - Path to the directory containing FastFoto files
    /// @param options - Optional configuration (recursive, profile)
    /// @throws Error if the path overlaps with an existing repo
    #[napi]
    pub fn add_repo(
        &self,
        directory_path: String,
        options: Option<AddRepoOptions>,
    ) -> napi::Result<()> {
        let recursive = options.as_ref().and_then(|o| o.recursive).unwrap_or(false);
        let profile_str = options.as_ref().and_then(|o| o.profile.as_deref());
        let scanner_profile = match profile_str {
            Some("enhanced_and_back") => CoreScannerProfile::EnhancedAndBack,
            Some("enhanced_only") => CoreScannerProfile::EnhancedOnly,
            Some("original_only") => CoreScannerProfile::OriginalOnly,
            _ => CoreScannerProfile::Auto,
        };

        let config = photostax_core::scanner::ScannerConfig {
            recursive,
            ..photostax_core::scanner::ScannerConfig::default()
        };
        let repo = LocalRepository::with_config(PathBuf::from(directory_path), config);
        let mut mgr = self.inner.borrow_mut();
        mgr.add_repo(Box::new(repo), scanner_profile)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(())
    }

    /// Register a foreign repository backed by a JavaScript provider.
    ///
    /// The provider object must implement:
    /// - `location: string` — canonical URI
    /// - `listEntries(prefix: string, recursive: boolean): FileEntry[]`
    /// - `readFile(path: string): Buffer`
    /// - `writeFile(path: string, data: Buffer): void`
    ///
    /// @param provider - Object implementing the RepositoryProvider interface
    /// @param options - Optional configuration (recursive, profile)
    /// @throws Error if the provider is invalid or overlaps with an existing repo
    #[napi]
    pub fn add_foreign_repo(
        &self,
        env: Env,
        #[napi(ts_arg_type = "RepositoryProvider")] provider: napi::JsObject,
        options: Option<AddRepoOptions>,
    ) -> napi::Result<()> {
        let recursive = options.as_ref().and_then(|o| o.recursive).unwrap_or(false);
        let profile_str = options.as_ref().and_then(|o| o.profile.as_deref());
        let scanner_profile = match profile_str {
            Some("enhanced_and_back") => CoreScannerProfile::EnhancedAndBack,
            Some("enhanced_only") => CoreScannerProfile::EnhancedOnly,
            Some("original_only") => CoreScannerProfile::OriginalOnly,
            _ => CoreScannerProfile::Auto,
        };

        // Read location
        let location: String = provider
            .get_named_property::<napi::JsString>("location")
            .map_err(|e| napi::Error::from_reason(format!("provider.location: {e}")))?
            .into_utf8()
            .map_err(|e| napi::Error::from_reason(format!("provider.location not UTF-8: {e}")))?
            .as_str()
            .map_err(|e| napi::Error::from_reason(format!("provider.location: {e}")))?
            .to_string();

        // Eagerly collect entries (lightweight — just filenames and sizes)
        let list_fn: napi::JsFunction = provider
            .get_named_property("listEntries")
            .map_err(|e| napi::Error::from_reason(format!("provider.listEntries: {e}")))?;

        let prefix_val = env.create_string("")?;
        let recursive_val = env.get_boolean(recursive)?;
        let entries_val: napi::JsUnknown = list_fn.call(
            Some(&provider),
            &[prefix_val.into_unknown(), recursive_val.into_unknown()],
        )?;
        let entries_array = unsafe { entries_val.cast::<napi::JsObject>() };
        let len: u32 = entries_array
            .get_named_property::<napi::JsNumber>("length")
            .map_err(|e| napi::Error::from_reason(format!("entries.length: {e}")))?
            .get_uint32()?;

        let mut cached_entries = Vec::with_capacity(len as usize);
        for i in 0..len {
            let entry: napi::JsObject = entries_array.get_element(i)?;
            let name: String = entry
                .get_named_property::<napi::JsString>("name")?
                .into_utf8()?
                .as_str()?
                .to_string();
            let folder: String = entry
                .get_named_property::<napi::JsString>("folder")?
                .into_utf8()?
                .as_str()?
                .to_string();
            let path: String = entry
                .get_named_property::<napi::JsString>("path")?
                .into_utf8()?
                .as_str()?
                .to_string();
            let size: f64 = entry
                .get_named_property::<napi::JsNumber>("size")?
                .get_double()?;

            cached_entries.push(photostax_core::scanner::FileEntry {
                name,
                folder,
                path,
                size: size as u64,
            });
        }

        // Create a persistent reference to the JS provider object
        let mut provider_ref = std::ptr::null_mut();
        let status = unsafe {
            napi::sys::napi_create_reference(env.raw(), provider.raw(), 1, &mut provider_ref)
        };
        if status != napi::sys::Status::napi_ok {
            return Err(napi::Error::from_reason(
                "failed to create reference to provider object",
            ));
        }

        let napi_provider = NapiProvider {
            location,
            cached_entries,
            provider_ref,
            env_raw: env.raw(),
        };

        let config = photostax_core::scanner::ScannerConfig {
            recursive,
            ..photostax_core::scanner::ScannerConfig::default()
        };
        let repo = photostax_core::backends::foreign::ForeignRepository::with_config(
            Box::new(napi_provider),
            config,
        );

        let mut mgr = self.inner.borrow_mut();
        mgr.add_repo(Box::new(repo), scanner_profile)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(())
    }

    /// Number of registered repositories.
    #[napi(getter)]
    pub fn repo_count(&self) -> u32 {
        self.inner.borrow().repo_count() as u32
    }

    /// Total number of stacks in the cache.
    #[napi(getter)]
    pub fn stack_count(&self) -> u32 {
        self.inner.borrow().len() as u32
    }

    /// Scan all registered repos and return all discovered stacks.
    #[napi]
    pub fn scan(&self, env: Env) -> napi::Result<Vec<JsPhotoStack>> {
        with_env_stashed(env, || mgr_scan(&self.inner))
    }

    /// Scan all repos and load full metadata for every stack.
    #[napi]
    pub fn scan_with_metadata(&self, env: Env) -> napi::Result<Vec<JsPhotoStack>> {
        with_env_stashed(env, || mgr_scan_with_metadata(&self.inner))
    }

    /// Unified query: search + paginate across all repos.
    #[napi(ts_args_type = "query?: SearchQuery, pageSize?: number, callback?: (phase: string, current: number, total: number) => void")]
    pub fn query(
        &self,
        env: Env,
        query: Option<JsSearchQuery>,
        page_size: Option<u32>,
        callback: Option<JsFunction>,
    ) -> napi::Result<JsQueryResult> {
        with_env_stashed(env, || mgr_query(&self.inner, query, page_size, callback))
    }

    /// Create a point-in-time snapshot across all repos.
    #[napi]
    pub fn create_snapshot(
        &self,
        env: Env,
        load_metadata: Option<bool>,
    ) -> napi::Result<JsScanSnapshot> {
        with_env_stashed(env, || mgr_create_snapshot(&self.inner, load_metadata))
    }
}
