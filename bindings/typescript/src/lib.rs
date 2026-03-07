//! Node.js native addon for photostax using napi-rs.
//!
//! This crate provides JavaScript/TypeScript bindings for the photostax-core library,
//! enabling Node.js applications to work with Epson FastFoto photo stacks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use napi::bindgen_prelude::*;
use napi_derive::napi;

use photostax_core::backends::local::LocalRepository;
use photostax_core::photo_stack::{Metadata as CoreMetadata, PhotoStack as CorePhotoStack};
use photostax_core::repository::Repository;
use photostax_core::search::{filter_stacks, SearchQuery as CoreSearchQuery};

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

        query
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

    /// Scan the repository and return all discovered photo stacks.
    ///
    /// Groups files by FastFoto naming convention and enriches each stack
    /// with EXIF, XMP, and sidecar metadata.
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
            .scan()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        let core_query: CoreSearchQuery = query.into();
        let results = filter_stacks(&stacks, &core_query);

        Ok(results.into_iter().map(JsPhotoStack::from).collect())
    }
}
