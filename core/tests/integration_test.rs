//! Integration tests for photostax-core.
//!
//! These tests verify end-to-end workflows using the test fixtures.

mod test_helpers;

use std::io::Read;
use std::path::PathBuf;

use photostax_core::backends::local::LocalRepository;
use photostax_core::metadata::exif;
use photostax_core::metadata::xmp;
use photostax_core::photo_stack::Metadata;
use photostax_core::repository::Repository;
use photostax_core::search::{filter_stacks, SearchQuery};

/// Get the path to the testdata directory.
fn testdata_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("testdata")
}

#[test]
fn test_end_to_end_scan_search_metadata() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path();

    // Create test repository with helper
    test_helpers::create_test_repository(dir);

    // Create repository and scan
    let repo = LocalRepository::new(dir);
    let stacks = repo.scan().unwrap();

    // Verify expected stacks were found
    assert!(
        stacks.len() >= 5,
        "Expected at least 5 stacks, found {}",
        stacks.len()
    );
    assert!(stacks.iter().any(|s| s.name() == "FamilyPhotos_0001"));
    assert!(stacks.iter().any(|s| s.name() == "FamilyPhotos_0002"));
    assert!(stacks.iter().any(|s| s.name() == "FamilyPhotos_0003"));
    assert!(stacks.iter().any(|s| s.name() == "FamilyPhotos_0004"));
    assert!(stacks.iter().any(|s| s.name() == "FamilyPhotos_0005"));

    // Search for stacks with back scans
    let q = SearchQuery::new().with_has_back(true);
    let with_back = filter_stacks(&stacks, &q);
    assert!(with_back.iter().any(|s| s.name() == "FamilyPhotos_0001"));
    assert!(with_back.iter().any(|s| s.name() == "FamilyPhotos_0005"));

    // Verify all 3 stack configurations:

    // Config 1: Original only (1 file, no enhanced, no back)
    let stack_0004 = stacks
        .iter()
        .find(|s| s.name() == "FamilyPhotos_0004")
        .unwrap();
    assert!(stack_0004.original().is_present());
    assert!(!stack_0004.enhanced().is_present());
    assert!(!stack_0004.back().is_present());

    // Config 2: Original + _a file (2 files; with the default classifier
    // synthetic solid-colour images may be reclassified)
    let stack_0002 = stacks
        .iter()
        .find(|s| s.name() == "FamilyPhotos_0002")
        .unwrap();
    assert!(stack_0002.original().is_present());
    // The _a file is present as either enhanced or back depending on classification
    assert!(stack_0002.enhanced().is_present() || stack_0002.back().is_present());

    // Config 3: Original + back (2 files, no enhanced)
    let stack_0005 = stacks
        .iter()
        .find(|s| s.name() == "FamilyPhotos_0005")
        .unwrap();
    assert!(stack_0005.original().is_present());
    assert!(!stack_0005.enhanced().is_present());
    assert!(stack_0005.back().is_present());

    // Get specific stack and verify structure
    let stack = {
        let stacks_tmp = repo.scan().unwrap();
        stacks_tmp
            .into_iter()
            .find(|s| s.name() == "FamilyPhotos_0001")
            .unwrap()
    };
    assert!(stack.original().is_present());
    assert!(stack.enhanced().is_present());
    assert!(stack.back().is_present());

    // Write metadata via handle
    let mut metadata = Metadata::default();
    metadata
        .xmp_tags
        .insert("description".to_string(), "Family photo 2024".to_string());
    metadata
        .custom_tags
        .insert("ocr_text".to_string(), serde_json::json!("Reunion 2024"));

    stack.metadata().write(&metadata).unwrap();

    // Re-scan and verify metadata persists (use scan_with_metadata to load sidecar)
    let stacks_after = repo.scan_with_metadata().unwrap();
    let stack_after = stacks_after
        .iter()
        .find(|s| s.name() == "FamilyPhotos_0001")
        .unwrap();

    // Custom tags should be in sidecar
    let meta_after = stack_after.metadata().cached().unwrap();
    assert_eq!(
        meta_after.custom_tags.get("ocr_text"),
        Some(&serde_json::json!("Reunion 2024"))
    );

    // XMP tags should be readable
    assert!(
        meta_after.xmp_tags.contains_key("description")
            || meta_after.custom_tags.contains_key("xmp:description")
    );
}

#[test]
fn test_xmp_readable_by_exif_tools() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path();

    // Create a JPEG stack with enhanced image
    test_helpers::create_fastfoto_stack(dir, "TestPhoto", 1, true, false, "jpg");

    let repo = LocalRepository::new(dir);
    let stack = {
        let stacks_tmp = repo.scan().unwrap();
        stacks_tmp
            .into_iter()
            .find(|s| s.name() == "TestPhoto_0001")
            .unwrap()
    };

    // Write XMP metadata via handle
    let mut metadata = Metadata::default();
    metadata.xmp_tags.insert(
        "description".to_string(),
        "XMP test description".to_string(),
    );
    metadata
        .xmp_tags
        .insert("creator".to_string(), "Test Author".to_string());

    stack.metadata().write(&metadata).unwrap();

    // write_metadata prefers enhanced image, so read from enhanced.
    // Construct path directly since ImageRef does not expose paths.
    let target_path = dir.join("TestPhoto_0001_a.jpg");
    let xmp_tags = xmp::read_xmp(&target_path).unwrap();
    assert_eq!(
        xmp_tags.get("description"),
        Some(&"XMP test description".to_string())
    );
    assert_eq!(xmp_tags.get("creator"), Some(&"Test Author".to_string()));
}

#[test]
fn test_with_committed_testdata() {
    let testdata = testdata_path();

    // Skip if testdata doesn't exist (might be running in different context)
    if !testdata.exists() {
        eprintln!("Skipping test_with_committed_testdata: testdata directory not found");
        return;
    }

    let repo = LocalRepository::new(&testdata);
    let stacks = repo.scan().unwrap();

    // Verify we can scan the committed testdata
    assert!(!stacks.is_empty(), "Testdata should contain stacks");

    // Verify EXIF data is readable from committed files.
    // Read directly from files in the testdata directory since ImageRef
    // does not expose file paths.
    let mut found_exif = false;
    for entry in std::fs::read_dir(&testdata).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| {
            let e = ext.to_string_lossy().to_lowercase();
            e == "jpg" || e == "jpeg" || e == "tif" || e == "tiff"
        }) {
            if let Ok(tags) = exif::read_exif_tags(&path) {
                if tags.contains_key("Make") || tags.contains_key("Model") {
                    found_exif = true;
                }
            }
        }
    }
    assert!(
        found_exif,
        "Expected at least one image with EXIF tags in test fixtures"
    );
}

#[test]
fn test_search_workflow() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path();

    test_helpers::create_test_repository(dir);

    let repo = LocalRepository::new(dir);

    // First, write some custom metadata for searching
    let stacks = repo.scan().unwrap();

    // Add OCR text to one stack via handle
    if let Some(stack) = stacks.iter().find(|s| s.name() == "FamilyPhotos_0001") {
        let mut metadata = Metadata::default();
        metadata
            .custom_tags
            .insert("ocr_text".to_string(), serde_json::json!("Birthday party"));
        stack.metadata().write(&metadata).unwrap();
    }

    // Add different OCR text to another stack via handle
    if let Some(stack) = stacks.iter().find(|s| s.name() == "FamilyPhotos_0002") {
        let mut metadata = Metadata::default();
        metadata.custom_tags.insert(
            "ocr_text".to_string(),
            serde_json::json!("Wedding ceremony"),
        );
        stack.metadata().write(&metadata).unwrap();
    }

    // Re-scan with metadata to pick up sidecar data for searching
    let stacks = repo.scan_with_metadata().unwrap();

    // Search for birthday
    let q = SearchQuery::new().with_text("birthday");
    let results = filter_stacks(&stacks, &q);
    assert!(results.iter().any(|s| s.name() == "FamilyPhotos_0001"));
    assert!(!results.iter().any(|s| s.name() == "FamilyPhotos_0002"));

    // Search for wedding
    let q = SearchQuery::new().with_text("wedding");
    let results = filter_stacks(&stacks, &q);
    assert!(results.iter().any(|s| s.name() == "FamilyPhotos_0002"));
}

#[test]
fn test_tiff_workflow() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path();

    // Create TIFF stack without enhanced (so original is used)
    test_helpers::create_fastfoto_stack(dir, "TiffTest", 1, false, true, "tif");

    let repo = LocalRepository::new(dir);
    let stacks = repo.scan().unwrap();

    assert_eq!(stacks.len(), 1);
    let stack = &stacks[0];
    assert_eq!(stack.name(), "TiffTest_0001");

    // Write XMP metadata via handle (should create sidecar for TIFF)
    let mut metadata = Metadata::default();
    metadata
        .xmp_tags
        .insert("description".to_string(), "TIFF test".to_string());

    stack.metadata().write(&metadata).unwrap();

    // Verify sidecar was created - write_metadata uses enhanced or original.
    // Construct path directly since ImageRef does not expose paths.
    let target_path = dir.join("TiffTest_0001.tif");
    let sidecar_path = target_path.with_extension("xmp");
    assert!(
        sidecar_path.exists(),
        "XMP sidecar should be created for TIFF"
    );

    // Clean up
    let _ = std::fs::remove_file(sidecar_path);
}

#[test]
fn test_read_image_content() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path();

    // Create test image with known content
    test_helpers::create_fastfoto_stack(dir, "ReadTest", 1, false, false, "jpg");

    let repo = LocalRepository::new(dir);
    let stacks = repo.scan().unwrap();
    let stack = &stacks[0];

    // Read the image via handle
    let mut content = Vec::new();
    stack
        .original()
        .read()
        .unwrap()
        .read_to_end(&mut content)
        .unwrap();

    // Verify it's a valid JPEG (starts with SOI marker)
    assert_eq!(&content[0..2], &[0xFF, 0xD8]);
}

#[test]
fn test_mixed_format_stack() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path();

    // Create the mixed format stack from test_helpers
    test_helpers::create_test_repository(dir);

    let repo = LocalRepository::new(dir);
    let stack = {
        let stacks_tmp = repo.scan().unwrap();
        stacks_tmp
            .into_iter()
            .find(|s| s.name() == "MixedBatch_0001")
            .unwrap()
    };

    // Original should be present
    assert!(stack.original().is_present());

    // Back should be present
    assert!(stack.back().is_present());
}
