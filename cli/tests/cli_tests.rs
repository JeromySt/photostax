//! Integration tests for the photostax-cli.
//!
//! These tests verify command-line functionality by creating temp directories
//! with test fixtures and running CLI commands against them.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

/// Get the path to the CLI binary
fn cli_bin() -> PathBuf {
    // Cargo puts binaries in target/<profile>/photostax-cli[.exe]
    // We can use CARGO_BIN_EXE_photostax-cli if building through cargo test
    // or construct the path manually
    let mut path = std::env::current_exe()
        .unwrap()
        .parent() // deps folder
        .unwrap()
        .parent() // debug/release folder
        .unwrap()
        .to_path_buf();

    path.push("photostax-cli");

    #[cfg(windows)]
    path.set_extension("exe");

    path
}

/// Create a test directory with JPEG fixtures
fn create_test_fixtures() -> TempDir {
    let tmp = TempDir::new().unwrap();

    // Create minimal JPEG files (valid JPEG header + minimal data)
    // JPEG files start with FF D8 FF E0 and end with FF D9
    let jpeg_data: Vec<u8> = vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06,
        0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B,
        0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
        0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31,
        0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF,
        0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00,
        0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
        0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05,
        0x04, 0x04, 0x00, 0x00, 0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21,
        0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08,
        0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A,
        0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37,
        0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56,
        0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75,
        0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93,
        0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9,
        0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6,
        0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
        0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7,
        0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xFB, 0xD3,
        0xFF, 0xD9,
    ];

    // Create stack IMG_001 (original + enhanced + back)
    write_file(tmp.path().join("IMG_001.jpg"), &jpeg_data);
    write_file(tmp.path().join("IMG_001_a.jpg"), &jpeg_data);
    write_file(tmp.path().join("IMG_001_b.jpg"), &jpeg_data);

    // Create stack IMG_002 (original + enhanced, no back)
    write_file(tmp.path().join("IMG_002.jpg"), &jpeg_data);
    write_file(tmp.path().join("IMG_002_a.jpg"), &jpeg_data);

    // Create stack IMG_003 (original only)
    write_file(tmp.path().join("IMG_003.jpg"), &jpeg_data);

    tmp
}

fn write_file(path: std::path::PathBuf, data: &[u8]) {
    let mut f = File::create(path).unwrap();
    f.write_all(data).unwrap();
}

fn run_cli(args: &[&str]) -> Output {
    Command::new(cli_bin())
        .args(args)
        .output()
        .expect("failed to run CLI")
}

// ============================================================================
// Help output tests
// ============================================================================

#[test]
fn test_help_output() {
    let output = run_cli(&["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("photostax-cli"));
    assert!(stdout.contains("scan"));
    assert!(stdout.contains("search"));
    assert!(stdout.contains("info"));
    assert!(stdout.contains("metadata"));
    assert!(stdout.contains("export"));
}

#[test]
fn test_scan_help() {
    let output = run_cli(&["scan", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("DIRECTORY"));
    assert!(stdout.contains("--format"));
    assert!(stdout.contains("--tiff-only"));
    assert!(stdout.contains("--jpeg-only"));
    assert!(stdout.contains("--with-back"));
}

#[test]
fn test_search_help() {
    let output = run_cli(&["search", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("QUERY"));
    assert!(stdout.contains("--exif"));
    assert!(stdout.contains("--tag"));
    assert!(stdout.contains("--has-back"));
}

#[test]
fn test_info_help() {
    let output = run_cli(&["info", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("STACK_ID"));
    assert!(stdout.contains("--format"));
}

#[test]
fn test_metadata_help() {
    let output = run_cli(&["metadata", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("read"));
    assert!(stdout.contains("write"));
    assert!(stdout.contains("delete"));
}

#[test]
fn test_export_help() {
    let output = run_cli(&["export", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("--output"));
}

// ============================================================================
// Scan command tests
// ============================================================================

#[test]
fn test_scan_table_output() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["scan", tmp.path().to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("3 photo stack(s)"));
    assert!(stdout.contains("IMG_001"));
    assert!(stdout.contains("IMG_002"));
    assert!(stdout.contains("IMG_003"));
}

#[test]
fn test_scan_json_output() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["scan", tmp.path().to_str().unwrap(), "--format", "json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());

    // Should be valid JSON
    let stacks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(stacks.len(), 3);

    // Check that stacks have expected fields
    let names: Vec<&str> = stacks.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"IMG_001"));
    assert!(names.contains(&"IMG_002"));
    assert!(names.contains(&"IMG_003"));
}

#[test]
fn test_scan_csv_output() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["scan", tmp.path().to_str().unwrap(), "--format", "csv"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("id,format,"));
    assert!(stdout.contains("IMG_001,-,"));
    assert!(stdout.contains("IMG_002,-,"));
    assert!(stdout.contains("IMG_003,-,"));
}

#[test]
fn test_scan_with_back_filter() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["scan", tmp.path().to_str().unwrap(), "--with-back"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("1 photo stack(s)"));
    assert!(stdout.contains("IMG_001"));
    assert!(!stdout.contains("IMG_002"));
    assert!(!stdout.contains("IMG_003"));
}

#[test]
fn test_scan_jpeg_only() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["scan", tmp.path().to_str().unwrap(), "--jpeg-only"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("3 photo stack(s)"));
}

// ============================================================================
// Search command tests
// ============================================================================

#[test]
fn test_search_by_id() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["search", tmp.path().to_str().unwrap(), "001"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("IMG_001"));
    assert!(!stdout.contains("IMG_002"));
    assert!(!stdout.contains("IMG_003"));
}

#[test]
fn test_search_with_has_back() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["search", tmp.path().to_str().unwrap(), "IMG", "--has-back"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("IMG_001"));
    assert!(!stdout.contains("IMG_002"));
}

#[test]
fn test_search_json_output() {
    let tmp = create_test_fixtures();
    let output = run_cli(&[
        "search",
        tmp.path().to_str().unwrap(),
        "IMG",
        "--format",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());

    // Should be valid JSON
    let _: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
}

// ============================================================================
// Info command tests
// ============================================================================

#[test]
fn test_info_existing_stack() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["info", tmp.path().to_str().unwrap(), "IMG_001"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("IMG_001"));
    assert!(stdout.contains("Original"));
}

#[test]
fn test_info_not_found() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["info", tmp.path().to_str().unwrap(), "NONEXISTENT"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2)); // EXIT_NOT_FOUND
    assert!(stderr.contains("not found"));
}

#[test]
fn test_info_json_output() {
    let tmp = create_test_fixtures();
    let output = run_cli(&[
        "info",
        tmp.path().to_str().unwrap(),
        "IMG_001",
        "--format",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());

    // Should be valid JSON
    let stack: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(stack["name"].as_str().unwrap(), "IMG_001");
}

// ============================================================================
// Metadata command tests
// ============================================================================

#[test]
fn test_metadata_read() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["metadata", "read", tmp.path().to_str().unwrap(), "IMG_001"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Metadata") || stdout.contains("EXIF") || stdout.contains("Custom"));
}

#[test]
fn test_metadata_write_and_read() {
    let tmp = create_test_fixtures();

    // Write a tag
    let output = run_cli(&[
        "metadata",
        "write",
        tmp.path().to_str().unwrap(),
        "IMG_001",
        "--tag",
        "album=Family Photos",
    ]);
    assert!(output.status.success());

    // Read it back
    let output = run_cli(&[
        "metadata",
        "read",
        tmp.path().to_str().unwrap(),
        "IMG_001",
        "--format",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    let meta: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        meta["custom_tags"]["album"].as_str().unwrap(),
        "Family Photos"
    );
}

#[test]
fn test_metadata_delete() {
    let tmp = create_test_fixtures();

    // First write a tag
    let output = run_cli(&[
        "metadata",
        "write",
        tmp.path().to_str().unwrap(),
        "IMG_001",
        "--tag",
        "temp_tag=delete_me",
    ]);
    assert!(output.status.success());

    // Now delete it
    let output = run_cli(&[
        "metadata",
        "delete",
        tmp.path().to_str().unwrap(),
        "IMG_001",
        "--tag",
        "temp_tag",
    ]);
    assert!(output.status.success());

    // Verify it's gone
    let output = run_cli(&[
        "metadata",
        "read",
        tmp.path().to_str().unwrap(),
        "IMG_001",
        "--format",
        "json",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let meta: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(meta["custom_tags"]["temp_tag"].is_null());
}

#[test]
fn test_metadata_not_found() {
    let tmp = create_test_fixtures();
    let output = run_cli(&[
        "metadata",
        "read",
        tmp.path().to_str().unwrap(),
        "NONEXISTENT",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2)); // EXIT_NOT_FOUND
    assert!(stderr.contains("not found"));
}

// ============================================================================
// Export command tests
// ============================================================================

#[test]
fn test_export_to_stdout() {
    let tmp = create_test_fixtures();
    let output = run_cli(&["export", tmp.path().to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());

    // Should be valid JSON array
    let stacks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(stacks.len(), 3);
}

#[test]
fn test_export_to_file() {
    let tmp = create_test_fixtures();
    let output_file = tmp.path().join("export.json");

    let output = run_cli(&[
        "export",
        tmp.path().to_str().unwrap(),
        "--output",
        output_file.to_str().unwrap(),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Exported 3 stack(s)"));

    // Verify file exists and contains valid JSON
    let content = fs::read_to_string(&output_file).unwrap();
    let stacks: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();
    assert_eq!(stacks.len(), 3);
}

// ============================================================================
// Error case tests
// ============================================================================

#[test]
fn test_scan_missing_directory() {
    let output = run_cli(&["scan", "/nonexistent/path/that/does/not/exist"]);

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1)); // EXIT_ERROR
}

#[test]
fn test_invalid_key_value_format() {
    let tmp = create_test_fixtures();
    let output = run_cli(&[
        "metadata",
        "write",
        tmp.path().to_str().unwrap(),
        "IMG_001",
        "--tag",
        "invalid_format",
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("KEY=VALUE"));
}
