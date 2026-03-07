//! XMP sidecar file-based metadata storage.
//!
//! This module provides per-image XMP sidecar files (`.xmp`) for storing
//! extended metadata alongside photo files. Uses a portable, industry-standard
//! format compatible with Adobe Lightroom, darktable, and other photo
//! applications.
//!
//! ## Sidecar File Layout
//!
//! Each photo stack gets a single `.xmp` file named after the stack ID:
//!
//! ```text
//! /photos/
//! ├── IMG_001.jpg
//! ├── IMG_001_a.jpg
//! ├── IMG_001_b.jpg
//! ├── IMG_001.xmp           ← XMP sidecar for the stack
//! ├── IMG_002.tif
//! └── IMG_002.xmp
//! ```
//!
//! ## Data Stored in Sidecar
//!
//! The XMP sidecar stores three categories of metadata:
//!
//! | Category | Namespace | Example |
//! |----------|-----------|---------|
//! | Standard XMP | `dc:` | `dc:description`, `dc:creator` |
//! | Custom tags | `photostax:customTags` | JSON blob of key-value pairs |
//! | EXIF overrides | `photostax:exifOverrides` | JSON blob of key-value pairs |
//!
//! ## Examples
//!
//! ```rust,no_run
//! use photostax_core::metadata::sidecar;
//! use std::path::Path;
//!
//! // Read all metadata from a stack's sidecar
//! let data = sidecar::read_sidecar(Path::new("/photos"), "IMG_001")?;
//!
//! // Remove a custom tag
//! sidecar::remove_custom_tag(Path::new("/photos"), "IMG_001", "ocr_text")?;
//! # Ok::<(), photostax_core::metadata::sidecar::SidecarError>(())
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::xmp::{self, XmpError};

/// Reserved XMP key for serialized custom tags (JSON object).
pub const CUSTOM_TAGS_KEY: &str = "customTags";

/// Reserved XMP key for serialized EXIF overrides (JSON object).
pub const EXIF_OVERRIDES_KEY: &str = "exifOverrides";

/// XMP sidecar file extension.
pub const SIDECAR_EXT: &str = "xmp";

/// Structured metadata read from or written to an XMP sidecar file.
///
/// Mirrors the three categories in [`Metadata`] but represents
/// what is stored in a single `.xmp` sidecar:
///
/// - `xmp_tags` — Standard XMP/Dublin Core key-value pairs
/// - `custom_tags` — Application-specific tags (JSON values)
/// - `exif_overrides` — User-supplied EXIF overrides (string values)
///
/// [`Metadata`]: crate::photo_stack::Metadata
#[derive(Debug, Clone, Default)]
pub struct SidecarData {
    /// Standard XMP tags (dc:description, dc:creator, photostax:stackId, etc.).
    pub xmp_tags: HashMap<String, String>,

    /// Custom application tags stored as JSON values.
    pub custom_tags: HashMap<String, serde_json::Value>,

    /// EXIF override values provided by the user.
    pub exif_overrides: HashMap<String, String>,
}

/// Returns the sidecar file path for a given stack in a directory.
///
/// # Examples
///
/// ```
/// use photostax_core::metadata::sidecar::sidecar_path;
/// use std::path::Path;
///
/// let path = sidecar_path(Path::new("/photos"), "IMG_001");
/// assert!(path.ends_with("IMG_001.xmp"));
/// ```
pub fn sidecar_path(directory: &Path, stack_id: &str) -> PathBuf {
    directory.join(format!("{stack_id}.{SIDECAR_EXT}"))
}

/// Read metadata from a stack's XMP sidecar file.
///
/// Returns [`SidecarData::default()`] if no sidecar file exists.
/// Parses the `customTags` and `exifOverrides` reserved keys from the
/// photostax namespace and returns them separately from standard XMP tags.
///
/// # Arguments
///
/// * `directory` - Directory containing photo files and sidecar
/// * `stack_id`  - The [`PhotoStack`] ID (e.g., `"IMG_001"`)
///
/// # Errors
///
/// Returns [`SidecarError`] if the file exists but cannot be read or parsed.
///
/// [`PhotoStack`]: crate::photo_stack::PhotoStack
pub fn read_sidecar(directory: &Path, stack_id: &str) -> Result<SidecarData, SidecarError> {
    let path = sidecar_path(directory, stack_id);
    if !path.exists() {
        return Ok(SidecarData::default());
    }

    let xml = std::fs::read_to_string(&path)?;
    let mut flat = xmp::parse_xmp_xml(&xml).map_err(SidecarError::Xmp)?;

    // Extract reserved keys
    let custom_tags: HashMap<String, serde_json::Value> = flat
        .remove(CUSTOM_TAGS_KEY)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let exif_overrides: HashMap<String, String> = flat
        .remove(EXIF_OVERRIDES_KEY)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    Ok(SidecarData {
        xmp_tags: flat,
        custom_tags,
        exif_overrides,
    })
}

/// Write metadata to a stack's XMP sidecar file.
///
/// Serializes all three metadata categories into a single XMP file.
/// Custom tags and EXIF overrides are stored as JSON blobs in reserved
/// photostax namespace fields. Any existing sidecar is overwritten.
///
/// # Arguments
///
/// * `directory` - Directory containing photo files
/// * `stack_id`  - The [`PhotoStack`] ID
/// * `data`      - Structured metadata to write
///
/// [`PhotoStack`]: crate::photo_stack::PhotoStack
pub fn write_sidecar(
    directory: &Path,
    stack_id: &str,
    data: &SidecarData,
) -> Result<(), SidecarError> {
    let mut flat: HashMap<String, String> = HashMap::new();

    // Add standard XMP tags
    for (k, v) in &data.xmp_tags {
        flat.insert(k.clone(), v.clone());
    }

    // Serialize custom tags as a JSON blob
    if !data.custom_tags.is_empty() {
        let json = serde_json::to_string(&data.custom_tags)
            .map_err(|e| SidecarError::Serialization(e.to_string()))?;
        flat.insert(CUSTOM_TAGS_KEY.to_string(), json);
    }

    // Serialize EXIF overrides as a JSON blob
    if !data.exif_overrides.is_empty() {
        let json = serde_json::to_string(&data.exif_overrides)
            .map_err(|e| SidecarError::Serialization(e.to_string()))?;
        flat.insert(EXIF_OVERRIDES_KEY.to_string(), json);
    }

    let path = sidecar_path(directory, stack_id);
    let xml = xmp::build_xmp_xml(&flat);
    std::fs::write(path, xml)?;

    Ok(())
}

/// Read-modify-write: merge new metadata into an existing sidecar.
///
/// Reads the current sidecar (if any), merges in the provided metadata
/// (new values override existing ones), and writes the result back.
///
/// # Arguments
///
/// * `directory` - Directory containing photo files
/// * `stack_id`  - The [`PhotoStack`] ID
/// * `metadata`  - New metadata to merge (from [`Metadata`])
///
/// [`PhotoStack`]: crate::photo_stack::PhotoStack
/// [`Metadata`]: crate::photo_stack::Metadata
pub fn merge_and_write(
    directory: &Path,
    stack_id: &str,
    xmp_tags: &HashMap<String, String>,
    custom_tags: &HashMap<String, serde_json::Value>,
    exif_overrides: &HashMap<String, String>,
) -> Result<(), SidecarError> {
    let mut existing = read_sidecar(directory, stack_id)?;

    // Merge XMP tags (new overrides existing)
    for (k, v) in xmp_tags {
        existing.xmp_tags.insert(k.clone(), v.clone());
    }

    // Merge custom tags
    for (k, v) in custom_tags {
        existing.custom_tags.insert(k.clone(), v.clone());
    }

    // Merge EXIF overrides
    for (k, v) in exif_overrides {
        existing.exif_overrides.insert(k.clone(), v.clone());
    }

    write_sidecar(directory, stack_id, &existing)
}

/// Remove a single custom tag from a sidecar file.
///
/// Reads the sidecar, removes the key from custom tags, and writes back.
/// Returns `true` if the key existed and was removed.
///
/// # Arguments
///
/// * `directory` - Directory containing photo files
/// * `stack_id`  - The [`PhotoStack`] ID
/// * `key`       - Custom tag key to remove
///
/// [`PhotoStack`]: crate::photo_stack::PhotoStack
pub fn remove_custom_tag(
    directory: &Path,
    stack_id: &str,
    key: &str,
) -> Result<bool, SidecarError> {
    let mut data = read_sidecar(directory, stack_id)?;
    let existed = data.custom_tags.remove(key).is_some();
    if existed {
        write_sidecar(directory, stack_id, &data)?;
    }
    Ok(existed)
}

/// Remove all custom tags for a photo stack.
///
/// Preserves XMP tags and EXIF overrides; only clears custom tags.
///
/// # Returns
///
/// The number of custom tags that were removed.
pub fn remove_all_custom_tags(directory: &Path, stack_id: &str) -> Result<usize, SidecarError> {
    let mut data = read_sidecar(directory, stack_id)?;
    let count = data.custom_tags.len();
    if count > 0 {
        data.custom_tags.clear();
        write_sidecar(directory, stack_id, &data)?;
    }
    Ok(count)
}

/// List all stack IDs that have XMP sidecar files in a directory.
///
/// Scans the directory for `*.xmp` files and extracts stack IDs.
pub fn list_sidecar_stacks(directory: &Path) -> Result<Vec<String>, SidecarError> {
    let mut ids = Vec::new();
    let entries = std::fs::read_dir(directory)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some(SIDECAR_EXT) {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
    }
    Ok(ids)
}

/// Errors from sidecar file operations.
///
/// # Variants
///
/// | Variant | When It Occurs |
/// |---------|----------------|
/// | [`Io`](Self::Io) | File cannot be read or written |
/// | [`Xmp`](Self::Xmp) | XMP XML is malformed |
/// | [`Serialization`](Self::Serialization) | JSON serialization/deserialization failed |
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    /// An I/O error occurred reading or writing the sidecar file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The XMP content could not be parsed.
    #[error("XMP error: {0}")]
    Xmp(XmpError),

    /// JSON serialization failed.
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sidecar_path() {
        let path = sidecar_path(Path::new("/photos"), "IMG_001");
        assert_eq!(path, PathBuf::from("/photos/IMG_001.xmp"));
    }

    #[test]
    fn test_read_sidecar_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let data = read_sidecar(tmp.path(), "NONEXISTENT").unwrap();
        assert!(data.xmp_tags.is_empty());
        assert!(data.custom_tags.is_empty());
        assert!(data.exif_overrides.is_empty());
    }

    #[test]
    fn test_write_and_read_xmp_tags() {
        let tmp = TempDir::new().unwrap();
        let data = SidecarData {
            xmp_tags: {
                let mut m = HashMap::new();
                m.insert("description".to_string(), "Family photo".to_string());
                m.insert("creator".to_string(), "John".to_string());
                m
            },
            ..Default::default()
        };

        write_sidecar(tmp.path(), "IMG_001", &data).unwrap();
        let read = read_sidecar(tmp.path(), "IMG_001").unwrap();

        assert_eq!(
            read.xmp_tags.get("description"),
            Some(&"Family photo".to_string())
        );
        assert_eq!(read.xmp_tags.get("creator"), Some(&"John".to_string()));
        assert!(read.custom_tags.is_empty());
        assert!(read.exif_overrides.is_empty());
    }

    #[test]
    fn test_write_and_read_custom_tags() {
        let tmp = TempDir::new().unwrap();
        let data = SidecarData {
            custom_tags: {
                let mut m = HashMap::new();
                m.insert("ocr_text".to_string(), serde_json::json!("Happy Birthday!"));
                m.insert("processed".to_string(), serde_json::json!(true));
                m.insert("people".to_string(), serde_json::json!(["John", "Jane"]));
                m
            },
            ..Default::default()
        };

        write_sidecar(tmp.path(), "IMG_001", &data).unwrap();
        let read = read_sidecar(tmp.path(), "IMG_001").unwrap();

        assert_eq!(
            read.custom_tags.get("ocr_text"),
            Some(&serde_json::json!("Happy Birthday!"))
        );
        assert_eq!(
            read.custom_tags.get("processed"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            read.custom_tags.get("people"),
            Some(&serde_json::json!(["John", "Jane"]))
        );
    }

    #[test]
    fn test_write_and_read_exif_overrides() {
        let tmp = TempDir::new().unwrap();
        let data = SidecarData {
            exif_overrides: {
                let mut m = HashMap::new();
                m.insert("Make".to_string(), "EPSON".to_string());
                m.insert("Model".to_string(), "FastFoto FF-680W".to_string());
                m
            },
            ..Default::default()
        };

        write_sidecar(tmp.path(), "IMG_001", &data).unwrap();
        let read = read_sidecar(tmp.path(), "IMG_001").unwrap();

        assert_eq!(read.exif_overrides.get("Make"), Some(&"EPSON".to_string()));
        assert_eq!(
            read.exif_overrides.get("Model"),
            Some(&"FastFoto FF-680W".to_string())
        );
    }

    #[test]
    fn test_write_and_read_all_categories() {
        let tmp = TempDir::new().unwrap();
        let data = SidecarData {
            xmp_tags: {
                let mut m = HashMap::new();
                m.insert("description".to_string(), "Test".to_string());
                m
            },
            custom_tags: {
                let mut m = HashMap::new();
                m.insert("album".to_string(), serde_json::json!("Vacation"));
                m
            },
            exif_overrides: {
                let mut m = HashMap::new();
                m.insert("Make".to_string(), "EPSON".to_string());
                m
            },
        };

        write_sidecar(tmp.path(), "IMG_001", &data).unwrap();
        let read = read_sidecar(tmp.path(), "IMG_001").unwrap();

        assert_eq!(read.xmp_tags.get("description"), Some(&"Test".to_string()));
        assert_eq!(
            read.custom_tags.get("album"),
            Some(&serde_json::json!("Vacation"))
        );
        assert_eq!(read.exif_overrides.get("Make"), Some(&"EPSON".to_string()));
    }

    #[test]
    fn test_merge_and_write() {
        let tmp = TempDir::new().unwrap();

        // Write initial data
        let initial = SidecarData {
            xmp_tags: {
                let mut m = HashMap::new();
                m.insert("description".to_string(), "Original".to_string());
                m.insert("creator".to_string(), "Alice".to_string());
                m
            },
            custom_tags: {
                let mut m = HashMap::new();
                m.insert("tag1".to_string(), serde_json::json!("val1"));
                m
            },
            ..Default::default()
        };
        write_sidecar(tmp.path(), "IMG_001", &initial).unwrap();

        // Merge new data
        let mut new_xmp = HashMap::new();
        new_xmp.insert("description".to_string(), "Updated".to_string());
        let mut new_custom = HashMap::new();
        new_custom.insert("tag2".to_string(), serde_json::json!("val2"));

        merge_and_write(
            tmp.path(),
            "IMG_001",
            &new_xmp,
            &new_custom,
            &HashMap::new(),
        )
        .unwrap();

        let read = read_sidecar(tmp.path(), "IMG_001").unwrap();
        assert_eq!(
            read.xmp_tags.get("description"),
            Some(&"Updated".to_string())
        );
        assert_eq!(read.xmp_tags.get("creator"), Some(&"Alice".to_string()));
        assert_eq!(
            read.custom_tags.get("tag1"),
            Some(&serde_json::json!("val1"))
        );
        assert_eq!(
            read.custom_tags.get("tag2"),
            Some(&serde_json::json!("val2"))
        );
    }

    #[test]
    fn test_remove_custom_tag() {
        let tmp = TempDir::new().unwrap();

        let data = SidecarData {
            custom_tags: {
                let mut m = HashMap::new();
                m.insert("tag1".to_string(), serde_json::json!("val1"));
                m.insert("tag2".to_string(), serde_json::json!("val2"));
                m
            },
            ..Default::default()
        };
        write_sidecar(tmp.path(), "IMG_001", &data).unwrap();

        assert!(remove_custom_tag(tmp.path(), "IMG_001", "tag1").unwrap());
        assert!(!remove_custom_tag(tmp.path(), "IMG_001", "nonexistent").unwrap());

        let read = read_sidecar(tmp.path(), "IMG_001").unwrap();
        assert_eq!(read.custom_tags.len(), 1);
        assert!(read.custom_tags.contains_key("tag2"));
    }

    #[test]
    fn test_remove_all_custom_tags() {
        let tmp = TempDir::new().unwrap();

        let data = SidecarData {
            xmp_tags: {
                let mut m = HashMap::new();
                m.insert("description".to_string(), "Keep me".to_string());
                m
            },
            custom_tags: {
                let mut m = HashMap::new();
                m.insert("a".to_string(), serde_json::json!(1));
                m.insert("b".to_string(), serde_json::json!(2));
                m.insert("c".to_string(), serde_json::json!(3));
                m
            },
            ..Default::default()
        };
        write_sidecar(tmp.path(), "IMG_001", &data).unwrap();

        let count = remove_all_custom_tags(tmp.path(), "IMG_001").unwrap();
        assert_eq!(count, 3);

        let read = read_sidecar(tmp.path(), "IMG_001").unwrap();
        assert!(read.custom_tags.is_empty());
        // XMP tags preserved
        assert_eq!(
            read.xmp_tags.get("description"),
            Some(&"Keep me".to_string())
        );
    }

    #[test]
    fn test_remove_all_custom_tags_empty() {
        let tmp = TempDir::new().unwrap();
        let count = remove_all_custom_tags(tmp.path(), "NONEXISTENT").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_remove_custom_tag_no_sidecar() {
        let tmp = TempDir::new().unwrap();
        let result = remove_custom_tag(tmp.path(), "NONEXISTENT", "key").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_list_sidecar_stacks() {
        let tmp = TempDir::new().unwrap();

        // Create some sidecar files
        let data = SidecarData::default();
        write_sidecar(tmp.path(), "IMG_001", &data).unwrap();
        write_sidecar(tmp.path(), "IMG_002", &data).unwrap();

        // Create a non-sidecar file
        std::fs::write(tmp.path().join("IMG_003.jpg"), b"jpeg").unwrap();

        let mut ids = list_sidecar_stacks(tmp.path()).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["IMG_001", "IMG_002"]);
    }

    #[test]
    fn test_sidecar_file_is_valid_xmp() {
        let tmp = TempDir::new().unwrap();
        let data = SidecarData {
            xmp_tags: {
                let mut m = HashMap::new();
                m.insert("description".to_string(), "Test photo".to_string());
                m
            },
            custom_tags: {
                let mut m = HashMap::new();
                m.insert("album".to_string(), serde_json::json!("Family"));
                m
            },
            ..Default::default()
        };

        write_sidecar(tmp.path(), "IMG_001", &data).unwrap();

        let content = std::fs::read_to_string(tmp.path().join("IMG_001.xmp")).unwrap();
        assert!(content.contains("<?xpacket begin="));
        assert!(content.contains("<dc:description>Test photo</dc:description>"));
        assert!(content.contains("photostax:customTags"));
        assert!(content.contains("<?xpacket end="));
    }

    #[test]
    fn test_sidecar_error_display() {
        let err = SidecarError::Serialization("test error".to_string());
        let display = format!("{}", err);
        assert!(display.contains("Serialization error"));
        assert!(display.contains("test error"));

        let io_err = SidecarError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found",
        ));
        assert!(format!("{}", io_err).contains("I/O error"));

        let xmp_err = SidecarError::Xmp(XmpError::XmpParse("bad xml".to_string()));
        assert!(format!("{}", xmp_err).contains("XMP error"));
    }

    #[test]
    fn test_sidecar_error_debug() {
        let err = SidecarError::Serialization("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Serialization"));
    }

    #[test]
    fn test_write_sidecar_creates_file() {
        let tmp = TempDir::new().unwrap();
        let path = sidecar_path(tmp.path(), "IMG_001");
        assert!(!path.exists());

        write_sidecar(tmp.path(), "IMG_001", &SidecarData::default()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_upsert_custom_tag() {
        let tmp = TempDir::new().unwrap();

        let mut custom = HashMap::new();
        custom.insert("status".to_string(), serde_json::json!("pending"));
        merge_and_write(
            tmp.path(),
            "IMG_001",
            &HashMap::new(),
            &custom,
            &HashMap::new(),
        )
        .unwrap();

        let mut custom2 = HashMap::new();
        custom2.insert("status".to_string(), serde_json::json!("done"));
        merge_and_write(
            tmp.path(),
            "IMG_001",
            &HashMap::new(),
            &custom2,
            &HashMap::new(),
        )
        .unwrap();

        let read = read_sidecar(tmp.path(), "IMG_001").unwrap();
        assert_eq!(
            read.custom_tags.get("status"),
            Some(&serde_json::json!("done"))
        );
    }

    #[test]
    fn test_custom_tags_preserve_json_types() {
        let tmp = TempDir::new().unwrap();
        let data = SidecarData {
            custom_tags: {
                let mut m = HashMap::new();
                m.insert("string_val".to_string(), serde_json::json!("hello"));
                m.insert("int_val".to_string(), serde_json::json!(42));
                m.insert("bool_val".to_string(), serde_json::json!(true));
                m.insert("array_val".to_string(), serde_json::json!(["a", "b"]));
                m.insert("null_val".to_string(), serde_json::json!(null));
                m
            },
            ..Default::default()
        };

        write_sidecar(tmp.path(), "IMG_001", &data).unwrap();
        let read = read_sidecar(tmp.path(), "IMG_001").unwrap();

        assert_eq!(
            read.custom_tags.get("string_val"),
            Some(&serde_json::json!("hello"))
        );
        assert_eq!(
            read.custom_tags.get("int_val"),
            Some(&serde_json::json!(42))
        );
        assert_eq!(
            read.custom_tags.get("bool_val"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            read.custom_tags.get("array_val"),
            Some(&serde_json::json!(["a", "b"]))
        );
        assert_eq!(
            read.custom_tags.get("null_val"),
            Some(&serde_json::json!(null))
        );
    }
}
