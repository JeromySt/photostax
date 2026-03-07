//! Generator script to create static test data files.
//!
//! Run with: cargo test --package photostax-core generate_testdata --features generate-testdata -- --ignored

mod test_helpers;

use std::fs;
use test_helpers::{create_jpeg_with_exif, create_tiff_with_exif};

/// Generate all static test data files for the testdata directory.
/// This is an ignored test that should be run manually when test data needs regeneration.
#[test]
#[ignore]
fn generate_testdata() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let testdata_dir = manifest_dir.join("tests").join("testdata");

    // Create directory if it doesn't exist
    fs::create_dir_all(&testdata_dir).expect("Failed to create testdata directory");

    let default_tags: Vec<(&str, &str)> = vec![
        ("Make", "EPSON"),
        ("Model", "FastFoto FF-680W"),
        ("Software", "EPSON FastFoto"),
        ("DateTime", "2024:06:15 14:30:00"),
    ];

    println!("Generating test data files in: {}", testdata_dir.display());

    // Helper to create JPEG
    let create_jpg = |name: &str| {
        let path = testdata_dir.join(name);
        let data = create_jpeg_with_exif(&default_tags);
        fs::write(&path, &data).unwrap_or_else(|_| panic!("Failed to write {}", name));
        println!("  Created: {} ({} bytes)", name, data.len());
    };

    // Helper to create TIFF
    let create_tif = |name: &str| {
        let path = testdata_dir.join(name);
        let data = create_tiff_with_exif(&default_tags);
        fs::write(&path, &data).unwrap_or_else(|_| panic!("Failed to write {}", name));
        println!("  Created: {} ({} bytes)", name, data.len());
    };

    // FamilyPhotos_0001 - Full JPEG stack
    create_jpg("FamilyPhotos_0001.jpg");
    create_jpg("FamilyPhotos_0001_a.jpg");
    create_jpg("FamilyPhotos_0001_b.jpg");

    // FamilyPhotos_0002 - Partial (original + enhanced only)
    create_jpg("FamilyPhotos_0002.jpg");
    create_jpg("FamilyPhotos_0002_a.jpg");

    // FamilyPhotos_0003 - Full TIFF stack
    create_tif("FamilyPhotos_0003.tif");
    create_tif("FamilyPhotos_0003_a.tif");
    create_tif("FamilyPhotos_0003_b.tif");

    // FamilyPhotos_0004 - Lonely original
    create_jpg("FamilyPhotos_0004.jpg");

    // FamilyPhotos_0005 - Original + back (no enhanced)
    create_jpg("FamilyPhotos_0005.jpg");
    create_jpg("FamilyPhotos_0005_b.jpg");

    // MixedBatch_0001 - Mixed formats
    create_jpg("MixedBatch_0001.jpg");
    create_jpg("MixedBatch_0001_a.jpg");
    create_tif("MixedBatch_0001_b.tif");

    println!("Test data generation complete!");
}
