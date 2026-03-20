//! Image classifier trait for determining if an ambiguous `_a` scan is
//! an enhanced front or a back-of-photo image.
//!
//! The [`ImageClassifier`] trait is dependency-injected: owned by
//! [`SessionManager`](crate::stack_manager::StackManager), shared to
//! repositories via `Arc<dyn ImageClassifier>` at registration time.
//!
//! The [`DefaultClassifier`] wraps the existing pixel-analysis logic from
//! [`classify`](crate::classify) to determine classification from a byte
//! stream.

use std::io::Read;

use crate::repository::RepositoryError;

/// Result of classifying an ambiguous `_a` image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    /// The image is an enhanced (color-corrected) front scan.
    Enhanced,
    /// The image is a back-of-photo scan.
    Back,
}

impl Classification {
    /// Convert from FFI integer: 0 = Enhanced, 1 = Back.
    pub fn from_int(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Enhanced),
            1 => Some(Self::Back),
            _ => None,
        }
    }

    /// Convert to FFI integer: 0 = Enhanced, 1 = Back.
    pub fn as_int(&self) -> i32 {
        match self {
            Self::Enhanced => 0,
            Self::Back => 1,
        }
    }
}

/// Trait for classifying ambiguous `_a` images as enhanced-front or back.
///
/// Injected into repositories by [`SessionManager`](crate::stack_manager::StackManager)
/// at registration time. All repos in a session share the same classifier
/// instance via `Arc<dyn ImageClassifier>`, ensuring consistent behaviour.
///
/// # Implementors
///
/// - [`DefaultClassifier`] — pixel-analysis (current built-in logic)
/// - Foreign classifiers from .NET/TypeScript bindings (ML-based, etc.)
pub trait ImageClassifier: Send + Sync {
    /// Classify an image from its byte stream.
    ///
    /// The reader provides the full image data. Implementations may read
    /// as much as needed (e.g., decode the image for pixel analysis, or
    /// run an ML model on a subset of bytes).
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Io`] if the stream cannot be read, or
    /// [`RepositoryError::Other`] if classification fails.
    fn classify(&self, reader: &mut dyn Read) -> Result<Classification, RepositoryError>;
}

/// Default classifier using pixel-analysis heuristics.
///
/// Delegates to [`crate::classify::is_back_of_photo`] which decodes the
/// image and checks pixel variance / mean brightness to determine if it
/// is a mostly-blank back scan.
pub struct DefaultClassifier;

impl ImageClassifier for DefaultClassifier {
    fn classify(&self, reader: &mut dyn Read) -> Result<Classification, RepositoryError> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).map_err(RepositoryError::Io)?;

        // Decode the image to check pixel statistics
        let img = image::load_from_memory(&buf)
            .map_err(|e| RepositoryError::Other(format!("image decode error: {e}")))?;

        let gray = img.to_luma8();
        let pixels: Vec<f64> = gray.pixels().map(|p| p.0[0] as f64).collect();

        if pixels.is_empty() {
            return Ok(Classification::Enhanced);
        }

        let mean = pixels.iter().sum::<f64>() / pixels.len() as f64;
        let variance = pixels.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / pixels.len() as f64;
        let std_dev = variance.sqrt();

        // Low variance + high brightness = likely a blank/mostly-white back
        if std_dev < 25.0 && mean > 180.0 {
            Ok(Classification::Back)
        } else {
            Ok(Classification::Enhanced)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_classification_from_int() {
        assert_eq!(Classification::from_int(0), Some(Classification::Enhanced));
        assert_eq!(Classification::from_int(1), Some(Classification::Back));
        assert_eq!(Classification::from_int(2), None);
        assert_eq!(Classification::from_int(-1), None);
    }

    #[test]
    fn test_classification_as_int() {
        assert_eq!(Classification::Enhanced.as_int(), 0);
        assert_eq!(Classification::Back.as_int(), 1);
    }

    #[test]
    fn test_default_classifier_white_image_is_back() {
        // Create a small all-white JPEG-like image
        let img = image::ImageBuffer::from_pixel(10, 10, image::Luma([255u8]));
        let mut buf = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new(&mut buf);
        img.write_with_encoder(encoder).unwrap();

        let mut reader = Cursor::new(buf);
        let result = DefaultClassifier.classify(&mut reader).unwrap();
        assert_eq!(result, Classification::Back);
    }

    #[test]
    fn test_default_classifier_varied_image_is_enhanced() {
        // Create a small image with high variance (gradient)
        let img = image::ImageBuffer::from_fn(100, 100, |x, _y| {
            image::Luma([(x as f32 / 100.0 * 255.0) as u8])
        });
        let mut buf = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new(&mut buf);
        img.write_with_encoder(encoder).unwrap();

        let mut reader = Cursor::new(buf);
        let result = DefaultClassifier.classify(&mut reader).unwrap();
        assert_eq!(result, Classification::Enhanced);
    }

    #[test]
    fn test_default_classifier_invalid_data() {
        let mut reader = Cursor::new(b"not an image");
        let result = DefaultClassifier.classify(&mut reader);
        assert!(result.is_err());
    }

    struct AlwaysBackClassifier;
    impl ImageClassifier for AlwaysBackClassifier {
        fn classify(&self, _reader: &mut dyn Read) -> Result<Classification, RepositoryError> {
            Ok(Classification::Back)
        }
    }

    #[test]
    fn test_custom_classifier() {
        let classifier = AlwaysBackClassifier;
        let mut reader = Cursor::new(Vec::<u8>::new());
        let result = classifier.classify(&mut reader).unwrap();
        assert_eq!(result, Classification::Back);
    }
}
