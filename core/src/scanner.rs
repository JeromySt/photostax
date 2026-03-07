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
///     ..ScannerConfig::default()
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

    /// Whether to recurse into subdirectories.
    ///
    /// When `true`, all nested subdirectories are scanned for photo stacks.
    /// Stack IDs remain unique per file stem — if the same stem exists in
    /// multiple subdirectories, the last one found wins.
    ///
    /// Default: `false`
    pub recursive: bool,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            enhanced_suffix: "_a".to_string(),
            back_suffix: "_b".to_string(),
            recursive: false,
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
    scan_directory_inner(dir, config, &mut stacks)?;

    let mut result: Vec<PhotoStack> = stacks.into_values().collect();
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

/// Inner recursive helper for [`scan_directory`].
fn scan_directory_inner(
    dir: &Path,
    config: &ScannerConfig,
    stacks: &mut HashMap<String, PhotoStack>,
) -> std::io::Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if config.recursive {
                scan_directory_inner(&path, config, stacks)?;
            }
            continue;
        }

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
    Ok(())
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

// ── Folder metadata parsing ──────────────────────────────────────────────────

/// Metadata derived from an Epson FastFoto folder name.
///
/// FastFoto organises scans into folders named `<year>_<month_or_season>_<subject>`
/// where each component is optional but the order is always preserved when more
/// than one is present.
///
/// This is a **pure parse** with no I/O — any backend (local filesystem, cloud
/// storage, etc.) can extract a folder name from a path and call
/// [`parse_folder_name`] to derive metadata.
///
/// # Examples
///
/// ```
/// use photostax_core::scanner::parse_folder_name;
///
/// let meta = parse_folder_name("1984_Mexico");
/// assert_eq!(meta.year, Some(1984));
/// assert_eq!(meta.month_or_season, None);
/// assert_eq!(meta.subject.as_deref(), Some("Mexico"));
///
/// let meta = parse_folder_name("2024_Spring_FamilyReunion");
/// assert_eq!(meta.year, Some(2024));
/// assert_eq!(meta.month_or_season.as_deref(), Some("Spring"));
/// assert_eq!(meta.subject.as_deref(), Some("FamilyReunion"));
///
/// let meta = parse_folder_name("SteveJones");
/// assert_eq!(meta.year, None);
/// assert_eq!(meta.subject.as_deref(), Some("SteveJones"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderMeta {
    /// Four-digit year from the folder name (e.g. `1984`, `2024`).
    pub year: Option<u16>,

    /// Month name or season from the FastFoto preset list.
    ///
    /// Recognised values (case-insensitive):
    /// - Seasons: `Spring`, `Summer`, `Fall`, `Winter`
    /// - Months: `January` through `December`
    pub month_or_season: Option<String>,

    /// Free-form subject string — remaining tokens joined by `_`.
    pub subject: Option<String>,
}

impl FolderMeta {
    /// Returns `true` if no metadata could be derived from the folder name.
    ///
    /// ```
    /// use photostax_core::scanner::parse_folder_name;
    /// assert!(parse_folder_name("").is_empty());
    /// assert!(!parse_folder_name("1984").is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.year.is_none() && self.month_or_season.is_none() && self.subject.is_none()
    }
}

/// Recognised months and seasons from the Epson FastFoto UI.
const MONTHS_AND_SEASONS: &[&str] = &[
    "spring",
    "summer",
    "fall",
    "winter",
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Returns `true` if `token` is a recognised month or season (case-insensitive).
fn is_month_or_season(token: &str) -> bool {
    let lower = token.to_lowercase();
    MONTHS_AND_SEASONS.contains(&lower.as_str())
}

/// Returns `true` if `token` looks like a plausible four-digit year.
fn is_year(token: &str) -> bool {
    if token.len() != 4 {
        return false;
    }
    token
        .parse::<u16>()
        .is_ok_and(|y| (1800..=2200).contains(&y))
}

/// Parse an Epson FastFoto folder name into structured metadata.
///
/// The folder naming convention is `<year>_<month_or_season>_<subject>`, where
/// each component is optional but the order is preserved. The parser works
/// left-to-right:
///
/// 1. If the first underscore-separated token is a 4-digit year → `year`
/// 2. If the next token is a recognised month or season → `month_or_season`
/// 3. All remaining tokens (joined with `_`) → `subject`
///
/// If the first token is **not** a year, the entire string becomes the `subject`
/// (the folder doesn't follow FastFoto convention for year, so we don't try to
/// pick out month/season either).
///
/// # Arguments
///
/// * `name` — The folder/directory name (not a full path). For example `"1984_Mexico"`.
///
/// # Examples
///
/// ```
/// use photostax_core::scanner::parse_folder_name;
///
/// // Year only
/// let m = parse_folder_name("1983");
/// assert_eq!(m.year, Some(1983));
/// assert!(m.month_or_season.is_none());
/// assert!(m.subject.is_none());
///
/// // Year + season + subject
/// let m = parse_folder_name("2024_Summer_Beach");
/// assert_eq!(m.year, Some(2024));
/// assert_eq!(m.month_or_season.as_deref(), Some("Summer"));
/// assert_eq!(m.subject.as_deref(), Some("Beach"));
///
/// // Subject only (no year prefix)
/// let m = parse_folder_name("SteveJones");
/// assert!(m.year.is_none());
/// assert_eq!(m.subject.as_deref(), Some("SteveJones"));
/// ```
pub fn parse_folder_name(name: &str) -> FolderMeta {
    let name = name.trim();
    if name.is_empty() {
        return FolderMeta::default();
    }

    let tokens: Vec<&str> = name.splitn(usize::MAX, '_').collect();
    let mut idx = 0;
    let mut meta = FolderMeta::default();

    // 1. Try year
    if idx < tokens.len() && is_year(tokens[idx]) {
        meta.year = Some(tokens[idx].parse().unwrap());
        idx += 1;
    } else {
        // First token isn't a year — entire string treated as subject
        meta.subject = Some(name.to_string());
        return meta;
    }

    // 2. Try month / season
    if idx < tokens.len() && is_month_or_season(tokens[idx]) {
        // Preserve original casing from the folder name
        meta.month_or_season = Some(tokens[idx].to_string());
        idx += 1;
    }

    // 3. Remaining tokens form the subject
    if idx < tokens.len() {
        meta.subject = Some(tokens[idx..].join("_"));
    }

    meta
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

    #[test]
    fn test_scan_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let config = ScannerConfig::default();
        let stacks = scan_directory(tmp.path(), &config).unwrap();
        assert!(stacks.is_empty());
    }

    #[test]
    fn test_scan_directory_with_no_valid_images() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create non-image files
        fs::write(dir.join("readme.txt"), b"text").unwrap();
        fs::write(dir.join("image.png"), b"png").unwrap();
        fs::write(dir.join("data.bmp"), b"bmp").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();
        assert!(stacks.is_empty());
    }

    #[test]
    fn test_scan_unusual_casing() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create files with unusual casing
        fs::write(dir.join("IMG_001.JPG"), b"original").unwrap();
        fs::write(dir.join("IMG_001_a.Jpg"), b"enhanced").unwrap();
        fs::write(dir.join("IMG_002.TIF"), b"original2").unwrap();
        fs::write(dir.join("IMG_002_a.Tif"), b"enhanced2").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 2);
        assert!(stacks.iter().any(|s| s.id == "IMG_001"));
        assert!(stacks.iter().any(|s| s.id == "IMG_002"));
    }

    #[test]
    fn test_scan_enhanced_only_no_original() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create only enhanced file, no original
        fs::write(dir.join("IMG_001_a.jpg"), b"enhanced").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 1);
        let s = &stacks[0];
        assert_eq!(s.id, "IMG_001");
        assert!(s.original.is_none());
        assert!(s.enhanced.is_some());
        assert!(s.back.is_none());
    }

    #[test]
    fn test_scan_back_only_no_original() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create only back file, no original
        fs::write(dir.join("IMG_001_b.jpg"), b"back").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 1);
        let s = &stacks[0];
        assert_eq!(s.id, "IMG_001");
        assert!(s.original.is_none());
        assert!(s.enhanced.is_none());
        assert!(s.back.is_some());
    }

    #[test]
    fn test_scan_custom_config_suffixes() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create files with custom suffixes
        fs::write(dir.join("IMG_001.jpg"), b"original").unwrap();
        fs::write(dir.join("IMG_001_enhanced.jpg"), b"enhanced").unwrap();
        fs::write(dir.join("IMG_001_back.jpg"), b"back").unwrap();

        let config = ScannerConfig {
            enhanced_suffix: "_enhanced".to_string(),
            back_suffix: "_back".to_string(),
            extensions: vec!["jpg".to_string()],
            ..ScannerConfig::default()
        };
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 1);
        let s = &stacks[0];
        assert!(s.original.is_some());
        assert!(s.enhanced.is_some());
        assert!(s.back.is_some());
    }

    #[test]
    fn test_scan_unicode_filenames() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create files with unicode filenames
        fs::write(dir.join("写真_001.jpg"), b"original").unwrap();
        fs::write(dir.join("写真_001_a.jpg"), b"enhanced").unwrap();
        fs::write(dir.join("фото_002.tif"), b"original2").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 2);
        assert!(stacks.iter().any(|s| s.id == "写真_001"));
        assert!(stacks.iter().any(|s| s.id == "фото_002"));
    }

    #[test]
    fn test_classify_stem_empty_string() {
        let config = ScannerConfig::default();
        let (base, variant) = classify_stem("", &config);
        assert_eq!(base, "");
        assert!(matches!(variant, Variant::Original));
    }

    #[test]
    fn test_classify_stem_double_suffix_a_b() {
        let config = ScannerConfig::default();
        // File ending with _a_b - should strip _b first (back), base is IMG_001_a
        let (base, variant) = classify_stem("IMG_001_a_b", &config);
        assert_eq!(base, "IMG_001_a");
        assert!(matches!(variant, Variant::Back));
    }

    #[test]
    fn test_classify_stem_double_suffix_b_a() {
        let config = ScannerConfig::default();
        // File ending with _b_a - should strip _a first (enhanced), base is IMG_001_b
        let (base, variant) = classify_stem("IMG_001_b_a", &config);
        assert_eq!(base, "IMG_001_b");
        assert!(matches!(variant, Variant::Enhanced));
    }

    #[test]
    fn test_scanner_config_default() {
        let config = ScannerConfig::default();
        assert_eq!(config.enhanced_suffix, "_a");
        assert_eq!(config.back_suffix, "_b");
        assert!(config.extensions.contains(&"jpg".to_string()));
        assert!(config.extensions.contains(&"jpeg".to_string()));
        assert!(config.extensions.contains(&"tif".to_string()));
        assert!(config.extensions.contains(&"tiff".to_string()));
    }

    #[test]
    fn test_scan_ignores_subdirectories() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create a file in root
        fs::write(dir.join("IMG_001.jpg"), b"original").unwrap();

        // Create a subdirectory with files
        let subdir = dir.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("IMG_002.jpg"), b"sub_original").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        // Should only find IMG_001, not the one in subdir
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].id, "IMG_001");
    }

    #[test]
    fn test_scan_results_sorted_by_id() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Create files in non-alphabetical order
        fs::write(dir.join("ZZZ_001.jpg"), b"z").unwrap();
        fs::write(dir.join("AAA_001.jpg"), b"a").unwrap();
        fs::write(dir.join("MMM_001.jpg"), b"m").unwrap();

        let config = ScannerConfig::default();
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 3);
        assert_eq!(stacks[0].id, "AAA_001");
        assert_eq!(stacks[1].id, "MMM_001");
        assert_eq!(stacks[2].id, "ZZZ_001");
    }

    #[test]
    fn test_scan_recursive_finds_subdirectory_files() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Root-level stack
        fs::write(dir.join("IMG_001.jpg"), b"original").unwrap();
        fs::write(dir.join("IMG_001_a.jpg"), b"enhanced").unwrap();

        // Subdirectory stack
        let subdir = dir.join("batch2");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("IMG_002.jpg"), b"sub_original").unwrap();
        fs::write(subdir.join("IMG_002_b.jpg"), b"sub_back").unwrap();

        // Nested subdirectory stack
        let nested = subdir.join("deep");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("IMG_003.jpg"), b"deep_original").unwrap();

        let config = ScannerConfig {
            recursive: true,
            ..ScannerConfig::default()
        };
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 3);
        assert_eq!(stacks[0].id, "IMG_001");
        assert_eq!(stacks[1].id, "IMG_002");
        assert_eq!(stacks[2].id, "IMG_003");

        // Verify the root stack has enhanced image
        assert!(stacks[0].original.is_some());
        assert!(stacks[0].enhanced.is_some());

        // Verify the subdirectory stack has back image
        assert!(stacks[1].original.is_some());
        assert!(stacks[1].back.is_some());

        // Verify the nested stack exists
        assert!(stacks[2].original.is_some());
    }

    #[test]
    fn test_scan_non_recursive_ignores_subdirectories() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        fs::write(dir.join("IMG_001.jpg"), b"original").unwrap();

        let subdir = dir.join("batch2");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("IMG_002.jpg"), b"sub_original").unwrap();

        let config = ScannerConfig::default(); // recursive: false by default
        let stacks = scan_directory(dir, &config).unwrap();

        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].id, "IMG_001");
    }

    #[test]
    fn test_scanner_config_default_not_recursive() {
        let config = ScannerConfig::default();
        assert!(!config.recursive);
    }

    // ── FolderMeta / parse_folder_name tests ───────────────────────────────

    #[test]
    fn test_parse_folder_year_only() {
        let m = parse_folder_name("1983");
        assert_eq!(m.year, Some(1983));
        assert!(m.month_or_season.is_none());
        assert!(m.subject.is_none());
    }

    #[test]
    fn test_parse_folder_year_and_subject() {
        let m = parse_folder_name("1984_Mexico");
        assert_eq!(m.year, Some(1984));
        assert!(m.month_or_season.is_none());
        assert_eq!(m.subject.as_deref(), Some("Mexico"));
    }

    #[test]
    fn test_parse_folder_year_and_multiword_subject() {
        let m = parse_folder_name("1984_Marylynn_and_Scott_Wedding");
        assert_eq!(m.year, Some(1984));
        assert!(m.month_or_season.is_none());
        assert_eq!(m.subject.as_deref(), Some("Marylynn_and_Scott_Wedding"));
    }

    #[test]
    fn test_parse_folder_year_month_subject() {
        let m = parse_folder_name("2024_January_FamilyReunion");
        assert_eq!(m.year, Some(2024));
        assert_eq!(m.month_or_season.as_deref(), Some("January"));
        assert_eq!(m.subject.as_deref(), Some("FamilyReunion"));
    }

    #[test]
    fn test_parse_folder_year_season_subject() {
        let m = parse_folder_name("2024_Spring_FamilyReunion");
        assert_eq!(m.year, Some(2024));
        assert_eq!(m.month_or_season.as_deref(), Some("Spring"));
        assert_eq!(m.subject.as_deref(), Some("FamilyReunion"));
    }

    #[test]
    fn test_parse_folder_year_and_season_only() {
        let m = parse_folder_name("2024_Fall");
        assert_eq!(m.year, Some(2024));
        assert_eq!(m.month_or_season.as_deref(), Some("Fall"));
        assert!(m.subject.is_none());
    }

    #[test]
    fn test_parse_folder_subject_only() {
        let m = parse_folder_name("SteveJones");
        assert!(m.year.is_none());
        assert!(m.month_or_season.is_none());
        assert_eq!(m.subject.as_deref(), Some("SteveJones"));
    }

    #[test]
    fn test_parse_folder_subject_with_underscores() {
        let m = parse_folder_name("Some_Random_Folder");
        // "Some" is not a year → entire string is subject
        assert!(m.year.is_none());
        assert_eq!(m.subject.as_deref(), Some("Some_Random_Folder"));
    }

    #[test]
    fn test_parse_folder_empty_string() {
        let m = parse_folder_name("");
        assert!(m.is_empty());
    }

    #[test]
    fn test_parse_folder_whitespace_only() {
        let m = parse_folder_name("   ");
        assert!(m.is_empty());
    }

    #[test]
    fn test_parse_folder_all_seasons() {
        for season in &["Spring", "Summer", "Fall", "Winter"] {
            let name = format!("2020_{season}");
            let m = parse_folder_name(&name);
            assert_eq!(m.year, Some(2020));
            assert_eq!(m.month_or_season.as_deref(), Some(*season));
            assert!(
                m.subject.is_none(),
                "season {season} got unexpected subject"
            );
        }
    }

    #[test]
    fn test_parse_folder_all_months() {
        for month in &[
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ] {
            let name = format!("2020_{month}");
            let m = parse_folder_name(&name);
            assert_eq!(m.year, Some(2020));
            assert_eq!(m.month_or_season.as_deref(), Some(*month));
        }
    }

    #[test]
    fn test_parse_folder_case_insensitive_season() {
        let m = parse_folder_name("2020_summer_Beach");
        assert_eq!(m.year, Some(2020));
        assert_eq!(m.month_or_season.as_deref(), Some("summer"));
        assert_eq!(m.subject.as_deref(), Some("Beach"));
    }

    #[test]
    fn test_parse_folder_year_and_subject_highres() {
        let m = parse_folder_name("2004_JonesFamily_HighRes");
        assert_eq!(m.year, Some(2004));
        assert!(m.month_or_season.is_none());
        assert_eq!(m.subject.as_deref(), Some("JonesFamily_HighRes"));
    }

    #[test]
    fn test_parse_folder_real_fastfoto_folders() {
        // All the folders from the user's actual FastFoto library
        let cases = vec![
            ("1977_JonesFamily", Some(1977), None, Some("JonesFamily")),
            ("1983", Some(1983), None, None),
            ("1984", Some(1984), None, None),
            ("1984_JonesFamily", Some(1984), None, Some("JonesFamily")),
            (
                "1984_Marylynn_and_Scott_Wedding",
                Some(1984),
                None,
                Some("Marylynn_and_Scott_Wedding"),
            ),
            ("1984_Mexico", Some(1984), None, Some("Mexico")),
            ("1993", Some(1993), None, None),
            ("2004_JonesFamily", Some(2004), None, Some("JonesFamily")),
            (
                "2004_JonesFamily_HighRes",
                Some(2004),
                None,
                Some("JonesFamily_HighRes"),
            ),
            ("SteveJones", None, None, Some("SteveJones")),
        ];
        for (name, year, month, subject) in cases {
            let m = parse_folder_name(name);
            assert_eq!(m.year, year, "year mismatch for {name}");
            assert_eq!(
                m.month_or_season.as_deref(),
                month,
                "month mismatch for {name}"
            );
            assert_eq!(m.subject.as_deref(), subject, "subject mismatch for {name}");
        }
    }

    #[test]
    fn test_parse_folder_year_season_multiword_subject() {
        let m = parse_folder_name("2024_Winter_Christmas_at_Grandmas");
        assert_eq!(m.year, Some(2024));
        assert_eq!(m.month_or_season.as_deref(), Some("Winter"));
        assert_eq!(m.subject.as_deref(), Some("Christmas_at_Grandmas"));
    }

    #[test]
    fn test_parse_folder_short_number_not_year() {
        // 3-digit number — not a year, treated as subject
        let m = parse_folder_name("123_Stuff");
        assert!(m.year.is_none());
        assert_eq!(m.subject.as_deref(), Some("123_Stuff"));
    }

    #[test]
    fn test_parse_folder_may_is_month() {
        // "May" is both a name and a month — when following a year, treat as month
        let m = parse_folder_name("2020_May_Flowers");
        assert_eq!(m.year, Some(2020));
        assert_eq!(m.month_or_season.as_deref(), Some("May"));
        assert_eq!(m.subject.as_deref(), Some("Flowers"));
    }

    #[test]
    fn test_folder_meta_is_empty() {
        assert!(FolderMeta::default().is_empty());
        assert!(!parse_folder_name("1984").is_empty());
        assert!(!parse_folder_name("SteveJones").is_empty());
    }
}
