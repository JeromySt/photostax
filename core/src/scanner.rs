use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;

use crate::photo_stack::PhotoStack;

/// Configuration for the FastFoto file scanner.
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// Suffix appended to the base name for enhanced images (default: `_a`).
    pub enhanced_suffix: String,
    /// Suffix appended to the base name for back-of-photo images (default: `_b`).
    pub back_suffix: String,
    /// File extensions to consider (default: `["jpg", "jpeg", "tif", "tiff"]`).
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

#[derive(Debug)]
enum Variant {
    Original,
    Enhanced,
    Back,
}

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
}
