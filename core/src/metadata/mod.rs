use std::ffi::OsStr;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub mod exif;
pub mod sidecar;
pub mod xmp;

/// Supported image formats for photo stacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    /// JPEG format (.jpg, .jpeg)
    Jpeg,
    /// TIFF format (.tif, .tiff)
    Tiff,
}

/// Detects the image format from a file path's extension.
///
/// Returns `None` if the extension is not a recognized image format.
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
