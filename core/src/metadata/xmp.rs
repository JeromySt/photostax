//! XMP metadata reading and writing for image files.
//!
//! XMP (Extensible Metadata Platform) is an ISO standard (ISO 16684-1:2012) for
//! embedding metadata in files. This module provides support for reading and
//! writing XMP data in JPEG and TIFF files, using the Dublin Core namespace
//! for standard fields and a custom photostax namespace for application-specific data.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use img_parts::jpeg::{markers, Jpeg, JpegSegment};

use super::{detect_image_format, ImageFormat};

/// XMP namespace URI for Adobe XMP meta wrapper.
const XMP_NS_X: &str = "adobe:ns:meta/";
/// XMP namespace URI for RDF.
const XMP_NS_RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
/// Dublin Core namespace URI.
const XMP_NS_DC: &str = "http://purl.org/dc/elements/1.1/";
/// Photostax custom namespace URI.
const XMP_NS_PHOTOSTAX: &str = "http://github.com/JeromySt/photostax/ns/1.0/";

/// Header identifying an XMP APP1 segment in JPEG files.
const XMP_APP1_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// Errors from XMP operations.
#[derive(Debug, thiserror::Error)]
pub enum XmpError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image parsing error: {0}")]
    ImageParse(String),
    #[error("XMP parsing error: {0}")]
    XmpParse(String),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}

/// Write XMP metadata to a file, detecting format from extension.
///
/// For JPEG files, writes XMP into an APP1 segment.
/// For TIFF files, writes to a `.xmp` sidecar file (direct TIFF modification is complex).
pub fn write_xmp(path: &Path, metadata: &HashMap<String, String>) -> Result<(), XmpError> {
    match detect_image_format(path) {
        Some(ImageFormat::Jpeg) => write_xmp_to_jpeg(path, metadata),
        Some(ImageFormat::Tiff) => write_xmp_to_tiff(path, metadata),
        None => Err(XmpError::UnsupportedFormat(
            path.extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
        )),
    }
}

/// Read XMP metadata from a file, detecting format from extension.
pub fn read_xmp(path: &Path) -> Result<HashMap<String, String>, XmpError> {
    match detect_image_format(path) {
        Some(ImageFormat::Jpeg) => read_xmp_from_jpeg(path),
        Some(ImageFormat::Tiff) => read_xmp_from_tiff(path),
        None => Err(XmpError::UnsupportedFormat(
            path.extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
        )),
    }
}

/// Write XMP metadata into a JPEG file.
///
/// Reads the existing file, injects/updates the XMP APP1 segment, and writes back.
pub fn write_xmp_to_jpeg(path: &Path, metadata: &HashMap<String, String>) -> Result<(), XmpError> {
    let data = fs::read(path)?;
    let mut jpeg =
        Jpeg::from_bytes(data.into()).map_err(|e| XmpError::ImageParse(e.to_string()))?;

    let xmp_xml = build_xmp_xml(metadata);
    let xmp_segment_data = build_xmp_segment_data(&xmp_xml);

    // Remove any existing XMP segments
    // We need to find XMP segments (APP1 with XMP header) and remove them
    let segments = jpeg.segments_mut();
    segments.retain(|seg| !is_xmp_segment(seg));

    // Create new XMP segment
    let xmp_segment = JpegSegment::new_with_contents(markers::APP1, xmp_segment_data.into());

    // Find position to insert after JFIF/EXIF APP0/APP1 segments
    let insert_pos = jpeg
        .segments()
        .iter()
        .take_while(|seg| {
            seg.marker() == markers::APP0
                || (seg.marker() == markers::APP1 && !is_xmp_segment(seg))
        })
        .count();

    jpeg.segments_mut().insert(insert_pos, xmp_segment);

    // Write back to file
    let output = jpeg.encoder().bytes();
    fs::write(path, output)?;

    Ok(())
}

/// Write XMP metadata for a TIFF file.
///
/// Due to TIFF's complex structure, we write to a sidecar `.xmp` file instead of
/// modifying the TIFF directly. This is the same approach used by many photo apps.
pub fn write_xmp_to_tiff(path: &Path, metadata: &HashMap<String, String>) -> Result<(), XmpError> {
    let xmp_xml = build_xmp_xml(metadata);
    let sidecar_path = path.with_extension("xmp");
    fs::write(&sidecar_path, xmp_xml)?;
    Ok(())
}

/// Read XMP metadata from a JPEG file.
pub fn read_xmp_from_jpeg(path: &Path) -> Result<HashMap<String, String>, XmpError> {
    let data = fs::read(path)?;
    let jpeg = Jpeg::from_bytes(data.into()).map_err(|e| XmpError::ImageParse(e.to_string()))?;

    // Find XMP APP1 segment
    for segment in jpeg.segments() {
        if is_xmp_segment(segment) {
            let contents = segment.contents();
            // Skip the XMP header
            if contents.len() > XMP_APP1_HEADER.len() {
                let xmp_data = &contents[XMP_APP1_HEADER.len()..];
                let xmp_str = std::str::from_utf8(xmp_data)
                    .map_err(|e| XmpError::XmpParse(e.to_string()))?;
                return parse_xmp_xml(xmp_str);
            }
        }
    }

    // No XMP found
    Ok(HashMap::new())
}

/// Read XMP metadata from a TIFF file.
///
/// Tries to read from a sidecar `.xmp` file first, then attempts to read embedded XMP.
pub fn read_xmp_from_tiff(path: &Path) -> Result<HashMap<String, String>, XmpError> {
    // Try sidecar file first
    let sidecar_path = path.with_extension("xmp");
    if sidecar_path.exists() {
        let xmp_str = fs::read_to_string(&sidecar_path)?;
        return parse_xmp_xml(&xmp_str);
    }

    // Try to read embedded XMP from TIFF using img-parts EXIF support
    // Note: img-parts doesn't directly support TIFF, so we return empty for now
    // and rely on the sidecar approach for TIFFs
    Ok(HashMap::new())
}

/// Check if a JPEG segment is an XMP APP1 segment.
fn is_xmp_segment(segment: &JpegSegment) -> bool {
    segment.marker() == markers::APP1 && segment.contents().starts_with(XMP_APP1_HEADER)
}

/// Build XMP segment data with the required header.
fn build_xmp_segment_data(xmp_xml: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(XMP_APP1_HEADER.len() + xmp_xml.len());
    data.extend_from_slice(XMP_APP1_HEADER);
    data.extend_from_slice(xmp_xml.as_bytes());
    data
}

/// Build standards-compliant XMP XML from metadata.
///
/// Uses Dublin Core namespace for standard fields (dc:description, dc:creator, etc.)
/// and a custom photostax namespace for application-specific fields.
fn build_xmp_xml(metadata: &HashMap<String, String>) -> String {
    let mut xml = String::new();

    // XMP packet header (required for compatibility)
    xml.push_str("<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    xml.push_str(&format!(
        "<x:xmpmeta xmlns:x=\"{}\">\n",
        XMP_NS_X
    ));
    xml.push_str(&format!(
        "  <rdf:RDF xmlns:rdf=\"{}\">\n",
        XMP_NS_RDF
    ));
    xml.push_str(&format!(
        "    <rdf:Description rdf:about=\"\"\n      xmlns:dc=\"{}\"\n      xmlns:photostax=\"{}\">\n",
        XMP_NS_DC, XMP_NS_PHOTOSTAX
    ));

    // Sort keys for deterministic output
    let mut keys: Vec<_> = metadata.keys().collect();
    keys.sort();

    for key in keys {
        let value = &metadata[key];
        let escaped_value = escape_xml(value);

        // Map common fields to Dublin Core, rest to photostax namespace
        let (ns, tag) = map_key_to_namespace(key);
        xml.push_str(&format!(
            "      <{}:{}>{}</{}:{}>\n",
            ns, tag, escaped_value, ns, tag
        ));
    }

    xml.push_str("    </rdf:Description>\n");
    xml.push_str("  </rdf:RDF>\n");
    xml.push_str("</x:xmpmeta>\n");
    xml.push_str("<?xpacket end=\"w\"?>");

    xml
}

/// Map a metadata key to the appropriate XMP namespace and tag name.
fn map_key_to_namespace(key: &str) -> (&'static str, &str) {
    // Standard Dublin Core mappings
    match key.to_lowercase().as_str() {
        "description" | "imagedescription" => ("dc", "description"),
        "creator" | "artist" => ("dc", "creator"),
        "title" => ("dc", "title"),
        "subject" | "keywords" => ("dc", "subject"),
        "rights" | "copyright" => ("dc", "rights"),
        "date" | "datetime" | "datetimeoriginal" => ("dc", "date"),
        _ => ("photostax", key),
    }
}

/// Parse XMP XML and extract metadata as key-value pairs.
fn parse_xmp_xml(xml: &str) -> Result<HashMap<String, String>, XmpError> {
    let mut metadata = HashMap::new();

    // Simple XML parsing - find tags and extract content
    // This handles the XMP structure we generate

    // Parse Dublin Core fields
    for (dc_tag, key) in [
        ("description", "description"),
        ("creator", "creator"),
        ("title", "title"),
        ("subject", "subject"),
        ("rights", "rights"),
        ("date", "date"),
    ] {
        if let Some(value) = extract_tag_content(xml, "dc", dc_tag) {
            metadata.insert(key.to_string(), unescape_xml(&value));
        }
    }

    // Parse photostax namespace fields
    let photostax_prefix = "photostax:";
    let mut search_pos = 0;
    while let Some(start) = xml[search_pos..].find(&format!("<{}", photostax_prefix)) {
        let abs_start = search_pos + start;
        let tag_start = abs_start + 1 + photostax_prefix.len();

        // Find the end of the tag name
        if let Some(tag_end_rel) = xml[tag_start..].find('>') {
            let tag_end = tag_start + tag_end_rel;
            let tag_name = &xml[tag_start..tag_end];

            // Find the closing tag and extract content
            let close_tag = format!("</{}:{}>", "photostax", tag_name);
            if let Some(content_end) = xml[tag_end + 1..].find(&close_tag) {
                let content = &xml[tag_end + 1..tag_end + 1 + content_end];
                metadata.insert(tag_name.to_string(), unescape_xml(content));
                search_pos = tag_end + 1 + content_end + close_tag.len();
            } else {
                search_pos = tag_end + 1;
            }
        } else {
            break;
        }
    }

    Ok(metadata)
}

/// Extract content from an XML tag with namespace prefix.
fn extract_tag_content(xml: &str, ns: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}:{}>", ns, tag);
    let close_tag = format!("</{}:{}>", ns, tag);

    let start = xml.find(&open_tag)?;
    let content_start = start + open_tag.len();
    let content_end = xml[content_start..].find(&close_tag)?;

    Some(xml[content_start..content_start + content_end].to_string())
}

/// Escape special XML characters.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Unescape XML entities.
fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// Create a minimal valid JPEG for testing.
    fn create_test_jpeg() -> Vec<u8> {
        // Minimal JPEG: SOI, APP0 (JFIF), minimal scan data, EOI
        let mut jpeg = Vec::new();

        // SOI marker
        jpeg.extend_from_slice(&[0xFF, 0xD8]);

        // APP0 JFIF marker
        jpeg.extend_from_slice(&[0xFF, 0xE0]);
        let jfif_data = b"JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00";
        jpeg.extend_from_slice(&((jfif_data.len() + 2) as u16).to_be_bytes());
        jpeg.extend_from_slice(jfif_data);

        // DQT (Define Quantization Table) - minimal
        jpeg.extend_from_slice(&[0xFF, 0xDB]);
        jpeg.extend_from_slice(&[0x00, 0x43]); // Length
        jpeg.push(0x00); // Table ID
        jpeg.extend_from_slice(&[16u8; 64]); // 64 bytes of quantization values

        // SOF0 (Start of Frame) - minimal 1x1 image
        jpeg.extend_from_slice(&[0xFF, 0xC0]);
        jpeg.extend_from_slice(&[0x00, 0x0B]); // Length
        jpeg.push(0x08); // Precision
        jpeg.extend_from_slice(&[0x00, 0x01]); // Height
        jpeg.extend_from_slice(&[0x00, 0x01]); // Width
        jpeg.push(0x01); // Components
        jpeg.extend_from_slice(&[0x01, 0x11, 0x00]); // Component data

        // DHT (Define Huffman Table) - minimal DC table
        jpeg.extend_from_slice(&[0xFF, 0xC4]);
        jpeg.extend_from_slice(&[0x00, 0x1F]); // Length
        jpeg.push(0x00); // DC table 0
        jpeg.extend_from_slice(&[0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        jpeg.extend_from_slice(&[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B]);

        // SOS (Start of Scan)
        jpeg.extend_from_slice(&[0xFF, 0xDA]);
        jpeg.extend_from_slice(&[0x00, 0x08]); // Length
        jpeg.push(0x01); // Components
        jpeg.extend_from_slice(&[0x01, 0x00]); // Component selector
        jpeg.extend_from_slice(&[0x00, 0x3F, 0x00]); // Spectral selection

        // Minimal scan data
        jpeg.push(0x7F);

        // EOI marker
        jpeg.extend_from_slice(&[0xFF, 0xD9]);

        jpeg
    }

    #[test]
    fn test_build_xmp_xml_basic() {
        let mut metadata = HashMap::new();
        metadata.insert("description".to_string(), "Test photo".to_string());
        metadata.insert("stackId".to_string(), "Family_001".to_string());

        let xml = build_xmp_xml(&metadata);

        assert!(xml.contains("<?xpacket begin="));
        assert!(xml.contains("<x:xmpmeta"));
        assert!(xml.contains("<dc:description>Test photo</dc:description>"));
        assert!(xml.contains("<photostax:stackId>Family_001</photostax:stackId>"));
        assert!(xml.contains("<?xpacket end=\"w\"?>"));
    }

    #[test]
    fn test_build_xmp_xml_escaping() {
        let mut metadata = HashMap::new();
        metadata.insert("description".to_string(), "Tom & Jerry <friends>".to_string());

        let xml = build_xmp_xml(&metadata);

        assert!(xml.contains("Tom &amp; Jerry &lt;friends&gt;"));
    }

    #[test]
    fn test_parse_xmp_xml_roundtrip() {
        let mut original = HashMap::new();
        original.insert("description".to_string(), "A lovely sunset".to_string());
        original.insert("creator".to_string(), "John Doe".to_string());
        original.insert("customField".to_string(), "custom value".to_string());

        let xml = build_xmp_xml(&original);
        let parsed = parse_xmp_xml(&xml).unwrap();

        assert_eq!(parsed.get("description"), Some(&"A lovely sunset".to_string()));
        assert_eq!(parsed.get("creator"), Some(&"John Doe".to_string()));
        assert_eq!(parsed.get("customField"), Some(&"custom value".to_string()));
    }

    #[test]
    fn test_parse_xmp_xml_with_special_chars() {
        let mut original = HashMap::new();
        original.insert("description".to_string(), "Photo with <special> & \"chars\"".to_string());

        let xml = build_xmp_xml(&original);
        let parsed = parse_xmp_xml(&xml).unwrap();

        assert_eq!(
            parsed.get("description"),
            Some(&"Photo with <special> & \"chars\"".to_string())
        );
    }

    #[test]
    fn test_write_read_xmp_jpeg_roundtrip() {
        let jpeg_data = create_test_jpeg();
        let tmp = NamedTempFile::with_suffix(".jpg").unwrap();
        fs::write(tmp.path(), &jpeg_data).unwrap();

        let mut metadata = HashMap::new();
        metadata.insert("description".to_string(), "Family reunion 2024".to_string());
        metadata.insert("stackId".to_string(), "Family_0042".to_string());

        // Write XMP
        write_xmp_to_jpeg(tmp.path(), &metadata).unwrap();

        // Read back
        let read_metadata = read_xmp_from_jpeg(tmp.path()).unwrap();

        assert_eq!(
            read_metadata.get("description"),
            Some(&"Family reunion 2024".to_string())
        );
        assert_eq!(
            read_metadata.get("stackId"),
            Some(&"Family_0042".to_string())
        );
    }

    #[test]
    fn test_write_xmp_to_tiff_creates_sidecar() {
        let tmp = NamedTempFile::with_suffix(".tif").unwrap();
        fs::write(tmp.path(), b"fake tiff data").unwrap();

        let mut metadata = HashMap::new();
        metadata.insert("description".to_string(), "TIFF test".to_string());

        write_xmp_to_tiff(tmp.path(), &metadata).unwrap();

        let sidecar_path = tmp.path().with_extension("xmp");
        assert!(sidecar_path.exists());

        let sidecar_content = fs::read_to_string(&sidecar_path).unwrap();
        assert!(sidecar_content.contains("<dc:description>TIFF test</dc:description>"));

        // Clean up
        let _ = fs::remove_file(&sidecar_path);
    }

    #[test]
    fn test_read_xmp_from_tiff_with_sidecar() {
        let tmp = NamedTempFile::with_suffix(".tif").unwrap();
        fs::write(tmp.path(), b"fake tiff data").unwrap();

        let sidecar_path = tmp.path().with_extension("xmp");
        let mut metadata = HashMap::new();
        metadata.insert("description".to_string(), "Sidecar test".to_string());
        fs::write(&sidecar_path, build_xmp_xml(&metadata)).unwrap();

        let read_metadata = read_xmp_from_tiff(tmp.path()).unwrap();

        assert_eq!(
            read_metadata.get("description"),
            Some(&"Sidecar test".to_string())
        );

        // Clean up
        let _ = fs::remove_file(&sidecar_path);
    }

    #[test]
    fn test_read_xmp_from_jpeg_no_xmp() {
        let jpeg_data = create_test_jpeg();
        let tmp = NamedTempFile::with_suffix(".jpg").unwrap();
        fs::write(tmp.path(), &jpeg_data).unwrap();

        let metadata = read_xmp_from_jpeg(tmp.path()).unwrap();
        assert!(metadata.is_empty());
    }

    #[test]
    fn test_read_xmp_from_tiff_no_sidecar() {
        let tmp = NamedTempFile::with_suffix(".tif").unwrap();
        fs::write(tmp.path(), b"fake tiff data").unwrap();

        let metadata = read_xmp_from_tiff(tmp.path()).unwrap();
        assert!(metadata.is_empty());
    }

    #[test]
    fn test_write_xmp_unsupported_format() {
        let tmp = NamedTempFile::with_suffix(".png").unwrap();
        fs::write(tmp.path(), b"fake png data").unwrap();

        let metadata = HashMap::new();
        let result = write_xmp(tmp.path(), &metadata);

        assert!(matches!(result, Err(XmpError::UnsupportedFormat(_))));
    }

    #[test]
    fn test_generic_write_read_xmp() {
        let jpeg_data = create_test_jpeg();
        let tmp = NamedTempFile::with_suffix(".jpg").unwrap();
        fs::write(tmp.path(), &jpeg_data).unwrap();

        let mut metadata = HashMap::new();
        metadata.insert("title".to_string(), "Generic test".to_string());

        write_xmp(tmp.path(), &metadata).unwrap();
        let read_metadata = read_xmp(tmp.path()).unwrap();

        assert_eq!(read_metadata.get("title"), Some(&"Generic test".to_string()));
    }

    #[test]
    fn test_map_key_to_namespace() {
        assert_eq!(map_key_to_namespace("description"), ("dc", "description"));
        assert_eq!(map_key_to_namespace("Description"), ("dc", "description"));
        assert_eq!(map_key_to_namespace("ImageDescription"), ("dc", "description"));
        assert_eq!(map_key_to_namespace("creator"), ("dc", "creator"));
        assert_eq!(map_key_to_namespace("Artist"), ("dc", "creator"));
        assert_eq!(map_key_to_namespace("customTag"), ("photostax", "customTag"));
    }

    #[test]
    fn test_overwrite_existing_xmp() {
        let jpeg_data = create_test_jpeg();
        let tmp = NamedTempFile::with_suffix(".jpg").unwrap();
        fs::write(tmp.path(), &jpeg_data).unwrap();

        // Write initial XMP
        let mut metadata1 = HashMap::new();
        metadata1.insert("description".to_string(), "First description".to_string());
        write_xmp_to_jpeg(tmp.path(), &metadata1).unwrap();

        // Overwrite with new XMP
        let mut metadata2 = HashMap::new();
        metadata2.insert("description".to_string(), "Second description".to_string());
        metadata2.insert("title".to_string(), "New title".to_string());
        write_xmp_to_jpeg(tmp.path(), &metadata2).unwrap();

        // Read and verify
        let read_metadata = read_xmp_from_jpeg(tmp.path()).unwrap();
        assert_eq!(
            read_metadata.get("description"),
            Some(&"Second description".to_string())
        );
        assert_eq!(read_metadata.get("title"), Some(&"New title".to_string()));
    }
}
