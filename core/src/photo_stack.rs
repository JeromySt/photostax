use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A unified representation of a single scanned photo from an Epson FastFoto scanner.
///
/// Groups the original scan, enhanced version, and back-of-photo image into
/// a single logical unit with associated metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoStack {
    /// Unique identifier derived from the base filename (without suffix/extension).
    pub id: String,
    /// Path to the original front scan (`<name>.jpg`).
    pub original: Option<PathBuf>,
    /// Path to the enhanced front scan (`<name>_a.jpg`).
    pub enhanced: Option<PathBuf>,
    /// Path to the back-of-photo scan (`<name>_b.jpg`).
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
}
