//! Image classification for ambiguous FastFoto `_a` scans.
//!
//! When the FastFoto scanner produces only an original and an `_a` image (no
//! `_b` file), the `_a` file might be either an enhanced front scan or a
//! back-of-photo scan. This module provides pixel-level analysis to distinguish
//! the two cases.
//!
//! **Back-of-photo** images are characterised by:
//! - High mean brightness (white/cream paper)
//! - Low pixel variance (mostly uniform colour)
//!
//! **Enhanced front** images have a wide variety of pixel intensities typical
//! of an actual photograph.

use std::io::Read;
use std::path::Path;

use crate::image_handle::ImageRef;
use crate::photo_stack::PhotoStack;
use crate::repository::RepositoryError;

/// Grayscale standard-deviation threshold below which an image is classified as
/// a back-of-photo scan.
const BACK_STD_DEV_THRESHOLD: f64 = 25.0;

/// Minimum mean brightness (0–255 grayscale) expected for a back-of-photo scan.
const BACK_MIN_BRIGHTNESS: f64 = 180.0;

/// Analyse an image and return `true` if it is likely a back-of-photo scan.
///
/// The function loads the image, converts it to 8-bit grayscale, samples
/// pixels for efficiency, and checks whether the standard deviation is below
/// [`BACK_STD_DEV_THRESHOLD`] **and** the mean brightness is above
/// [`BACK_MIN_BRIGHTNESS`].
///
/// # Errors
///
/// Returns [`RepositoryError::Other`] if the image cannot be decoded.
pub fn is_likely_back(path: &Path) -> Result<bool, RepositoryError> {
    let img = image::open(path).map_err(|e| {
        RepositoryError::Other(format!(
            "Failed to open image for classification {}: {e}",
            path.display()
        ))
    })?;

    classify_image(img)
}

/// Analyse in-memory image data and return `true` if it is likely a back-of-photo scan.
fn is_likely_back_from_bytes(data: &[u8]) -> Result<bool, RepositoryError> {
    let img = image::load_from_memory(data).map_err(|e| {
        RepositoryError::Other(format!("Failed to decode image for classification: {e}"))
    })?;

    classify_image(img)
}

/// Core classification logic on a decoded image.
fn classify_image(img: image::DynamicImage) -> Result<bool, RepositoryError> {
    let gray = img.into_luma8();
    let (width, height) = gray.dimensions();

    if width == 0 || height == 0 {
        return Ok(false);
    }

    const MIN_DIMENSION: u32 = 32;
    if width < MIN_DIMENSION || height < MIN_DIMENSION {
        return Ok(false);
    }

    let total = (width as usize) * (height as usize);
    let stride = std::cmp::max(1, total / 10_000);
    let pixels: Vec<f64> = gray
        .as_raw()
        .iter()
        .step_by(stride)
        .map(|&p| p as f64)
        .collect();

    if pixels.is_empty() {
        return Ok(false);
    }

    let n = pixels.len() as f64;
    let mean = pixels.iter().sum::<f64>() / n;
    let variance = pixels.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();

    Ok(std_dev < BACK_STD_DEV_THRESHOLD && mean > BACK_MIN_BRIGHTNESS)
}

/// Classify an ambiguous `_a` image in a [`PhotoStack`].
///
/// If the stack has an `enhanced` variant but no `back` variant, the `_a` image
/// is analysed. When it looks like a back-of-photo scan, the handle is moved
/// from `enhanced` to `back`.
///
/// Returns `true` if the stack was reclassified.
///
/// Stacks that already have a `back` variant, or that have no `enhanced` variant,
/// are left unchanged. If the image cannot be decoded the stack is left
/// unchanged (classification is best-effort).
pub fn classify_ambiguous(stack: &mut PhotoStack) -> Result<bool, RepositoryError> {
    let mut inner = stack.inner.write().unwrap();
    if !inner.enhanced.is_present() || inner.back.is_present() {
        return Ok(false);
    }

    // Read image data through the handle
    let mut reader = match inner.enhanced.read() {
        Ok(r) => r,
        Err(_) => return Ok(false),
    };
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| RepositoryError::Other(format!("Failed to read enhanced image: {e}")))?;

    match is_likely_back_from_bytes(&buf) {
        Ok(true) => {
            let enhanced = std::mem::replace(&mut inner.enhanced, ImageRef::absent());
            inner.back = enhanced;
            Ok(true)
        }
        Ok(false) => Ok(false),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::backends::local_handles::LocalImageHandle;

    fn local_ref(path: &Path) -> ImageRef {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        ImageRef::new(Arc::new(LocalImageHandle::new(path, size)))
    }

    /// Create a small solid-white JPEG (back-of-photo like).
    fn create_white_jpeg(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let img = image::RgbImage::from_fn(64, 64, |_, _| image::Rgb([250, 248, 245]));
        img.save(&path).unwrap();
        path
    }

    /// Create a small colourful JPEG (front photo like).
    fn create_colourful_jpeg(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let img = image::RgbImage::from_fn(64, 64, |x, y| {
            image::Rgb([(x * 4) as u8, (y * 4) as u8, ((x + y) * 2) as u8])
        });
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn test_is_likely_back_white_image() {
        let tmp = TempDir::new().unwrap();
        let path = create_white_jpeg(tmp.path(), "back.jpg");
        assert!(is_likely_back(&path).unwrap());
    }

    #[test]
    fn test_is_likely_back_colourful_image() {
        let tmp = TempDir::new().unwrap();
        let path = create_colourful_jpeg(tmp.path(), "front.jpg");
        assert!(!is_likely_back(&path).unwrap());
    }

    #[test]
    fn test_classify_ambiguous_reclassifies_back() {
        let tmp = TempDir::new().unwrap();
        let back_path = create_white_jpeg(tmp.path(), "IMG_001_a.jpg");
        let orig_path = create_colourful_jpeg(tmp.path(), "IMG_001.jpg");

        let mut stack = PhotoStack::new("IMG_001");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = local_ref(&orig_path);
            inner.enhanced = local_ref(&back_path);
        }

        assert!(classify_ambiguous(&mut stack).unwrap());
        let inner = stack.inner.read().unwrap();
        assert!(!inner.enhanced.is_present());
        assert!(inner.back.is_present());
    }

    #[test]
    fn test_classify_ambiguous_keeps_enhanced() {
        let tmp = TempDir::new().unwrap();
        let front_path = create_colourful_jpeg(tmp.path(), "IMG_002_a.jpg");
        let orig_path = create_colourful_jpeg(tmp.path(), "IMG_002.jpg");

        let mut stack = PhotoStack::new("IMG_002");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = local_ref(&orig_path);
            inner.enhanced = local_ref(&front_path);
        }

        assert!(!classify_ambiguous(&mut stack).unwrap());
        let inner = stack.inner.read().unwrap();
        assert!(inner.enhanced.is_present());
        assert!(!inner.back.is_present());
    }

    #[test]
    fn test_classify_ambiguous_noop_when_back_exists() {
        let tmp = TempDir::new().unwrap();
        let enhanced = create_white_jpeg(tmp.path(), "IMG_003_a.jpg");
        let back = create_colourful_jpeg(tmp.path(), "IMG_003_b.jpg");

        let mut stack = PhotoStack::new("IMG_003");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.enhanced = local_ref(&enhanced);
            inner.back = local_ref(&back);
        }

        assert!(!classify_ambiguous(&mut stack).unwrap());
        let inner = stack.inner.read().unwrap();
        assert!(inner.enhanced.is_present());
    }

    #[test]
    fn test_classify_ambiguous_noop_when_no_enhanced() {
        let tmp = TempDir::new().unwrap();
        let orig = create_colourful_jpeg(tmp.path(), "IMG_004.jpg");

        let mut stack = PhotoStack::new("IMG_004");
        {
            let mut inner = stack.inner.write().unwrap();
            inner.original = local_ref(&orig);
        }

        assert!(!classify_ambiguous(&mut stack).unwrap());
    }
}
