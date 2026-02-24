//! Directory scanning and photo stack grouping.
//!
//! This module provides functionality to scan directories containing Epson FastFoto
//! scans and group related files into [`PhotoStack`] objects based on the FastFoto
//! naming convention.
//!
//! ## FastFoto Naming Convention
//!
//! Epson FastFoto scanners produce files with a consistent naming pattern:
//!
//! ```text
//! IMG_0001.jpg      # Original front scan (no suffix)
//! IMG_0001_a.jpg    # Enhanced/color-corrected front (suffix: _a)
//! IMG_0001_b.jpg    # Back of photo (suffix: _b)
//! ```
//!
//! The scanner detects these suffixes and groups files with the same base name
//! into a single [`PhotoStack`] with ID `IMG_0001`.
//!
//! ## Performance
//!
//! - Single-pass directory scan using [`std::fs::read_dir`]
//! - O(n) time complexity where n is the number of files
//! - Memory usage proportional to the number of unique stacks
//!
//! ## Examples
//!
//! ```rust,no_run
//! use photostax_core::scanner::{scan_directory, ScannerConfig};
//! use std::path::Path;
//!
//! let config = ScannerConfig::default();
//! let stacks = scan_directory(Path::new("/photos"), &config)?;
//!
//! for stack in stacks {
//!     println!("{}: {} files", stack.id, [
//!         stack.original.as_ref(),
//!         stack.enhanced.as_ref(),
//!         stack.back.as_ref(),
//!     ].iter().filter(|x| x.is_some()).count());
//! }
//! # Ok::<(), std::io::Error>(())
//! ```

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;

use crate::photo_stack::PhotoStack;

/// Configuration for the FastFoto file scanner.
///
/// Controls how files are identified and grouped into photo stacks. The default
/// configuration matches the standard Epson FastFoto naming convention.
///
/// # Fields
///
/// | Field | Default | Description |
/// |-------|---------|-------------|
/// | `enhanced_suffix` | `_a` | Suffix for enhanced/color-corrected images |
/// | `back_suffix` | `_b` | Suffix for back-of-photo scans |
/// | `extensions` | `jpg`, `jpeg`, `tif`, `tiff` | File extensions to scan |
///
/// # Examples
///
/// Using custom suffixes for a different scanner:
///
/// ```
/// use photostax_core::scanner::ScannerConfig;
///
/// let config = ScannerConfig {
///     enhanced_suffix: "_enhanced".to_string(),
///     back_suffix: "_back".to_string(),
///     extensions: vec!["jpg".to_string()],
/// };
/// ```
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// Suffix appended to the base name for enhanced/color-corrected images.
    ///
    /// Default: `_a` (FastFoto convention for auto-enhanced images)
    pub enhanced_suffix: String,

    /// Suffix appended to the base name for back-of-photo images.
    ///
    /// Default: `_b` (FastFoto convention for back scans)
    pub back_suffix: String,

    /// File extensions to consider as valid image files.
    ///
    /// Default: `["jpg", "jpeg", "tif", "tiff"]` (both JPEG and TIFF formats)
    pub extensions: Vec<String>,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            enhanced_suffix: "_a".to_string(),
            back_suffix: "_b".to_string(),
            extensions: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "tif".to_string(),
                "tiff".to_string(),
            ],
        }
    }
}

/// Scans a directory and groups Epson FastFoto files into [`PhotoStack`] objects.
///
/// Files are grouped by their base name (without suffix or extension). The function
/// recognizes the `_a` (enhanced) and `_b` (back) suffixes from the FastFoto naming
/// convention.
///
/// # Arguments
///
/// * `dir` - The directory path to scan
/// * `config` - Scanner configuration specifying suffixes and extensions
///
/// # Returns
///
/// A vector of [`PhotoStack`] objects sorted alphabetically by ID.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the directory cannot be read.
///
/// # Performance
///
/// This function performs a single-pass scan of the directory. Files are processed
/// in the order returned by the filesystem, then sorted at the end.
///
/// # Examples
///
/// Basic usage with default configuration:
///
/// ```rust,no_run
/// use photostax_core::scanner::{scan_directory, ScannerConfig};
/// use std::path::Path;
///
/// let stacks = scan_directory(Path::new("/photos"), &ScannerConfig::default())?;
/// println!("Found {} photo stacks", stacks.len());
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// Scanning with custom configuration:
///
/// ```rust,no_run
/// use photostax_core::scanner::{scan_directory, ScannerConfig};
/// use std::path::Path;
///
/// let config = ScannerConfig {
///     extensions: vec!["tif".to_string()], // TIFF only
///     ..ScannerConfig::default()
/// };
/// let stacks = scan_directory(Path::new("/archive"), &config)?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn scan_directory(dir: &Path, config: &ScannerConfig) -> std::io::Result<Vec<PhotoStack>> {
    let mut stacks: HashMap<String, PhotoStack> = HashMap::new();

    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(OsStr::to_str)
            .map(|s| s.to_lowercase());

        let is_valid_ext = ext
            .as_ref()
            .map(|e| config.extensions.contains(e))
            .unwrap_or(false);

        if !is_valid_ext {
            continue;
        }

        let stem = match path.file_stem().and_then(OsStr::to_str) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let (base_name, variant) = classify_stem(&stem, config);

        let stack = stacks
            .entry(base_name.clone())
            .or_insert_with(|| PhotoStack::new(&base_name));

        match variant {
            Variant::Original => stack.original = Some(path),
            Variant::Enhanced => stack.enhanced = Some(path),
            Variant::Back => stack.back = Some(path),
        }
    }

    let mut result: Vec<PhotoStack> = stacks.into_values().collect();
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

/// Internal classification of file variants.
#[derive(Debug)]
enum Variant {
    /// Original scan (no suffix)
    Original,
    /// Enhanced/color-corrected (`_a` suffix)
    Enhanced,
    /// Back of photo (`_b` suffix)
    Back,
}

/// Classify a file stem into its base name and variant type.
fn classify_stem(stem: &str, config: &ScannerConfig) -> (String, Variant) {
    if let Some(base) = stem.strip_suffix(&config.enhanced_suffix) {
        (base.to_string(), Variant::Enhanced)
    } else if let Some(base) = stem.strip_suffix(&config.back_suffix) {
        (base.to_string(), Variant::Back)
    } else {
        (stem.to_string(), Variant::Original)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_scan_groups_photo_stack() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create test files
        fs::write(dir.join("IMG_001.jpg"), b"original").unwrap();
        fs::write(dir.join("IMG_001_a.jpg"), b"enhanced").unwrap();
        fs::write(dir.join("IMG_001_b.jpg"), b"back").unwrap();
        fs::write(dir.join("IMG_002.jpg"), b"original2").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 2);

        let s1 = stacks.iter().find(|s| s.id == "IMG_001").unwrap();
        assert!(s1.original.is_some());
        assert!(s1.enhanced.is_some());
        assert!(s1.back.is_some());

        let s2 = stacks.iter().find(|s| s.id == "IMG_002").unwrap();
        assert!(s2.original.is_some());
        assert!(s2.enhanced.is_none());
        assert!(s2.back.is_none());
    }

    #[test]
    fn test_classify_stem() {
        let config = ScannerConfig::default();

        let (base, variant) = classify_stem("IMG_001", &config);
        assert_eq!(base, "IMG_001");
        assert!(matches!(variant, Variant::Original));

        let (base, variant) = classify_stem("IMG_001_a", &config);
        assert_eq!(base, "IMG_001");
        assert!(matches!(variant, Variant::Enhanced));

        let (base, variant) = classify_stem("IMG_001_b", &config);
        assert_eq!(base, "IMG_001");
        assert!(matches!(variant, Variant::Back));
    }

    #[test]
    fn test_scan_tiff_only_directory() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create TIFF test files
        fs::write(dir.join("IMG_001.tif"), b"original").unwrap();
        fs::write(dir.join("IMG_001_a.tif"), b"enhanced").unwrap();
        fs::write(dir.join("IMG_001_b.tif"), b"back").unwrap();
        fs::write(dir.join("IMG_002.tiff"), b"original2").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 2);

        let s1 = stacks.iter().find(|s| s.id == "IMG_001").unwrap();
        assert!(s1.original.is_some());
        assert!(s1.enhanced.is_some());
        assert!(s1.back.is_some());

        let s2 = stacks.iter().find(|s| s.id == "IMG_002").unwrap();
        assert!(s2.original.is_some());
    }

    #[test]
    fn test_scan_mixed_jpg_and_tiff_directory() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create mixed JPEG and TIFF files (different stacks)
        fs::write(dir.join("IMG_001.jpg"), b"original").unwrap();
        fs::write(dir.join("IMG_001_a.jpg"), b"enhanced").unwrap();
        fs::write(dir.join("IMG_002.tif"), b"original2").unwrap();
        fs::write(dir.join("IMG_002_a.tif"), b"enhanced2").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 2);

        let s1 = stacks.iter().find(|s| s.id == "IMG_001").unwrap();
        assert!(s1.original.is_some());
        assert!(s1.enhanced.is_some());

        let s2 = stacks.iter().find(|s| s.id == "IMG_002").unwrap();
        assert!(s2.original.is_some());
        assert!(s2.enhanced.is_some());
    }

    #[test]
    fn test_scan_stack_with_mixed_extensions() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create a stack where original is jpg and back is tif
        fs::write(dir.join("IMG_001.jpg"), b"original").unwrap();
        fs::write(dir.join("IMG_001_b.tif"), b"back").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 1);

        let s1 = stacks.iter().find(|s| s.id == "IMG_001").unwrap();
        assert!(s1.original.is_some());
        assert!(s1.back.is_some());
        // Verify they have different extensions
        let orig_ext = s1.original.as_ref().unwrap().extension().unwrap();
        let back_ext = s1.back.as_ref().unwrap().extension().unwrap();
        assert_eq!(orig_ext, "jpg");
        assert_eq!(back_ext, "tif");
    }
}
