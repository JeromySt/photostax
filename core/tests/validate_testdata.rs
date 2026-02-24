//! Integration test that validates all committed testdata files.
//!
//! Ensures that each test file:
//! - Can be parsed by `kamadak-exif`
//! - Contains expected EXIF tags
//! - All expected files exist

mod test_helpers;

use exif::{In, Reader, Tag};
use std::fs;
use std::io::BufReader;

/// List of all expected test data files
const EXPECTED_FILES: &[&str] = &[
    "FamilyPhotos_0001.jpg",
    "FamilyPhotos_0001_a.jpg",
    "FamilyPhotos_0001_b.jpg",
    "FamilyPhotos_0002.jpg",
    "FamilyPhotos_0002_a.jpg",
    "FamilyPhotos_0003.tif",
    "FamilyPhotos_0003_a.tif",
    "FamilyPhotos_0003_b.tif",
    "FamilyPhotos_0004.jpg",
    "MixedBatch_0001.jpg",
    "MixedBatch_0001_a.jpg",
    "MixedBatch_0001_b.tif",
];

fn testdata_dir() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("tests").join("testdata")
}

#[test]
fn test_all_expected_files_exist() {
    let dir = testdata_dir();

    for filename in EXPECTED_FILES {
        let path = dir.join(filename);
        assert!(
            path.exists(),
            "Expected test file does not exist: {}",
            path.display()
        );
    }
}

#[test]
fn test_jpeg_files_have_valid_exif() {
    let dir = testdata_dir();

    let jpeg_files: Vec<_> = EXPECTED_FILES
        .iter()
        .filter(|f| f.ends_with(".jpg"))
        .collect();

    for filename in jpeg_files {
        let path = dir.join(filename);
        let file = fs::File::open(&path).expect(&format!("Failed to open {}", filename));
        let mut reader = BufReader::new(file);

        let exif = Reader::new()
            .read_from_container(&mut reader)
            .expect(&format!("Failed to parse EXIF from {}", filename));

        // Verify Make tag exists
        let make = exif.get_field(Tag::Make, In::PRIMARY);
        assert!(
            make.is_some(),
            "Missing Make tag in {}",
            filename
        );

        // Verify Model tag exists
        let model = exif.get_field(Tag::Model, In::PRIMARY);
        assert!(
            model.is_some(),
            "Missing Model tag in {}",
            filename
        );
    }
}

#[test]
fn test_tiff_files_have_valid_exif() {
    let dir = testdata_dir();

    let tiff_files: Vec<_> = EXPECTED_FILES
        .iter()
        .filter(|f| f.ends_with(".tif"))
        .collect();

    for filename in tiff_files {
        let path = dir.join(filename);
        let file = fs::File::open(&path).expect(&format!("Failed to open {}", filename));
        let mut reader = BufReader::new(file);

        let exif = Reader::new()
            .read_from_container(&mut reader)
            .expect(&format!("Failed to parse EXIF from {}", filename));

        // Verify Make tag exists
        let make = exif.get_field(Tag::Make, In::PRIMARY);
        assert!(
            make.is_some(),
            "Missing Make tag in {}",
            filename
        );

        // Verify Model tag exists
        let model = exif.get_field(Tag::Model, In::PRIMARY);
        assert!(
            model.is_some(),
            "Missing Model tag in {}",
            filename
        );
    }
}

#[test]
fn test_all_files_under_5kb() {
    let dir = testdata_dir();

    for filename in EXPECTED_FILES {
        let path = dir.join(filename);
        let metadata = fs::metadata(&path)
            .expect(&format!("Failed to get metadata for {}", filename));

        assert!(
            metadata.len() < 5 * 1024,
            "File {} is too large: {} bytes (limit: 5KB)",
            filename,
            metadata.len()
        );
    }
}

#[test]
fn test_exif_tags_have_expected_values() {
    let dir = testdata_dir();

    // Test a few representative files for expected EXIF values
    let sample_file = dir.join("FamilyPhotos_0001.jpg");
    let file = fs::File::open(&sample_file).expect("Failed to open sample file");
    let mut reader = BufReader::new(file);

    let exif = Reader::new()
        .read_from_container(&mut reader)
        .expect("Failed to parse EXIF from sample file");

    // Check Make value
    if let Some(make) = exif.get_field(Tag::Make, In::PRIMARY) {
        let value = make.display_value().to_string();
        assert!(
            value.contains("EPSON"),
            "Make should contain 'EPSON', got: {}",
            value
        );
    }

    // Check Model value
    if let Some(model) = exif.get_field(Tag::Model, In::PRIMARY) {
        let value = model.display_value().to_string();
        assert!(
            value.contains("FastFoto"),
            "Model should contain 'FastFoto', got: {}",
            value
        );
    }
}
