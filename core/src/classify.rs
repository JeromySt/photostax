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

use std::path::Path;

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

    let gray = img.into_luma8();
    let (width, height) = gray.dimensions();

    if width == 0 || height == 0 {
        return Ok(false);
    }

    // Images below a minimum resolution are not real scans — skip classification.
    const MIN_DIMENSION: u32 = 32;
    if width < MIN_DIMENSION || height < MIN_DIMENSION {
        return Ok(false);
    }

    // Sample ~10 000 pixels evenly across the image for efficiency.
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
/// If the stack has an `enhanced` path but no `back` path, the `_a` image is
/// analysed. When it looks like a back-of-photo scan, the path is moved from
/// `enhanced` to `back`.
///
/// Returns `true` if the stack was reclassified.
///
/// Stacks that already have a `back` path, or that have no `enhanced` path,
/// are left unchanged. If the image cannot be decoded the stack is left
/// unchanged (classification is best-effort).
pub fn classify_ambiguous(stack: &mut PhotoStack) -> Result<bool, RepositoryError> {
    // Only ambiguous when enhanced exists but back does not.
    if stack.enhanced.is_none() || stack.back.is_some() {
        return Ok(false);
    }

    let path = Path::new(&stack.enhanced.as_ref().unwrap().path);
    match is_likely_back(path) {
        Ok(true) => {
            stack.back = stack.enhanced.take();
            Ok(true)
        }
        Ok(false) => Ok(false),
        // Image couldn't be decoded — leave classification as-is.
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashing::ImageFile;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn img(path: PathBuf) -> ImageFile {
        ImageFile::new(path.to_string_lossy(), 0)
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

        let mut stack = PhotoStack::new("IMG_001");
        stack.original = Some(img(tmp.path().join("IMG_001.jpg")));
        stack.enhanced = Some(img(back_path.clone()));

        assert!(classify_ambiguous(&mut stack).unwrap());
        assert!(stack.enhanced.is_none());
        assert_eq!(
            stack.back.as_ref().unwrap().path,
            back_path.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn test_classify_ambiguous_keeps_enhanced() {
        let tmp = TempDir::new().unwrap();
        let front_path = create_colourful_jpeg(tmp.path(), "IMG_002_a.jpg");

        let mut stack = PhotoStack::new("IMG_002");
        stack.original = Some(img(tmp.path().join("IMG_002.jpg")));
        stack.enhanced = Some(img(front_path.clone()));

        assert!(!classify_ambiguous(&mut stack).unwrap());
        assert_eq!(
            stack.enhanced.as_ref().unwrap().path,
            front_path.to_string_lossy().as_ref()
        );
        assert!(stack.back.is_none());
    }

    #[test]
    fn test_classify_ambiguous_noop_when_back_exists() {
        let tmp = TempDir::new().unwrap();
        let enhanced = create_white_jpeg(tmp.path(), "IMG_003_a.jpg");

        let mut stack = PhotoStack::new("IMG_003");
        stack.enhanced = Some(img(enhanced.clone()));
        stack.back = Some(img(tmp.path().join("IMG_003_b.jpg")));

        assert!(!classify_ambiguous(&mut stack).unwrap());
        assert_eq!(
            stack.enhanced.as_ref().unwrap().path,
            enhanced.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn test_classify_ambiguous_noop_when_no_enhanced() {
        let mut stack = PhotoStack::new("IMG_004");
        stack.original = Some(img(PathBuf::from("/photos/IMG_004.jpg")));

        assert!(!classify_ambiguous(&mut stack).unwrap());
    }
}
