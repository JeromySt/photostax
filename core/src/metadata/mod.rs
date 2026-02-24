//! Image format detection and metadata sub-modules.
//!
//! This module provides utilities for working with image metadata:
//!
//! - [`exif`] — Read EXIF tags from JPEG and TIFF files
//! - [`xmp`] — Read and write XMP/Dublin Core metadata
//! - [`sidecar`] — SQLite sidecar database for custom metadata
//!
//! ## Supported Formats
//!
//! | Format | Extensions | EXIF | XMP |
//! |--------|------------|------|-----|
//! | JPEG | `.jpg`, `.jpeg` | ✓ Embedded | ✓ Embedded |
//! | TIFF | `.tif`, `.tiff` | ✓ Embedded | Sidecar `.xmp` |

use std::ffi::OsStr;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub mod exif;
pub mod sidecar;
pub mod xmp;

/// Supported image formats for photo stacks.
///
/// Both JPEG and TIFF are commonly used by Epson FastFoto scanners.
/// The format is typically detected from file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    /// JPEG format (`.jpg`, `.jpeg`).
    ///
    /// Lossy compression, widely compatible, supports embedded EXIF and XMP.
    Jpeg,

    /// TIFF format (`.tif`, `.tiff`).
    ///
    /// Lossless compression, higher quality, supports embedded EXIF.
    /// XMP is stored in sidecar `.xmp` files for TIFF images.
    Tiff,
}

/// Detects the image format from a file path's extension.
///
/// Returns `None` if the extension is not a recognized image format.
/// Detection is case-insensitive.
///
/// # Examples
///
/// ```
/// use photostax_core::metadata::{detect_image_format, ImageFormat};
/// use std::path::Path;
///
/// assert_eq!(detect_image_format(Path::new("photo.jpg")), Some(ImageFormat::Jpeg));
/// assert_eq!(detect_image_format(Path::new("photo.TIF")), Some(ImageFormat::Tiff));
/// assert_eq!(detect_image_format(Path::new("photo.png")), None);
/// ```
pub fn detect_image_format(path: &Path) -> Option<ImageFormat> {
    let ext = path.extension().and_then(OsStr::to_str)?.to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_image_format_jpeg() {
        assert_eq!(
            detect_image_format(&PathBuf::from("photo.jpg")),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(
            detect_image_format(&PathBuf::from("photo.jpeg")),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(
            detect_image_format(&PathBuf::from("photo.JPG")),
            Some(ImageFormat::Jpeg)
        );
    }

    #[test]
    fn test_detect_image_format_tiff() {
        assert_eq!(
            detect_image_format(&PathBuf::from("photo.tif")),
            Some(ImageFormat::Tiff)
        );
        assert_eq!(
            detect_image_format(&PathBuf::from("photo.tiff")),
            Some(ImageFormat::Tiff)
        );
        assert_eq!(
            detect_image_format(&PathBuf::from("photo.TIF")),
            Some(ImageFormat::Tiff)
        );
    }

    #[test]
    fn test_detect_image_format_unknown() {
        assert_eq!(detect_image_format(&PathBuf::from("photo.png")), None);
        assert_eq!(detect_image_format(&PathBuf::from("photo.bmp")), None);
        assert_eq!(detect_image_format(&PathBuf::from("photo")), None);
    }
}
