use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::photo_stack::PhotoStack;

/// Configuration for the FastFoto file scanner.
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// Suffix appended to the base name for enhanced images (default: `_a`).
    pub enhanced_suffix: String,
    /// Suffix appended to the base name for back-of-photo images (default: `_b`).
    pub back_suffix: String,
    /// File extensions to consider (default: `["jpg", "jpeg"]`).
    pub extensions: Vec<String>,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            enhanced_suffix: "_a".to_string(),
            back_suffix: "_b".to_string(),
            extensions: vec!["jpg".to_string(), "jpeg".to_string()],
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
}
