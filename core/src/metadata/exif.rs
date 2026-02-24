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
/// Returns a map of tag name → display value for all tags present in the file.
/// Non-JPEG/TIFF or files without EXIF data return an empty map (not an error).
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
#[derive(Debug, thiserror::Error)]
pub enum ExifError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
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

        let tmp = tempfile::Builder::new()
            .suffix(".tif")
            .tempfile()
            .unwrap();
        std::fs::write(tmp.path(), &tiff_data).unwrap();

        let result = read_exif_tags(tmp.path());
        // kamadak-exif should recognize this as a TIFF container
        // and return an empty map (no EXIF tags in this minimal file)
        assert!(result.is_ok());
    }
}
