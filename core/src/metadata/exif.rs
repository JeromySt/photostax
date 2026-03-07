//! EXIF metadata reading from JPEG and TIFF images.
//!
//! This module provides functions to extract EXIF (Exchangeable Image File Format)
//! metadata from image files. EXIF data contains camera/scanner settings, timestamps,
//! and other technical information embedded in image files.
//!
//! ## Standard Tags Extracted
//!
//! The [`read_exif_tags`] function extracts these commonly-used tags:
//!
//! | Tag | Description |
//! |-----|-------------|
//! | `ImageDescription` | Description of the image |
//! | `Make` | Camera/scanner manufacturer |
//! | `Model` | Camera/scanner model |
//! | `Orientation` | Image orientation |
//! | `DateTime` | File modification time |
//! | `DateTimeOriginal` | Original capture time |
//! | `DateTimeDigitized` | Digitization time |
//! | `ExposureTime` | Exposure duration |
//! | `FNumber` | F-stop number |
//! | `ISOSpeed` | ISO sensitivity |
//! | `FocalLength` | Lens focal length |
//! | `ImageWidth` | Image width in pixels |
//! | `ImageLength` | Image height in pixels |
//! | `Artist` | Image creator |
//! | `Copyright` | Copyright notice |
//! | `GPSLatitude` | GPS latitude |
//! | `GPSLongitude` | GPS longitude |
//! | `XResolution` | Horizontal resolution |
//! | `YResolution` | Vertical resolution |
//! | `Software` | Software used |
//!
//! ## Examples
//!
//! ```rust,no_run
//! use photostax_core::metadata::exif::read_exif_tags;
//! use std::path::Path;
//!
//! let tags = read_exif_tags(Path::new("photo.jpg"))?;
//! if let Some(make) = tags.get("Make") {
//!     println!("Scanned with: {}", make);
//! }
//! # Ok::<(), photostax_core::metadata::exif::ExifError>(())
//! ```

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use exif::{In, Tag};

/// Well-known EXIF tags we extract by default.
const STANDARD_TAGS: &[(Tag, &str)] = &[
    (Tag::ImageDescription, "ImageDescription"),
    (Tag::Make, "Make"),
    (Tag::Model, "Model"),
    (Tag::Orientation, "Orientation"),
    (Tag::DateTime, "DateTime"),
    (Tag::DateTimeOriginal, "DateTimeOriginal"),
    (Tag::DateTimeDigitized, "DateTimeDigitized"),
    (Tag::ExposureTime, "ExposureTime"),
    (Tag::FNumber, "FNumber"),
    (Tag::ISOSpeed, "ISOSpeed"),
    (Tag::FocalLength, "FocalLength"),
    (Tag::ImageWidth, "ImageWidth"),
    (Tag::ImageLength, "ImageLength"),
    (Tag::Artist, "Artist"),
    (Tag::Copyright, "Copyright"),
    (Tag::GPSLatitude, "GPSLatitude"),
    (Tag::GPSLatitudeRef, "GPSLatitudeRef"),
    (Tag::GPSLongitude, "GPSLongitude"),
    (Tag::GPSLongitudeRef, "GPSLongitudeRef"),
    (Tag::XResolution, "XResolution"),
    (Tag::YResolution, "YResolution"),
    (Tag::Software, "Software"),
];

/// Read standard EXIF tags from a JPEG or TIFF file.
///
/// Returns a map of tag name → display value for commonly-used EXIF fields.
/// Files without EXIF data (or non-image files) return an empty map rather
/// than an error, making this function safe to call on any file.
///
/// # Arguments
///
/// * `path` - Path to a JPEG or TIFF image file
///
/// # Returns
///
/// A `HashMap` mapping tag names (e.g., `"Make"`, `"DateTime"`) to their
/// string values.
///
/// # Errors
///
/// - [`ExifError::Io`] if the file cannot be read
/// - [`ExifError::Parse`] if the EXIF data is malformed
///
/// # Examples
///
/// ```rust,no_run
/// use photostax_core::metadata::exif::read_exif_tags;
/// use std::path::Path;
///
/// let tags = read_exif_tags(Path::new("photo.jpg"))?;
/// println!("Found {} EXIF tags", tags.len());
///
/// if let Some(date) = tags.get("DateTimeOriginal") {
///     println!("Photo taken: {}", date);
/// }
/// # Ok::<(), photostax_core::metadata::exif::ExifError>(())
/// ```
pub fn read_exif_tags(path: &Path) -> Result<HashMap<String, String>, ExifError> {
    let file = File::open(path).map_err(ExifError::Io)?;
    let mut reader = BufReader::new(file);

    let exif_data = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(data) => data,
        Err(exif::Error::NotFound(_) | exif::Error::InvalidFormat(_)) => {
            return Ok(HashMap::new());
        }
        Err(e) => return Err(ExifError::Parse(e.to_string())),
    };

    let mut tags = HashMap::new();

    for &(tag, name) in STANDARD_TAGS {
        if let Some(field) = exif_data.get_field(tag, In::PRIMARY) {
            tags.insert(name.to_string(), field.display_value().to_string());
        }
    }

    Ok(tags)
}

/// Read all EXIF fields (not just standard ones) from a JPEG or TIFF file.
///
/// Unlike [`read_exif_tags`], this function returns every EXIF field found,
/// including vendor-specific and obscure tags.
///
/// # Arguments
///
/// * `path` - Path to a JPEG or TIFF image file
///
/// # Returns
///
/// A `HashMap` mapping tag names to their string values.
///
/// # Errors
///
/// Same as [`read_exif_tags`].
///
/// # Examples
///
/// ```rust,no_run
/// use photostax_core::metadata::exif::read_all_exif_tags;
/// use std::path::Path;
///
/// let all_tags = read_all_exif_tags(Path::new("photo.jpg"))?;
/// for (tag, value) in &all_tags {
///     println!("{}: {}", tag, value);
/// }
/// # Ok::<(), photostax_core::metadata::exif::ExifError>(())
/// ```
pub fn read_all_exif_tags(path: &Path) -> Result<HashMap<String, String>, ExifError> {
    let file = File::open(path).map_err(ExifError::Io)?;
    let mut reader = BufReader::new(file);

    let exif_data = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(data) => data,
        Err(exif::Error::NotFound(_) | exif::Error::InvalidFormat(_)) => {
            return Ok(HashMap::new());
        }
        Err(e) => return Err(ExifError::Parse(e.to_string())),
    };

    let mut tags = HashMap::new();
    for field in exif_data.fields() {
        let name = field.tag.to_string();
        let value = field.display_value().to_string();
        tags.insert(name, value);
    }

    Ok(tags)
}

/// Errors from EXIF operations.
///
/// # Variants
///
/// | Variant | When It Occurs |
/// |---------|----------------|
/// | [`Io`](Self::Io) | File cannot be opened or read |
/// | [`Parse`](Self::Parse) | EXIF data is present but malformed |
#[derive(Debug, thiserror::Error)]
pub enum ExifError {
    /// An I/O error occurred while reading the file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The EXIF data could not be parsed.
    ///
    /// This typically indicates corrupted or non-standard EXIF data.
    #[error("EXIF parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_read_exif_nonexistent_file() {
        let result = read_exif_tags(&PathBuf::from("nonexistent.jpg"));
        assert!(result.is_err());
    }

    #[test]
    fn test_read_exif_non_jpeg_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"not a jpeg file").unwrap();
        let result = read_exif_tags(tmp.path());
        // Should return Ok with empty map for non-JPEG/TIFF
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_read_exif_from_tiff_container() {
        // Create a minimal valid TIFF file with no EXIF data
        // TIFF header: little-endian (II), magic 42, first IFD offset
        let mut tiff_data = Vec::new();
        // Byte order: little-endian "II"
        tiff_data.extend_from_slice(b"II");
        // Magic number: 42
        tiff_data.extend_from_slice(&42u16.to_le_bytes());
        // Offset to first IFD: 8 (right after header)
        tiff_data.extend_from_slice(&8u32.to_le_bytes());
        // IFD entry count: 0 (empty IFD for minimal file)
        tiff_data.extend_from_slice(&0u16.to_le_bytes());
        // Next IFD offset: 0 (no more IFDs)
        tiff_data.extend_from_slice(&0u32.to_le_bytes());

        let tmp = tempfile::Builder::new().suffix(".tif").tempfile().unwrap();
        std::fs::write(tmp.path(), &tiff_data).unwrap();

        let result = read_exif_tags(tmp.path());
        // kamadak-exif should recognize this as a TIFF container
        // and return an empty map (no EXIF tags in this minimal file)
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_all_exif_tags_nonexistent() {
        let result = read_all_exif_tags(&PathBuf::from("nonexistent.jpg"));
        assert!(result.is_err());
    }

    #[test]
    fn test_read_all_exif_tags_non_image() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"not an image").unwrap();
        let result = read_all_exif_tags(tmp.path());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_exif_error_display() {
        let io_err = ExifError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found",
        ));
        let display = format!("{}", io_err);
        assert!(display.contains("I/O error"));

        let parse_err = ExifError::Parse("invalid format".to_string());
        let display2 = format!("{}", parse_err);
        assert!(display2.contains("EXIF parse error"));
    }

    #[test]
    fn test_exif_error_debug() {
        let err = ExifError::Parse("test error".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Parse"));
    }
}
