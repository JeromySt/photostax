//! Node.js native addon for photostax using napi-rs.
//!
//! This crate provides JavaScript/TypeScript bindings for the photostax-core library,
//! enabling Node.js applications to work with Epson FastFoto photo stacks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use napi::bindgen_prelude::*;
use napi_derive::napi;

use photostax_core::backends::local::LocalRepository;
use photostax_core::photo_stack::{Metadata as CoreMetadata, PhotoStack as CorePhotoStack, Rotation as CoreRotation, RotationTarget as CoreRotationTarget, ScannerProfile as CoreScannerProfile};
use photostax_core::repository::Repository;
use photostax_core::search::{filter_stacks, paginate_stacks, PaginationParams, SearchQuery as CoreSearchQuery};
use photostax_core::snapshot::ScanSnapshot as CoreScanSnapshot;

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

/// A unified representation of a single scanned photo from an Epson FastFoto scanner.
///
/// Groups the original scan, enhanced version, and back-of-photo image into
/// a single logical unit with associated metadata.
#[napi(object)]
#[derive(Clone)]
pub struct JsPhotoStack {
    /// Unique identifier derived from the base filename
    pub id: String,
    /// Path to the original front scan (may be null)
    pub original: Option<String>,
    /// Path to the enhanced/color-corrected scan (may be null)
    pub enhanced: Option<String>,
    /// Path to the back-of-photo scan (may be null)
    pub back: Option<String>,
    /// Combined metadata from all sources
    pub metadata: JsMetadata,
}

impl From<CorePhotoStack> for JsPhotoStack {
    fn from(s: CorePhotoStack) -> Self {
        Self {
            id: s.id,
            original: s.original.map(|p| p.to_string_lossy().to_string()),
            enhanced: s.enhanced.map(|p| p.to_string_lossy().to_string()),
            back: s.back.map(|p| p.to_string_lossy().to_string()),
            metadata: s.metadata.into(),
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
#[napi(object)]
#[derive(Clone)]
pub struct JsPaginatedResult {
    /// The photo stacks in this page.
    pub items: Vec<JsPhotoStack>,
    /// Total number of stacks across all pages.
    pub total_count: u32,
    /// The offset used for this page.
    pub offset: u32,
    /// The page size limit used for this page.
    pub limit: u32,
    /// Whether there are more items beyond this page.
    pub has_more: bool,
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
    inner: LocalRepository,
}

#[napi]
impl PhotostaxRepository {
    /// Create a new repository rooted at the given directory path.
    ///
    /// @param directoryPath - Path to the directory containing FastFoto photo files
    /// @param options - Optional configuration (e.g. `{ recursive: true }`)
    /// @throws Error if the path is invalid
    #[napi(constructor)]
    pub fn new(directory_path: String, options: Option<RepositoryOptions>) -> Self {
        let recursive = options.as_ref().and_then(|o| o.recursive).unwrap_or(false);
        let config = photostax_core::scanner::ScannerConfig {
            recursive,
            ..photostax_core::scanner::ScannerConfig::default()
        };
        Self {
            inner: LocalRepository::with_config(PathBuf::from(directory_path), config),
        }
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
    pub fn scan(&self) -> napi::Result<Vec<JsPhotoStack>> {
        self.inner
            .scan()
            .map(|stacks| stacks.into_iter().map(JsPhotoStack::from).collect())
            .map_err(|e| napi::Error::from_reason(e.to_string()))
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
    #[napi(ts_args_type = "profile?: string, callback?: (phase: string, current: number, total: number) => void")]
    pub fn scan_with_progress(
        &self,
        profile: Option<String>,
        callback: Option<JsFunction>,
    ) -> napi::Result<Vec<JsPhotoStack>> {
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

        self.inner
            .scan_with_progress(scanner_profile, progress)
            .map(|stacks| stacks.into_iter().map(JsPhotoStack::from).collect())
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Scan the repository and return all photo stacks with full metadata loaded.
    ///
    /// This is the slower path that reads EXIF, XMP, and sidecar data for every stack.
    /// Prefer `scan()` + `loadMetadata()` for lazy-loading in large repositories.
    ///
    /// @returns Array of photo stacks with complete metadata
    /// @throws Error if the directory cannot be accessed
    #[napi]
    pub fn scan_with_metadata(&self) -> napi::Result<Vec<JsPhotoStack>> {
        self.inner
            .scan_with_metadata()
            .map(|stacks| stacks.into_iter().map(JsPhotoStack::from).collect())
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Retrieve a single photo stack by its ID.
    ///
    /// @param id - The stack identifier (base filename without suffix)
    /// @returns The photo stack with the given ID
    /// @throws Error if the stack is not found or cannot be accessed
    #[napi]
    pub fn get_stack(&self, id: String) -> napi::Result<JsPhotoStack> {
        self.inner
            .get_stack(&id)
            .map(JsPhotoStack::from)
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Load full metadata (EXIF, XMP, sidecar) for a specific stack.
    ///
    /// Use this with `scan()` for lazy-loading: scan first to get lightweight
    /// stacks, then load metadata on demand for individual stacks.
    ///
    /// @param stackId - The stack identifier
    /// @returns The loaded metadata
    /// @throws Error if the stack is not found or metadata cannot be read
    #[napi]
    pub fn load_metadata(&self, stack_id: String) -> napi::Result<JsMetadata> {
        let mut stack = self
            .inner
            .get_stack(&stack_id)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        self.inner
            .load_metadata(&mut stack)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        Ok(stack.metadata.into())
    }

    /// Read the raw bytes of an image file.
    ///
    /// @param path - Path to the image file (from a PhotoStack)
    /// @returns Buffer containing the image bytes
    /// @throws Error if the file cannot be read
    #[napi]
    pub fn read_image(&self, path: String) -> napi::Result<Buffer> {
        self.inner
            .read_image(Path::new(&path))
            .map(|bytes| bytes.into())
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Write metadata tags to a photo stack.
    ///
    /// XMP tags are written to the image file (or sidecar for TIFF).
    /// Custom and EXIF tags are stored in the sidecar database.
    ///
    /// @param stackId - The ID of the stack to update
    /// @param metadata - The metadata to write
    /// @throws Error if the stack is not found or metadata cannot be written
    #[napi]
    pub fn write_metadata(&self, stack_id: String, metadata: JsMetadata) -> napi::Result<()> {
        let stack = self
            .inner
            .get_stack(&stack_id)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        let core_metadata: CoreMetadata = metadata.into();

        self.inner
            .write_metadata(&stack, &core_metadata)
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// Search for photo stacks matching the given query.
    ///
    /// @param query - Search criteria (all filters are AND'd together)
    /// @returns Array of matching photo stacks
    /// @throws Error if the repository cannot be scanned
    #[napi]
    pub fn search(&self, query: JsSearchQuery) -> napi::Result<Vec<JsPhotoStack>> {
        let stacks = self
            .inner
            .scan_with_metadata()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        let core_query: CoreSearchQuery = query.into();
        let results = filter_stacks(&stacks, &core_query);

        Ok(results.into_iter().map(JsPhotoStack::from).collect())
    }

    /// Scan the repository and return a paginated page of photo stacks.
    ///
    /// @param offset - Number of stacks to skip (0-based)
    /// @param limit - Maximum number of stacks to return per page
    /// @param loadMetadata - When true, loads EXIF/XMP/sidecar metadata for each stack in the page
    /// @returns Paginated result with items and metadata
    /// @throws Error if the directory cannot be accessed
    #[napi]
    pub fn scan_paginated(&self, offset: u32, limit: u32, load_metadata: Option<bool>) -> napi::Result<JsPaginatedResult> {
        let stacks = self
            .inner
            .scan()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        let paginated = paginate_stacks(
            &stacks,
            &PaginationParams {
                offset: offset as usize,
                limit: limit as usize,
            },
        );

        let items: Vec<JsPhotoStack> = if load_metadata.unwrap_or(false) {
            paginated
                .items
                .into_iter()
                .map(|s| {
                    let mut owned = s.clone();
                    let _ = self.inner.load_metadata(&mut owned);
                    JsPhotoStack::from(owned)
                })
                .collect()
        } else {
            paginated.items.into_iter().map(JsPhotoStack::from).collect()
        };

        Ok(JsPaginatedResult {
            items,
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
        let stacks = self
            .inner
            .scan_with_metadata()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        let core_query: CoreSearchQuery = query.into();
        let filtered = filter_stacks(&stacks, &core_query);

        let paginated = paginate_stacks(
            &filtered,
            &PaginationParams {
                offset: offset as usize,
                limit: limit as usize,
            },
        );

        Ok(JsPaginatedResult {
            items: paginated.items.into_iter().map(JsPhotoStack::from).collect(),
            total_count: paginated.total_count as u32,
            offset: paginated.offset as u32,
            limit: paginated.limit as u32,
            has_more: paginated.has_more,
        })
    }

    /// Rotate images in a photo stack by the given number of degrees.
    ///
    /// Image files are decoded, rotated at the pixel level, and re-encoded
    /// on disk. Returns the refreshed stack.
    ///
    /// @param stackId - The ID of the stack to rotate
    /// @param degrees - Rotation angle: 90, -90, 180, or -180
    /// @param target - Which images to rotate: "all" (default), "front", or "back"
    /// @returns The updated photo stack with refreshed metadata
    /// @throws Error if the stack is not found, degrees are invalid, or rotation fails
    #[napi]
    pub fn rotate_stack(
        &self,
        stack_id: String,
        degrees: i32,
        target: Option<String>,
    ) -> napi::Result<JsPhotoStack> {
        let rotation = CoreRotation::from_degrees(degrees).ok_or_else(|| {
            napi::Error::from_reason(format!(
                "Invalid rotation: {degrees}°. Accepted values: 90, -90, 180, -180"
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

        self.inner
            .rotate_stack(&stack_id, rotation, rotation_target)
            .map(JsPhotoStack::from)
            .map_err(|e| napi::Error::from_reason(e.to_string()))
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
        let snapshot = if load_metadata.unwrap_or(false) {
            CoreScanSnapshot::from_scan_with_metadata(&self.inner)
        } else {
            CoreScanSnapshot::from_scan(&self.inner)
        };

        snapshot
            .map(|s| JsScanSnapshot { inner: s })
            .map_err(|e| napi::Error::from_reason(e.to_string()))
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
    pub fn check_snapshot_status(&self, snapshot: &JsScanSnapshot) -> napi::Result<JsSnapshotStatus> {
        snapshot
            .inner
            .check_status(&self.inner)
            .map(|s| JsSnapshotStatus {
                is_stale: s.is_stale,
                snapshot_count: s.snapshot_count as u32,
                current_count: s.current_count as u32,
                added: s.added as u32,
                removed: s.removed as u32,
            })
            .map_err(|e| napi::Error::from_reason(e.to_string()))
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
            items: paginated.items.into_iter().map(JsPhotoStack::from).collect(),
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
