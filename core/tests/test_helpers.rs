//! Test helper functions for creating synthetic FastFoto test fixtures.
//!
//! Provides utilities to generate valid JPEG and TIFF files with EXIF data
//! that are parseable by the `kamadak-exif` crate.

use std::fs;
use std::path::{Path, PathBuf};

/// Create a minimal valid JPEG file with EXIF data embedded.
/// The JPEG must be parseable by the `kamadak-exif` crate.
///
/// # Arguments
/// * `tags` - Slice of (tag_name, value) pairs. Supported tags:
///   - "Make", "Model", "Software", "DateTime"
///   - "ImageWidth", "ImageLength" (as string integers)
///   - "XResolution", "YResolution" (as string integers)
pub fn create_jpeg_with_exif(tags: &[(&str, &str)]) -> Vec<u8> {
    // Default values for EXIF tags
    let make = find_tag(tags, "Make").unwrap_or("EPSON");
    let model = find_tag(tags, "Model").unwrap_or("FastFoto FF-680W");
    let software = find_tag(tags, "Software").unwrap_or("EPSON FastFoto");
    let datetime = find_tag(tags, "DateTime").unwrap_or("2024:06:15 14:30:00");
    let width: u32 = find_tag(tags, "ImageWidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let height: u32 = find_tag(tags, "ImageLength")
        .and_then(|s| s.parse().ok())
        .unwrap_or(75);
    let x_res: u32 = find_tag(tags, "XResolution")
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let y_res: u32 = find_tag(tags, "YResolution")
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    let mut buf = Vec::new();

    // SOI marker
    buf.extend_from_slice(&[0xFF, 0xD8]);

    // Build EXIF APP1 segment
    let exif_data = build_exif_ifd(make, model, software, datetime, width, height, x_res, y_res);
    let app1_len = (exif_data.len() + 2) as u16;
    buf.extend_from_slice(&[0xFF, 0xE1]); // APP1 marker
    buf.extend_from_slice(&app1_len.to_be_bytes()); // Segment length
    buf.extend_from_slice(&exif_data);

    // DQT (Define Quantization Table) - minimal
    buf.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x43, 0x00]); // DQT marker + length + table ID
    // Simple quantization table (64 bytes)
    for i in 0..64u8 {
        buf.push(16 + (i / 8));
    }

    // SOF0 (Start of Frame, baseline DCT)
    buf.extend_from_slice(&[0xFF, 0xC0]); // SOF0 marker
    buf.extend_from_slice(&[0x00, 0x0B]); // Length
    buf.push(0x08); // Precision (8 bits)
    buf.extend_from_slice(&(height as u16).to_be_bytes()); // Height
    buf.extend_from_slice(&(width as u16).to_be_bytes()); // Width
    buf.push(0x01); // Number of components (grayscale)
    buf.extend_from_slice(&[0x01, 0x11, 0x00]); // Component: ID=1, sampling=1x1, quant table=0

    // DHT (Define Huffman Table) - minimal DC table
    buf.extend_from_slice(&[0xFF, 0xC4]); // DHT marker
    buf.extend_from_slice(&[0x00, 0x1F]); // Length
    buf.push(0x00); // DC table, ID 0
    // Huffman code lengths (16 bytes) - simple table
    buf.extend_from_slice(&[0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01]);
    buf.extend_from_slice(&[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    // Huffman values (12 bytes)
    buf.extend_from_slice(&[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B]);

    // DHT - AC table
    buf.extend_from_slice(&[0xFF, 0xC4]); // DHT marker
    buf.extend_from_slice(&[0x00, 0xB5]); // Length
    buf.push(0x10); // AC table, ID 0
    // Standard AC Huffman table lengths
    buf.extend_from_slice(&[0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03]);
    buf.extend_from_slice(&[0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D]);
    // AC Huffman values (162 bytes) - fill with standard values
    let ac_values: [u8; 162] = [
        0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61,
        0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52,
        0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25,
        0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45,
        0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64,
        0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83,
        0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99,
        0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6,
        0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3,
        0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8,
        0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA,
    ];
    buf.extend_from_slice(&ac_values);

    // SOS (Start of Scan)
    buf.extend_from_slice(&[0xFF, 0xDA]); // SOS marker
    buf.extend_from_slice(&[0x00, 0x08]); // Length
    buf.push(0x01); // Number of components
    buf.extend_from_slice(&[0x01, 0x00]); // Component 1: DC=0, AC=0
    buf.extend_from_slice(&[0x00, 0x3F, 0x00]); // Spectral selection, successive approx

    // Minimal scan data (produces a small gray image)
    // This is entropy-coded data that represents a minimal valid image
    buf.extend_from_slice(&[0xFB, 0xD3, 0x28, 0x79, 0xF9]);

    // EOI marker
    buf.extend_from_slice(&[0xFF, 0xD9]);

    buf
}

/// Create a minimal valid TIFF file with EXIF-style IFD entries.
/// Must be parseable by `kamadak-exif`.
pub fn create_tiff_with_exif(tags: &[(&str, &str)]) -> Vec<u8> {
    let make = find_tag(tags, "Make").unwrap_or("EPSON");
    let model = find_tag(tags, "Model").unwrap_or("FastFoto FF-680W");
    let software = find_tag(tags, "Software").unwrap_or("EPSON FastFoto");
    let datetime = find_tag(tags, "DateTime").unwrap_or("2024:06:15 14:30:00");
    // Keep dimensions small to stay under 5KB (width * height bytes for strip data)
    let width: u32 = find_tag(tags, "ImageWidth")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let height: u32 = find_tag(tags, "ImageLength")
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    let x_res: u32 = find_tag(tags, "XResolution")
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let y_res: u32 = find_tag(tags, "YResolution")
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    let mut buf = Vec::new();

    // TIFF header (little-endian)
    buf.extend_from_slice(&[0x49, 0x49]); // Little-endian marker "II"
    buf.extend_from_slice(&[0x2A, 0x00]); // Magic number 42
    buf.extend_from_slice(&[0x08, 0x00, 0x00, 0x00]); // IFD0 offset (8)

    // Collect strings and their offsets
    let make_bytes = null_terminated(make);
    let model_bytes = null_terminated(model);
    let software_bytes = null_terminated(software);
    let datetime_bytes = null_terminated(datetime);

    // IFD0 entries (we'll have 13 entries for a valid TIFF)
    let num_entries: u16 = 13;

    // Calculate data area offset (after IFD)
    // IFD header: 2 bytes (count)
    // IFD entries: 13 * 12 = 156 bytes
    // Next IFD pointer: 4 bytes
    // Total IFD: 8 + 2 + 156 + 4 = 170, data starts at 170
    let ifd_start: u32 = 8;
    let data_area_start: u32 = ifd_start + 2 + (num_entries as u32 * 12) + 4;

    // Track current offset in data area
    let mut data_offset = data_area_start;

    // Write IFD count
    buf.extend_from_slice(&num_entries.to_le_bytes());

    // Helper to write an IFD entry
    fn write_ifd_entry(
        buf: &mut Vec<u8>,
        tag: u16,
        field_type: u16,
        count: u32,
        value_or_offset: u32,
    ) {
        buf.extend_from_slice(&tag.to_le_bytes());
        buf.extend_from_slice(&field_type.to_le_bytes());
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&value_or_offset.to_le_bytes());
    }

    // TIFF tag constants
    const TAG_IMAGE_WIDTH: u16 = 256;
    const TAG_IMAGE_LENGTH: u16 = 257;
    const TAG_BITS_PER_SAMPLE: u16 = 258;
    const TAG_COMPRESSION: u16 = 259;
    const TAG_PHOTOMETRIC: u16 = 262;
    const TAG_STRIP_OFFSETS: u16 = 273;
    const TAG_SAMPLES_PER_PIXEL: u16 = 277;
    const TAG_ROWS_PER_STRIP: u16 = 278;
    const TAG_STRIP_BYTE_COUNTS: u16 = 279;
    const TAG_X_RESOLUTION: u16 = 282;
    const TAG_Y_RESOLUTION: u16 = 283;
    #[allow(dead_code)]
    const TAG_RESOLUTION_UNIT: u16 = 296;
    const TAG_MAKE: u16 = 271;
    const TAG_MODEL: u16 = 272;
    #[allow(dead_code)]
    const TAG_SOFTWARE: u16 = 305;
    #[allow(dead_code)]
    const TAG_DATETIME: u16 = 306;

    // Type constants
    const TYPE_SHORT: u16 = 3;
    const TYPE_LONG: u16 = 4;
    const TYPE_RATIONAL: u16 = 5;
    const TYPE_ASCII: u16 = 2;

    // Calculate offsets for each string/data
    let make_offset = data_offset;
    data_offset += make_bytes.len() as u32;

    let model_offset = data_offset;
    data_offset += model_bytes.len() as u32;

    let _software_offset = data_offset;
    data_offset += software_bytes.len() as u32;

    let _datetime_offset = data_offset;
    data_offset += datetime_bytes.len() as u32;

    let x_res_offset = data_offset;
    data_offset += 8; // RATIONAL is 8 bytes

    let y_res_offset = data_offset;
    data_offset += 8;

    let strip_offset = data_offset;
    let strip_size: u32 = (width * height) as u32; // 1 byte per pixel grayscale

    // IFD entries (must be in ascending tag order)
    write_ifd_entry(&mut buf, TAG_IMAGE_WIDTH, TYPE_LONG, 1, width);
    write_ifd_entry(&mut buf, TAG_IMAGE_LENGTH, TYPE_LONG, 1, height);
    write_ifd_entry(&mut buf, TAG_BITS_PER_SAMPLE, TYPE_SHORT, 1, 8); // 8 bits
    write_ifd_entry(&mut buf, TAG_COMPRESSION, TYPE_SHORT, 1, 1); // No compression
    write_ifd_entry(&mut buf, TAG_PHOTOMETRIC, TYPE_SHORT, 1, 1); // BlackIsZero
    write_ifd_entry(&mut buf, TAG_MAKE, TYPE_ASCII, make_bytes.len() as u32, make_offset);
    write_ifd_entry(&mut buf, TAG_MODEL, TYPE_ASCII, model_bytes.len() as u32, model_offset);
    write_ifd_entry(&mut buf, TAG_STRIP_OFFSETS, TYPE_LONG, 1, strip_offset);
    write_ifd_entry(&mut buf, TAG_SAMPLES_PER_PIXEL, TYPE_SHORT, 1, 1);
    write_ifd_entry(&mut buf, TAG_ROWS_PER_STRIP, TYPE_LONG, 1, height);
    write_ifd_entry(&mut buf, TAG_STRIP_BYTE_COUNTS, TYPE_LONG, 1, strip_size);
    write_ifd_entry(&mut buf, TAG_X_RESOLUTION, TYPE_RATIONAL, 1, x_res_offset);
    write_ifd_entry(&mut buf, TAG_Y_RESOLUTION, TYPE_RATIONAL, 1, y_res_offset);

    // We need to add more entries but the above 13 entries are not in order
    // Let me rebuild this properly with sorted tags

    // Clear and rebuild
    buf.clear();

    // TIFF header
    buf.extend_from_slice(&[0x49, 0x49]); // Little-endian "II"
    buf.extend_from_slice(&[0x2A, 0x00]); // Magic 42
    buf.extend_from_slice(&[0x08, 0x00, 0x00, 0x00]); // IFD0 at offset 8

    // We'll write 15 IFD entries
    let num_entries: u16 = 15;
    let ifd_size = 2 + (num_entries as usize * 12) + 4;
    let data_start = 8 + ifd_size;

    // Pre-calculate all data offsets
    let mut cur_offset = data_start as u32;

    let make_off = cur_offset;
    cur_offset += make_bytes.len() as u32;

    let model_off = cur_offset;
    cur_offset += model_bytes.len() as u32;

    let software_off = cur_offset;
    cur_offset += software_bytes.len() as u32;

    let _datetime_off = cur_offset;
    cur_offset += datetime_bytes.len() as u32;

    let xres_off = cur_offset;
    cur_offset += 8;

    let yres_off = cur_offset;
    cur_offset += 8;

    let strip_off = cur_offset;

    // Write IFD count
    buf.extend_from_slice(&num_entries.to_le_bytes());

    // Entries must be sorted by tag number
    // 256 ImageWidth, 257 ImageLength, 258 BitsPerSample, 259 Compression,
    // 262 PhotometricInterpretation, 271 Make, 272 Model, 273 StripOffsets,
    // 277 SamplesPerPixel, 278 RowsPerStrip, 279 StripByteCounts,
    // 282 XResolution, 283 YResolution, 296 ResolutionUnit, 305 Software, 306 DateTime

    write_ifd_entry(&mut buf, 256, TYPE_LONG, 1, width); // ImageWidth
    write_ifd_entry(&mut buf, 257, TYPE_LONG, 1, height); // ImageLength
    write_ifd_entry(&mut buf, 258, TYPE_SHORT, 1, 8); // BitsPerSample
    write_ifd_entry(&mut buf, 259, TYPE_SHORT, 1, 1); // Compression (none)
    write_ifd_entry(&mut buf, 262, TYPE_SHORT, 1, 1); // PhotometricInterpretation
    write_ifd_entry(&mut buf, 271, TYPE_ASCII, make_bytes.len() as u32, make_off); // Make
    write_ifd_entry(&mut buf, 272, TYPE_ASCII, model_bytes.len() as u32, model_off); // Model
    write_ifd_entry(&mut buf, 273, TYPE_LONG, 1, strip_off); // StripOffsets
    write_ifd_entry(&mut buf, 277, TYPE_SHORT, 1, 1); // SamplesPerPixel
    write_ifd_entry(&mut buf, 278, TYPE_LONG, 1, height); // RowsPerStrip
    write_ifd_entry(&mut buf, 279, TYPE_LONG, 1, strip_size); // StripByteCounts
    write_ifd_entry(&mut buf, 282, TYPE_RATIONAL, 1, xres_off); // XResolution
    write_ifd_entry(&mut buf, 283, TYPE_RATIONAL, 1, yres_off); // YResolution
    write_ifd_entry(&mut buf, 296, TYPE_SHORT, 1, 2); // ResolutionUnit (inch)
    write_ifd_entry(&mut buf, 305, TYPE_ASCII, software_bytes.len() as u32, software_off); // Software

    // Next IFD pointer (0 = no more IFDs)
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    // Data area
    buf.extend_from_slice(&make_bytes);
    buf.extend_from_slice(&model_bytes);
    buf.extend_from_slice(&software_bytes);
    buf.extend_from_slice(&datetime_bytes);

    // XResolution as RATIONAL (numerator/denominator)
    buf.extend_from_slice(&x_res.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());

    // YResolution as RATIONAL
    buf.extend_from_slice(&y_res.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());

    // Image strip data (minimal grayscale)
    let strip_data = vec![0x80u8; strip_size as usize]; // Mid-gray
    buf.extend_from_slice(&strip_data);

    buf
}

/// Create a complete FastFoto stack in `dir` with given prefix/sequence.
///
/// # Arguments
/// * `dir` - Directory to create files in
/// * `prefix` - Filename prefix (e.g., "FamilyPhotos")
/// * `seq` - Sequence number (will be formatted as 4-digit)
/// * `include_enhanced` - Whether to create the `_a` variant
/// * `include_back` - Whether to create the `_b` variant
/// * `format` - File format: "jpg" or "tif"
///
/// # Returns
/// Vector of paths to created files
pub fn create_fastfoto_stack(
    dir: &Path,
    prefix: &str,
    seq: u32,
    include_enhanced: bool,
    include_back: bool,
    format: &str,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let ext = match format {
        "tif" | "tiff" => "tif",
        _ => "jpg",
    };

    let base_name = format!("{}_{:04}", prefix, seq);
    let tags: Vec<(&str, &str)> = vec![
        ("Make", "EPSON"),
        ("Model", "FastFoto FF-680W"),
        ("Software", "EPSON FastFoto"),
        ("DateTime", "2024:06:15 14:30:00"),
    ];

    // Original
    let original_name = format!("{}.{}", base_name, ext);
    let original_path = dir.join(&original_name);
    let data = if ext == "tif" {
        create_tiff_with_exif(&tags)
    } else {
        create_jpeg_with_exif(&tags)
    };
    fs::write(&original_path, &data).expect("Failed to write original file");
    paths.push(original_path);

    // Enhanced (_a)
    if include_enhanced {
        let enhanced_name = format!("{}_a.{}", base_name, ext);
        let enhanced_path = dir.join(&enhanced_name);
        fs::write(&enhanced_path, &data).expect("Failed to write enhanced file");
        paths.push(enhanced_path);
    }

    // Back (_b)
    if include_back {
        let back_name = format!("{}_b.{}", base_name, ext);
        let back_path = dir.join(&back_name);
        fs::write(&back_path, &data).expect("Failed to write back file");
        paths.push(back_path);
    }

    paths
}

/// Populate a directory with a standard set of FastFoto test files.
///
/// Creates the following structure:
/// - FamilyPhotos_0001.jpg + _a + _b (full JPEG stack)
/// - FamilyPhotos_0002.jpg + _a (partial - no back)
/// - FamilyPhotos_0003.tif + _a + _b (full TIFF stack)
/// - FamilyPhotos_0004.jpg (lonely original)
/// - MixedBatch_0001.jpg + _a + _b.tif (mixed formats)
pub fn create_test_repository(dir: &Path) {
    // FamilyPhotos_0001 - full JPEG stack
    create_fastfoto_stack(dir, "FamilyPhotos", 1, true, true, "jpg");

    // FamilyPhotos_0002 - partial (original + enhanced only)
    create_fastfoto_stack(dir, "FamilyPhotos", 2, true, false, "jpg");

    // FamilyPhotos_0003 - full TIFF stack
    create_fastfoto_stack(dir, "FamilyPhotos", 3, true, true, "tif");

    // FamilyPhotos_0004 - lonely original
    create_fastfoto_stack(dir, "FamilyPhotos", 4, false, false, "jpg");

    // MixedBatch_0001 - mixed formats (jpg original/enhanced, tif back)
    create_fastfoto_stack(dir, "MixedBatch", 1, true, false, "jpg");
    // Create TIFF back separately
    let tags: Vec<(&str, &str)> = vec![
        ("Make", "EPSON"),
        ("Model", "FastFoto FF-680W"),
        ("Software", "EPSON FastFoto"),
        ("DateTime", "2024:06:15 14:30:00"),
    ];
    let back_path = dir.join("MixedBatch_0001_b.tif");
    fs::write(&back_path, create_tiff_with_exif(&tags)).expect("Failed to write mixed back file");
}

// Helper function to find a tag value
fn find_tag<'a>(tags: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    tags.iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
}

// Helper to create null-terminated string bytes
fn null_terminated(s: &str) -> Vec<u8> {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

// Build EXIF data for JPEG APP1 segment
fn build_exif_ifd(
    make: &str,
    model: &str,
    software: &str,
    datetime: &str,
    width: u32,
    height: u32,
    x_res: u32,
    y_res: u32,
) -> Vec<u8> {
    let mut buf = Vec::new();

    // EXIF header
    buf.extend_from_slice(b"Exif\x00\x00");

    // TIFF header (little-endian)
    let _tiff_start = buf.len();
    buf.extend_from_slice(&[0x49, 0x49]); // "II" little-endian
    buf.extend_from_slice(&[0x2A, 0x00]); // Magic 42
    buf.extend_from_slice(&[0x08, 0x00, 0x00, 0x00]); // IFD0 offset

    // Prepare string data
    let make_bytes = null_terminated(make);
    let model_bytes = null_terminated(model);
    let software_bytes = null_terminated(software);
    let datetime_bytes = null_terminated(datetime);

    // Calculate IFD structure
    // We'll use 10 entries
    let num_entries: u16 = 10;
    let ifd_entry_size = 12;
    let ifd_header_size = 2;
    let next_ifd_size = 4;
    let ifd_size = ifd_header_size + (num_entries as usize * ifd_entry_size) + next_ifd_size;

    // Data starts after IFD (offset from TIFF header)
    let data_start = 8 + ifd_size;
    let mut data_offset = data_start as u32;

    // Pre-calculate offsets
    let make_off = data_offset;
    data_offset += make_bytes.len() as u32;

    let model_off = data_offset;
    data_offset += model_bytes.len() as u32;

    let software_off = data_offset;
    data_offset += software_bytes.len() as u32;

    let datetime_off = data_offset;
    data_offset += datetime_bytes.len() as u32;

    let xres_off = data_offset;
    data_offset += 8;

    let yres_off = data_offset;

    // Write IFD entry count
    buf.extend_from_slice(&num_entries.to_le_bytes());

    // IFD entries (must be in ascending tag order)
    fn write_entry(buf: &mut Vec<u8>, tag: u16, typ: u16, count: u32, val: u32) {
        buf.extend_from_slice(&tag.to_le_bytes());
        buf.extend_from_slice(&typ.to_le_bytes());
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&val.to_le_bytes());
    }

    const TYPE_ASCII: u16 = 2;
    const TYPE_SHORT: u16 = 3;
    const TYPE_LONG: u16 = 4;
    const TYPE_RATIONAL: u16 = 5;

    // Tags sorted by number
    write_entry(&mut buf, 256, TYPE_LONG, 1, width); // ImageWidth
    write_entry(&mut buf, 257, TYPE_LONG, 1, height); // ImageLength
    write_entry(&mut buf, 271, TYPE_ASCII, make_bytes.len() as u32, make_off); // Make
    write_entry(&mut buf, 272, TYPE_ASCII, model_bytes.len() as u32, model_off); // Model
    write_entry(&mut buf, 274, TYPE_SHORT, 1, 1); // Orientation (normal)
    write_entry(&mut buf, 282, TYPE_RATIONAL, 1, xres_off); // XResolution
    write_entry(&mut buf, 283, TYPE_RATIONAL, 1, yres_off); // YResolution
    write_entry(&mut buf, 296, TYPE_SHORT, 1, 2); // ResolutionUnit (inch)
    write_entry(&mut buf, 305, TYPE_ASCII, software_bytes.len() as u32, software_off); // Software
    write_entry(&mut buf, 306, TYPE_ASCII, datetime_bytes.len() as u32, datetime_off); // DateTime

    // Next IFD pointer (0 = none)
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    // Data area
    buf.extend_from_slice(&make_bytes);
    buf.extend_from_slice(&model_bytes);
    buf.extend_from_slice(&software_bytes);
    buf.extend_from_slice(&datetime_bytes);

    // XResolution RATIONAL
    buf.extend_from_slice(&x_res.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());

    // YResolution RATIONAL
    buf.extend_from_slice(&y_res.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_jpeg_with_exif() {
        let data = create_jpeg_with_exif(&[]);

        // Verify JPEG structure
        assert_eq!(&data[0..2], &[0xFF, 0xD8], "Should start with SOI");
        assert!(data.len() < 5000, "Should be under 5KB");

        // Verify it ends with EOI
        let len = data.len();
        assert_eq!(&data[len - 2..], &[0xFF, 0xD9], "Should end with EOI");
    }

    #[test]
    fn test_create_tiff_with_exif() {
        let data = create_tiff_with_exif(&[]);

        // Verify TIFF header
        assert_eq!(&data[0..2], &[0x49, 0x49], "Should be little-endian");
        assert_eq!(&data[2..4], &[0x2A, 0x00], "Should have magic 42");
        assert!(data.len() < 10000, "Should be reasonably small");
    }

    #[test]
    fn test_create_fastfoto_stack() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths = create_fastfoto_stack(tmp.path(), "Test", 1, true, true, "jpg");

        assert_eq!(paths.len(), 3);
        assert!(paths[0].exists());
        assert!(paths[1].exists());
        assert!(paths[2].exists());

        // Verify naming
        assert!(paths[0].file_name().unwrap().to_str().unwrap().contains("Test_0001.jpg"));
        assert!(paths[1].file_name().unwrap().to_str().unwrap().contains("Test_0001_a.jpg"));
        assert!(paths[2].file_name().unwrap().to_str().unwrap().contains("Test_0001_b.jpg"));
    }

    #[test]
    fn test_create_test_repository() {
        let tmp = tempfile::TempDir::new().unwrap();
        create_test_repository(tmp.path());

        // Check expected files exist
        assert!(tmp.path().join("FamilyPhotos_0001.jpg").exists());
        assert!(tmp.path().join("FamilyPhotos_0001_a.jpg").exists());
        assert!(tmp.path().join("FamilyPhotos_0001_b.jpg").exists());
        assert!(tmp.path().join("FamilyPhotos_0002.jpg").exists());
        assert!(tmp.path().join("FamilyPhotos_0002_a.jpg").exists());
        assert!(tmp.path().join("FamilyPhotos_0003.tif").exists());
        assert!(tmp.path().join("FamilyPhotos_0004.jpg").exists());
        assert!(tmp.path().join("MixedBatch_0001.jpg").exists());
        assert!(tmp.path().join("MixedBatch_0001_b.tif").exists());
    }
}
